use anyhow::Result;
use audio_midi_io_traits::{
    has_audio_processing_function::HasAudioProcessingFunction,
    has_command_queue::{Command, HasCommandQueueSender},
};
use common::logging::macros::*;
use lockfree_queue::{Receiver, Sender};
use std::collections::LinkedList;
use std::{
    sync::{atomic::AtomicU32, Arc},
    time::Duration,
};
shoop_log_unit!("Backend.AudioMidiHost");

#[derive(Default)]
struct HostSharedState {
    n_last_processed: AtomicU32,
}

pub struct HostProcessor {
    shared_state: Arc<HostSharedState>,
    cmd_receiver: Receiver<Command<HostProcessor>>,
    processors: LinkedList<Box<dyn HasAudioProcessingFunction>>,
}

pub struct Host {
    shared_state: Arc<HostSharedState>,
    cmd_sender: Sender<Command<HostProcessor>>,
}

impl HasCommandQueueSender<HostProcessor> for Host {
    fn process_command_sender<'a>(&'a self) -> &'a Sender<Command<HostProcessor>> {
        &self.cmd_sender
    }
}

impl HostProcessor {
    fn new(
        shared_state: Arc<HostSharedState>,
        cmd_receiver: Receiver<Command<HostProcessor>>,
    ) -> Self {
        Self {
            shared_state: shared_state,
            cmd_receiver: cmd_receiver,
            processors: LinkedList::default(),
        }
    }
}

impl HasAudioProcessingFunction for HostProcessor {
    fn process(&mut self, nframes: usize) -> Result<()> {
        while let Ok(cmd) = self.cmd_receiver.recv() {
            if let Err(e) = cmd(self) {
                error!("Failed to execute command on process thread: {e}");
            }
        }
        for processor in self.processors.iter_mut() {
            processor.process(nframes)?;
        }
        Ok(())
    }
}

impl Host {
    /// Create a new audio/midi host. How to create/access the processing thread
    /// is up to the caller.
    /// This function takes a function which it will call once during the execution
    /// of new(). It yields a callback which the caller can use on another thread
    /// to create a single processor on any thread except the thread this Host is
    /// used on.
    /// The caller should ensure that the creation happens before the callback
    /// returns.
    /// The caller should also ensure that the processor's process() function is
    /// called regularly from the audio thread.
    /// These conditions are checked by this function immediately trying to queue
    /// and wait for a test command sent to the new processor over a channel.
    /// if the command times out (optional), creation is deemed to have failed.
    pub fn new(
        create_processor_fn: impl FnOnce(Box<dyn FnOnce() -> Result<HostProcessor> + Send>),
        process_check_timeout: Option<Duration>,
    ) -> Result<Self> {
        let shared_state = Arc::new(HostSharedState::default());
        let (sender, receiver) = lockfree_queue::create()?;

        let obj = Self {
            shared_state: shared_state.clone(),
            cmd_sender: sender,
        };

        let create_processor = move || {
            let processor = HostProcessor::new(shared_state, receiver);
            Ok(processor)
        };
        create_processor_fn(Box::new(create_processor));

        // By now, the processor should have been created on some other
        // thread. Test this by sending a test command.
        if let Some(timeout) = process_check_timeout {
            obj.wait_process(timeout)?;
        }

        Ok(obj)
    }
}
