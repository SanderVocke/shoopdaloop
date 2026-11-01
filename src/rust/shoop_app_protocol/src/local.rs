//! Scheduled typed delivery: no encoding, recursive dispatch or same-thread waits.

use crate::{ClientMessage, ServiceMessage, QUEUE_CAPACITY};
use std::sync::mpsc::{self, Receiver, SyncSender};

pub struct ClientPort {
    pub commands: SyncSender<ClientMessage>,
    pub updates: Receiver<ServiceMessage>,
}

pub struct ServicePort {
    pub commands: Receiver<ClientMessage>,
    pub updates: SyncSender<ServiceMessage>,
}

pub fn channel() -> (ClientPort, ServicePort) {
    let (commands, requests) = mpsc::sync_channel(QUEUE_CAPACITY);
    let (updates, responses) = mpsc::sync_channel(QUEUE_CAPACITY);
    (
        ClientPort {
            commands,
            updates: responses,
        },
        ServicePort {
            commands: requests,
            updates,
        },
    )
}
