#![cfg(not(target_arch = "wasm32"))]
#![deny(unsafe_code)]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Forwards graph/rate/xrun notifications and records shutdown without invoking application code.
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
    fn port_registration(&mut self, client: &jack::Client, port: jack::PortId, registered: bool) {
        self.handler.port_registration(client, port, registered);
    }
    fn port_rename(
        &mut self,
        client: &jack::Client,
        port: jack::PortId,
        old: &str,
        new: &str,
    ) -> jack::Control {
        self.handler.port_rename(client, port, old, new)
    }
    fn ports_connected(
        &mut self,
        client: &jack::Client,
        a: jack::PortId,
        b: jack::PortId,
        connected: bool,
    ) {
        self.handler.ports_connected(client, a, b, connected);
    }
    fn sample_rate(&mut self, client: &jack::Client, rate: jack::Frames) -> jack::Control {
        self.handler.sample_rate(client, rate)
    }
    fn xrun(&mut self, client: &jack::Client) -> jack::Control {
        self.handler.xrun(client)
    }
}

/// Reads the native registration index used by JACK port notifications, off the callback thread.
#[allow(unsafe_code)]
pub fn port_id<Spec>(client: &jack::Client, port: &jack::Port<Spec>) -> Option<jack::PortId> {
    if port.client_ptr() != client.raw() {
        return None;
    }
    // The matching borrowed client keeps JACK's shared port metadata mapped during these reads.
    Some(unsafe { jack_sys::jack_uuid_to_index(jack_sys::jack_port_uuid(port.raw())) })
}
