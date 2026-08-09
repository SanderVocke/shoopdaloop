use shoop_app::{ApplicationAudioPreview, ApplicationHandle};
use shoop_egui::AppIntent;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::JoinHandle;
use std::time::Duration;

enum PreviewCommand {
    Play(ApplicationAudioPreview),
    Shutdown,
}

pub struct NativePreviewPlayer {
    sender: SyncSender<PreviewCommand>,
    worker: Option<JoinHandle<()>>,
    handle: ApplicationHandle,
}

impl NativePreviewPlayer {
    pub fn new(handle: ApplicationHandle) -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker_handle = handle.clone();
        let worker = std::thread::Builder::new()
            .name("shoop-click-preview".to_owned())
            .spawn(move || run_worker(receiver, worker_handle))?;
        Ok(Self {
            sender,
            worker: Some(worker),
            handle,
        })
    }

    pub fn play(&self, preview: ApplicationAudioPreview) {
        let request_id = preview.request_id;
        if preview.sample_rate == 0 || preview.samples.is_empty() {
            complete(
                &self.handle,
                request_id,
                false,
                "Click preview has no playable audio".to_owned(),
            );
            return;
        }
        if let Err(error) = self.sender.try_send(PreviewCommand::Play(preview)) {
            let message = match error {
                TrySendError::Full(_) => "Another click preview is already queued",
                TrySendError::Disconnected(_) => "Click preview worker is unavailable",
            };
            complete(&self.handle, request_id, false, message.to_owned());
        }
    }
}

impl Drop for NativePreviewPlayer {
    fn drop(&mut self) {
        let _ = self.sender.send(PreviewCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(receiver: Receiver<PreviewCommand>, handle: ApplicationHandle) {
    let mut next = None;
    loop {
        let command = match next.take() {
            Some(command) => command,
            None => match receiver.recv() {
                Ok(command) => command,
                Err(_) => return,
            },
        };
        let PreviewCommand::Play(preview) = command else {
            return;
        };
        let request_id = preview.request_id;
        let playback = start_playback(&preview);
        let (mut stream, sink) = match playback {
            Ok(playback) => playback,
            Err(error) => {
                complete(&handle, request_id, false, error.to_string());
                continue;
            }
        };
        stream.log_on_drop(false);
        loop {
            match receiver.try_recv() {
                Ok(PreviewCommand::Play(replacement)) => {
                    sink.stop();
                    complete(
                        &handle,
                        request_id,
                        false,
                        "Click preview superseded".to_owned(),
                    );
                    next = Some(PreviewCommand::Play(replacement));
                    break;
                }
                Ok(PreviewCommand::Shutdown) | Err(TryRecvError::Disconnected) => {
                    sink.stop();
                    return;
                }
                Err(TryRecvError::Empty) => {}
            }
            if sink.empty() {
                complete(
                    &handle,
                    request_id,
                    true,
                    "Click preview completed".to_owned(),
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn start_playback(
    preview: &ApplicationAudioPreview,
) -> anyhow::Result<(rodio::OutputStream, rodio::Sink)> {
    let stream = rodio::OutputStreamBuilder::open_default_stream()
        .map_err(|error| anyhow::anyhow!("Could not open preview output: {error}"))?;
    let sink = rodio::Sink::connect_new(stream.mixer());
    sink.append(rodio::buffer::SamplesBuffer::new(
        1,
        preview.sample_rate,
        preview.samples.to_vec(),
    ));
    sink.play();
    Ok((stream, sink))
}

fn complete(handle: &ApplicationHandle, request_id: u64, success: bool, message: String) {
    let _ = handle.dispatch(AppIntent::CompleteClickTrackPreview {
        request_id,
        success,
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_backend::FakeBackend;
    use std::sync::Arc;

    #[test]
    fn invalid_preview_fails_without_opening_hardware() {
        let runtime =
            shoop_app::ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        let started = std::time::Instant::now();
        let loop_id = loop {
            let snapshot = handle.snapshot();
            if snapshot.status.sample_rate > 0 {
                break snapshot.tracks[0].loops[0].id;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        };
        handle
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id,
                request: shoop_egui::ClickTrackRequest::default(),
            })
            .unwrap();
        let preview = loop {
            if let Some(preview) = handle.take_audio_preview() {
                break preview;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        };
        let request_id = preview.request_id;
        let player = NativePreviewPlayer::new(handle.clone()).unwrap();
        player.play(ApplicationAudioPreview {
            request_id,
            sample_rate: 0,
            samples: Arc::from([]),
        });
        while handle.snapshot().click_track.preview_status
            != shoop_egui::ClickTrackPreviewStatus::Failed
        {
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(handle.snapshot().click_track.preview_request_id, request_id);
    }
}
