#![cfg(not(target_arch = "wasm32"))]
#![deny(unsafe_code)]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Forwards rate/xrun notifications and records shutdown without invoking application code.
pub struct Notifications<N> {
    handler: N,
    shutdown: Arc<AtomicBool>,
}
impl<N> Notifications<N> {
    pub fn new(handler: N, shutdown: Arc<AtomicBool>) -> Self {
        Self { handler, shutdown }
    }
}
impl<N: jack::NotificationHandler> jack::NotificationHandler for Notifications<N> {
    #[allow(unsafe_code)]
    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        // Only a lock-free atomic store is permitted in the signal-style callback.
        self.shutdown.store(true, Ordering::Release);
    }
    fn sample_rate(&mut self, client: &jack::Client, rate: jack::Frames) -> jack::Control {
        self.handler.sample_rate(client, rate)
    }
    fn xrun(&mut self, client: &jack::Client) -> jack::Control {
        self.handler.xrun(client)
    }
}
