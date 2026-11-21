#![cfg(not(target_arch = "wasm32"))]

mod discovery;
mod media;
mod transport;
mod wire;

pub use discovery::discover_lan;
pub use transport::{transport_bytes, ClientMedia, Counters, Dsp, ServerMedia};
pub use wire::{Config, MediaLimits};

mod control;
mod server;

pub use control::{
    describe, set_editor, shutdown, Action, ChainDescription, Description, Kind, Lifecycle, Status,
    CONTROL_VERSION, DEFAULT_COPIES,
};
pub use server::{serve, Chain, ChainControls};

mod client;
pub use client::{Client, Observation, Reconnect, RenderEndpoint, Ticket};
