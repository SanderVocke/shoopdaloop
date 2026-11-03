//! Fixed-size graph execution with direct render-owned state.
//!
//! Preparation validates and binds routing once. Callback maintenance runs once
//! around all ticks, independent of tick count. An execution fault is terminal;
//! no callback may be replayed after a processor has mutated its state.

mod graph;
mod host;
mod queue;
mod timing;
pub use graph::{Block, Config, Diagnostics, PreparedConfig, Processor, Runtime, State};
pub use host::BufferedHost;
pub use queue::{command_channel, CommandControl, CommandQueue, Receipt, Ticket};
pub use timing::{Quantum, Tick};
