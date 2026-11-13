//! Engine compatibility exports for the shared record carrier.
#![cfg(not(target_arch = "wasm32"))]

pub use shoop_transport_shared_memory::*;
pub mod client {
    pub type Client = shoop_rt_transport::records::RecordClient<super::Port>;
}
pub mod server {
    pub type Server<E> = shoop_rt_transport::records::RecordServer<E, super::Port>;
}
