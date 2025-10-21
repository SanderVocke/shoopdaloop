use std::{sync::mpsc::Sender, time::Duration};

use anyhow::Result;
use audio_midi_io_common::host::{Host, HostProcessor};
use audio_midi_io_traits::has_audio_processing_function::HasAudioProcessingFunction;
use common::logging::macros::*;
shoop_log_unit!("Backend.DummyAudioMidiHost");

/// The processing thread function for this host.
fn dummy_processing_thread(
    processor_factory: impl FnOnce() -> Result<DummyHostProcessor> + Send,
    ready_rendezvous: Sender<()>,
    process_interval: Duration,
    process_frames_per_iteration: usize,
) {
    if let Err(e) = || -> Result<()> {
        // First create a processor object.
        let mut processor = processor_factory()?;

        ready_rendezvous.send(())?;

        loop {
            // Wait until the next processing interval.
            std::thread::sleep(process_interval);

            // Process the audio.
            if let Err(e) = processor.process(process_frames_per_iteration) {
                error!("Process iteration failed: {e}");
            }
        }
    }() {
        error!("Dummy processing thead: error: {e}");
    }
}

pub struct DummyHost {
    base: Host,
}

pub struct DummyHostProcessor {
    base: HostProcessor,
}

impl HasAudioProcessingFunction for DummyHostProcessor {
    fn process(&mut self, nframes: usize) -> Result<()> {
        self.base.process(nframes)
    }
}

impl DummyHost {
    pub fn new(
        create_processor_callback: impl FnOnce(Box<dyn FnOnce() -> Result<HostProcessor> + Send>),
        process_check_timeout: Option<Duration>,
    ) -> Result<Self, anyhow::Error> {
        Ok(DummyHost {
            base : Host::new(create_processor_callback, process_check_timeout)?,
        })
    }
}
