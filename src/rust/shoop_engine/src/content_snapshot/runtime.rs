use super::{
    audio_snapshot_channel, midi_snapshot_channel, AudioProcessSnapshotWriter,
    AudioSnapshotControl, AudioSnapshotPublisher, AudioSnapshotReader, MidiProcessSnapshotWriter,
    MidiSnapshotControl, MidiSnapshotPublisher, MidiSnapshotReader, SessionContentEpoch,
};
use crate::realtime_lock_guard::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const PUBLISHER_POLL_INTERVAL: Duration = Duration::from_millis(1);

enum RuntimeCommand {
    AddAudio(AudioSnapshotPublisher),
    AddMidi(MidiSnapshotPublisher),
    Stop,
}

struct RuntimeInner {
    commands: Sender<RuntimeCommand>,
    epoch: Arc<SessionContentEpoch>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct ContentSnapshotRuntime {
    inner: Arc<RuntimeInner>,
}

impl Default for ContentSnapshotRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentSnapshotRuntime {
    pub fn new() -> Self {
        let (commands, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("shoop-content-snapshots".to_string())
            .spawn(move || publisher_worker(receiver))
            .expect("content snapshot publisher worker must start");
        Self {
            inner: Arc::new(RuntimeInner {
                commands,
                epoch: Arc::new(SessionContentEpoch::default()),
                worker: Mutex::new(Some(worker)),
            }),
        }
    }

    pub fn create_audio_channel(
        &self,
        chunk_size: usize,
        transport_blocks: usize,
    ) -> (
        AudioProcessSnapshotWriter,
        AudioSnapshotControl,
        AudioSnapshotReader,
    ) {
        let (writer, control, publisher, reader) =
            audio_snapshot_channel(Arc::clone(&self.inner.epoch), chunk_size, transport_blocks);
        self.inner
            .commands
            .send(RuntimeCommand::AddAudio(publisher))
            .expect("content snapshot worker is alive");
        (writer, control, reader)
    }

    pub fn create_midi_channel(
        &self,
        block_events: usize,
        transport_blocks: usize,
    ) -> (
        MidiProcessSnapshotWriter,
        MidiSnapshotControl,
        MidiSnapshotReader,
    ) {
        let (writer, control, publisher, reader) = midi_snapshot_channel(
            Arc::clone(&self.inner.epoch),
            block_events,
            transport_blocks,
        );
        self.inner
            .commands
            .send(RuntimeCommand::AddMidi(publisher))
            .expect("content snapshot worker is alive");
        (writer, control, reader)
    }

    pub fn capture_epoch(&self) -> Option<u64> {
        self.inner.epoch.capture()
    }

    pub fn validate_epoch(&self, captured: u64) -> bool {
        self.inner.epoch.validate(captured)
    }
}

impl Drop for RuntimeInner {
    fn drop(&mut self) {
        let _ = self.commands.send(RuntimeCommand::Stop);
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            let _ = worker.join();
        }
    }
}

fn publisher_worker(commands: Receiver<RuntimeCommand>) {
    let mut audio_publishers = Vec::<AudioSnapshotPublisher>::new();
    let mut midi_publishers = Vec::<MidiSnapshotPublisher>::new();
    loop {
        match commands.recv_timeout(PUBLISHER_POLL_INTERVAL) {
            Ok(RuntimeCommand::AddAudio(publisher)) => audio_publishers.push(publisher),
            Ok(RuntimeCommand::AddMidi(publisher)) => midi_publishers.push(publisher),
            Ok(RuntimeCommand::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        while let Ok(command) = commands.try_recv() {
            match command {
                RuntimeCommand::AddAudio(publisher) => audio_publishers.push(publisher),
                RuntimeCommand::AddMidi(publisher) => midi_publishers.push(publisher),
                RuntimeCommand::Stop => return,
            }
        }
        audio_publishers.retain_mut(|publisher| {
            publisher.pump();
            !publisher.is_retired()
        });
        midi_publishers.retain_mut(|publisher| {
            publisher.pump();
            !publisher.is_retired()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_snapshot::{ContentMutation, ContentRevision};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let start = Instant::now();
        while !predicate() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "snapshot publisher did not converge"
            );
            thread::yield_now();
        }
    }

    #[test]
    fn one_worker_publishes_multiple_channel_streams() {
        let runtime = ContentSnapshotRuntime::new();
        let (mut first_writer, _first_control, first_reader) = runtime.create_audio_channel(4, 4);
        let (mut second_writer, _second_control, second_reader) =
            runtime.create_audio_channel(4, 4);

        assert!(first_writer.begin_mutation(ContentMutation::Loading));
        let first_revision = first_writer
            .publish_range(0, &[1.0, 2.0], 2, true)
            .expect("first publish");
        first_writer.finish_mutation(false);

        assert!(second_writer.begin_mutation(ContentMutation::Loading));
        let second_revision = second_writer
            .publish_range(0, &[8.0], 1, true)
            .expect("second publish");
        second_writer.finish_mutation(false);

        wait_until(|| {
            first_reader
                .try_current()
                .is_ok_and(|snapshot| snapshot.revision == first_revision)
                && second_reader
                    .try_current()
                    .is_ok_and(|snapshot| snapshot.revision == second_revision)
        });
        assert_eq!(first_reader.latest().snapshot.contiguous(), vec![1.0, 2.0]);
        assert_eq!(second_reader.latest().snapshot.contiguous(), vec![8.0]);
        assert_eq!(first_revision, ContentRevision(1));
        assert_eq!(second_revision, ContentRevision(1));
    }

    #[test]
    fn session_epoch_spans_all_registered_channels() {
        let runtime = ContentSnapshotRuntime::new();
        let (first, _first_control, _) = runtime.create_audio_channel(4, 2);
        let (second, _second_control, _) = runtime.create_audio_channel(4, 2);
        let stable = runtime.capture_epoch().expect("stable");
        assert!(first.begin_mutation(ContentMutation::Recording));
        assert!(runtime.capture_epoch().is_none());
        first.cancel_mutation();
        assert!(!runtime.validate_epoch(stable));
        let stable = runtime.capture_epoch().expect("stable again");
        assert!(second.begin_mutation(ContentMutation::Clearing));
        second.cancel_mutation();
        assert!(!runtime.validate_epoch(stable));
    }

    #[test]
    fn concurrent_readers_observe_only_complete_generations_under_stress() {
        let runtime = ContentSnapshotRuntime::new();
        let (mut writer, control, reader) = runtime.create_audio_channel(4, 8);
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let reader_thread = {
            let reader = reader.clone();
            let stop = Arc::clone(&stop);
            let failed = Arc::clone(&failed);
            thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let data = reader.latest().snapshot.contiguous();
                    if data.len() != 4 && !data.is_empty() {
                        failed.store(true, Ordering::Release);
                        return;
                    }
                    if let Some(first) = data.first() {
                        if data.iter().any(|sample| sample != first) {
                            failed.store(true, Ordering::Release);
                            return;
                        }
                    }
                }
            })
        };

        for value in 1..=10_000_u32 {
            assert!(writer.begin_mutation(ContentMutation::Loading));
            let samples = [value as f32; 4];
            while writer
                .publish_range(0, &samples, samples.len(), true)
                .is_none()
            {
                thread::yield_now();
            }
            writer.finish_mutation(false);
        }
        let recovery = control
            .prepare(&[10_000.0; 4], ContentMutation::Loading)
            .expect("prepare explicit recovery");
        while !writer.install_prepared(recovery) {
            thread::yield_now();
        }
        wait_until(|| {
            reader
                .try_current()
                .is_ok_and(|snapshot| snapshot.contiguous() == vec![10_000.0; 4])
        });
        stop.store(true, Ordering::Release);
        reader_thread.join().expect("reader thread");
        assert!(!failed.load(Ordering::Acquire));
    }

    #[test]
    fn a_reader_keeps_its_last_manifest_after_runtime_shutdown() {
        let reader = {
            let runtime = ContentSnapshotRuntime::new();
            let (mut writer, _control, reader) = runtime.create_audio_channel(4, 4);
            assert!(writer.begin_mutation(ContentMutation::Loading));
            writer
                .publish_range(0, &[3.0, 4.0], 2, true)
                .expect("publish");
            writer.finish_mutation(false);
            wait_until(|| reader.try_current().is_ok());
            reader
        };
        assert_eq!(reader.latest().snapshot.contiguous(), vec![3.0, 4.0]);
    }
}
