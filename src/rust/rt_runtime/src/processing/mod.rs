//! Whole-quantum graph execution with direct render-owned state.
//!
//! Preparation validates and binds routing once. Callback maintenance runs once
//! around all ticks, independent of tick count. An execution fault is terminal;
//! no callback may be replayed after a processor has mutated its state.

mod graph;
mod host;
pub(crate) mod host_buffer;
mod operation;
mod publication;
mod queue;
mod routing;
mod timing;
pub use graph::{Block, Config, Diagnostics, PreparedConfig, Processor, Runtime, State};
pub use host::BufferedHost;
pub use operation::{NodeOperation, OperationContext, OperationControl};
pub use publication::{GraphControl, GraphReceipt, GraphRejected, GraphTicket};
pub use queue::{
    command_channel, command_channel_with_budget, CommandControl, CommandQueue, Receipt, Ticket,
};
pub use routing::{GraphCompiler, PreparedGraph};
pub use timing::{Quantum, Tick};
