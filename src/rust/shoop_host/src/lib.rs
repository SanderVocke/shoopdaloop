//! Native session hosting and attachment adapters. The engine connection stays inside the host.
#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]
pub mod config;
pub mod connection;
pub mod engine;
pub mod launch;
mod private;
pub mod registry;
pub mod serve;

pub mod remote;
