//! Prepared headless loop/DSP composition with one render owner.
#![forbid(unsafe_code)]

mod control;
mod preparation;
mod render;
mod types;

pub use control::{Client, Ticket};
pub use preparation::{prepare, prepare_with_content};
pub use render::{Render, Stopped};
pub use shoop_example_dsp as dsp;
pub use shoop_loops::{media, runtime::Operation as LoopOperation, runtime::Reply as LoopReply};
pub use types::*;
