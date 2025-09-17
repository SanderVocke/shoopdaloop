use std::sync::atomic::AtomicU32;

use audio_midi_io_traits::has_command_queue::HasCommandQueueSender;

struct HostSharedState {
    n_last_processed: AtomicU32,
}

struct Host {
    shared_state: Arc<HostSharedState>,
    cmd_sender: Sender<HostProcessor>,
}

struct HostProcessor {}

impl HasCommandQueueSender<HostProcessor> for Host {
    fn process_command_sender<'a>(&'a self) -> &'a Sender<HostProcessor> {
        &self.cmd_sender
    }
}
