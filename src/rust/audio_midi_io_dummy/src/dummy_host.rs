use std::{os::unix::thread, sync::mpsc::Sender, time::Duration};

use anyhow::Result;
use audio_midi_io_common::host::{Host, HostProcessor};
use audio_midi_io_traits::has_audio_processing_function::HasAudioProcessingFunction;
use common::logging::macros::*;
shoop_log_unit!("Backend.DummyAudioMidiHost");

/// The processing thread function for this host.
fn dummy_processing_thread(
    mut processor: DummyHostProcessor,
    config: DummyProcessingThreadConfig,
) {
    if let Err(e) = || -> Result<()> {
        

        loop {
            // Wait until the next processing interval.
            std::thread::sleep(config.process_interval);

            // Process the audio.
            if let Err(e) = processor.process(config.process_frames_per_iteration) {
                error!("Process iteration failed: {e}");
            }
        }
    }() {
        error!("Dummy processing thead: error: {e}");
    }
}

#[derive(Clone)]
pub struct DummyProcessingThreadConfig {
    process_interval : Duration,
    process_frames_per_iteration : usize,
}

pub struct DummyHostConfig {
    thread_config: DummyProcessingThreadConfig,
}

pub struct DummyHost {
    base: Host,
    config: DummyHostConfig,
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
    pub fn new(config: DummyHostConfig) -> Result<Self, anyhow::Error> {
        // To be called by base Host::new, which we should then use to create
        // a process thread and processor on it.
        let thread_config = config.thread_config.clone();

        // channel we can use to wait until the process thread is up and running
        let (ready_rendezvous_sender, ready_rendezvous_receiver) = std::sync::mpsc::channel();

        let create_base_host_processor_callback =
            |base_processor_factory: Box<dyn FnOnce() -> Result<HostProcessor> + Send>| {

                let processing_thread = move || {
                    if let Err(e) = || -> Result<()> {
                        // create a processor object.
                        let base_processor = base_processor_factory()?;
                        let dummy_processor = DummyHostProcessor { base: base_processor };
                        ready_rendezvous_sender.send(())?;
                        dummy_processing_thread(dummy_processor, thread_config);

                        Ok(())
                    }() {
                        todo!();
                    }
                };

                let join_handle = std::thread::spawn(processing_thread);

                todo!();
            };

        let base_host = Host::new(create_base_host_processor_callback, Some(Duration::from_secs(1)))?;

        // Wait until the processor creation is finished.
        ready_rendezvous_receiver.recv_timeout(Duration::from_secs(1))?;
        
        Ok(Self {
            base: base_host,
            config: config,
        })
    }
}
