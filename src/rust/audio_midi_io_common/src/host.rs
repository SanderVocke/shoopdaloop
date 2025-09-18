use std::sync::{atomic::AtomicU32, Arc, Mutex};
use audio_midi_io_traits::has_command_queue::{Command, HasCommandQueueSender};
use lockfree_queue::{Receiver, Sender};
use anyhow::Result;

trait BlackBox {}

#[derive (Default)]
struct HostSharedState {
    n_last_processed: AtomicU32,
}

struct HostProcessor {
    shared_state: Arc<HostSharedState>,
}

impl BlackBox for HostProcessor {}

pub struct Host {
    shared_state: Arc<HostSharedState>,
    cmd_sender: Sender<Command<HostProcessor>>,
    processor_handle: Arc<dyn BlackBox>,
}

impl HasCommandQueueSender<HostProcessor> for Host {
    fn process_command_sender<'a>(&'a self) -> &'a Sender<Command<HostProcessor>> {
        &self.cmd_sender
    }
}

impl HostProcessor {
    fn new(shared_state: Arc<HostSharedState>, cmd_receiver: Receiver<Command<HostProcessor>>) -> Self {
        Self {
            shared_state: shared_state,
        }
    }
}

impl Host {
    pub fn new() -> Result<Self> {
        let shared_state = Arc::new(HostSharedState::default());

        let (sender, receiver) = lockfree_queue::create()?;

        let processor = HostProcessor::new(
            shared_state.clone(),
            receiver,
        );

        Ok(Self {
            shared_state: shared_state,
            cmd_sender: sender,
            processor_handle: Arc::new(processor),
        })
    }
}