use crate::application::{ApplicationAudioPreview, ApplicationHandle};
use shoop_egui::AppIntent;

fn complete(handle: &ApplicationHandle, request_id: u64, success: bool, message: String) {
    let _ = handle.dispatch(AppIntent::CompleteClickTrackPreview {
        request_id,
        success,
        message,
    });
}

#[cfg(feature = "cpal")]
pub use system::{is_available, NativePreviewPlayer};

#[cfg(not(feature = "cpal"))]
pub fn is_available() -> bool {
    false
}

#[cfg(not(feature = "cpal"))]
pub struct NativePreviewPlayer {
    handle: ApplicationHandle,
}

#[cfg(not(feature = "cpal"))]
impl NativePreviewPlayer {
    pub fn new(handle: ApplicationHandle) -> anyhow::Result<Self> {
        Ok(Self { handle })
    }
    pub fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }
    pub fn play(&self, preview: ApplicationAudioPreview) {
        complete(
            &self.handle,
            preview.request_id,
            false,
            "System preview support was disabled at build time".to_owned(),
        );
    }
}

#[cfg(feature = "cpal")]
mod system {
    use super::*;
    use std::cell::Cell;
    use std::num::{NonZeroU16, NonZeroU32};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
    use std::thread::JoinHandle;
    use std::time::Duration;

    enum PreviewCommand {
        Play(ApplicationAudioPreview),
        Stop(SyncSender<()>),
        Shutdown,
    }

    pub fn is_available() -> bool {
        rodio::DeviceSinkBuilder::from_default_device().is_ok()
    }

    pub struct NativePreviewPlayer {
        sender: SyncSender<PreviewCommand>,
        worker: Option<JoinHandle<()>>,
        request_id: Cell<Option<u64>>,
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
                request_id: Cell::new(None),
                handle,
            })
        }

        pub fn stop(&self) -> anyhow::Result<()> {
            if self.request_id.get().is_none() {
                return Ok(());
            }
            let (sender, receiver) = mpsc::sync_channel(1);
            self.sender.try_send(PreviewCommand::Stop(sender))?;
            receiver.recv_timeout(Duration::from_secs(2))?;
            self.request_id.set(None);
            Ok(())
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
            if let Err(error) = self.stop().and_then(|()| {
                self.sender
                    .try_send(PreviewCommand::Play(preview))
                    .map_err(Into::into)
            }) {
                complete(&self.handle, request_id, false, error.to_string());
            } else {
                self.request_id.set(Some(request_id));
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
        let mut active: Option<(rodio::MixerDeviceSink, rodio::Player, u64)> = None;
        loop {
            match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(PreviewCommand::Play(preview)) => {
                    active.take();
                    match start_playback(&preview) {
                        Ok((mut stream, player)) => {
                            stream.log_on_drop(false);
                            let request_id = preview.request_id;
                            active = Some((stream, player, request_id));
                            let _ =
                                handle.dispatch(AppIntent::ClickTrackPreviewStarted { request_id });
                        }
                        Err(error) => {
                            complete(&handle, preview.request_id, false, error.to_string())
                        }
                    }
                }
                Ok(PreviewCommand::Stop(ack)) => {
                    active.take();
                    let _ = ack.send(());
                }
                Ok(PreviewCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if active.as_ref().is_some_and(|(_, player, _)| player.empty()) {
                let (_, _, request_id) = active.take().expect("active preview");
                complete(
                    &handle,
                    request_id,
                    true,
                    "Click preview completed".to_owned(),
                );
            }
        }
    }

    fn start_playback(
        preview: &ApplicationAudioPreview,
    ) -> anyhow::Result<(rodio::MixerDeviceSink, rodio::Player)> {
        let stream = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|error| anyhow::anyhow!("Could not open preview output: {error}"))?;
        let sink = rodio::Player::connect_new(stream.mixer());
        let sample_rate = NonZeroU32::new(preview.sample_rate)
            .ok_or_else(|| anyhow::anyhow!("Click preview sample rate is zero"))?;
        sink.append(rodio::buffer::SamplesBuffer::new(
            NonZeroU16::MIN,
            sample_rate,
            preview.samples.to_vec(),
        ));
        sink.play();
        Ok((stream, sink))
    }
}
