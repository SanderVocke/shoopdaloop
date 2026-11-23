//! Internal command/response FIFOs and complete-state publications.
use rt_runtime::{immediate, snapshot};
use shoop_rt_protocol::immediate::{CommandRecord, Header};

pub use rt_runtime::Error;
mod endpoint;
pub use endpoint::{Endpoint, Settings};
pub type CommandProducer = immediate::Producer<Header, CommandRecord>;
pub type CommandConsumer = immediate::Consumer<Header, CommandRecord>;
pub type SnapshotReader = snapshot::Reader;
pub type SnapshotPublisher = snapshot::Publisher;

pub struct Control {
    pub commands: CommandProducer,
    pub switches: Option<CommandProducer>,
    pub snapshots: SnapshotReader,
}
pub struct Execution {
    pub commands: CommandConsumer,
    pub switches: Option<CommandConsumer>,
    pub snapshots: SnapshotPublisher,
}

/// Allocate all FIFO and publication storage outside processing.
pub fn channel(
    command_records: usize,
    snapshot_slots: usize,
    snapshot_bytes: usize,
) -> Result<(Control, Execution), Error> {
    if command_records < 2 || snapshot_bytes < shoop_rt_protocol::snapshot::HEADER {
        return Err(Error::InvalidConfig);
    }
    let (snapshots, reader) = snapshot::channel(snapshot_slots, snapshot_bytes)?;
    let (commands, consumer) = immediate::paired_channel(command_records);
    Ok((
        Control {
            commands,
            switches: None,
            snapshots: reader,
        },
        Execution {
            commands: consumer,
            switches: None,
            snapshots,
        },
    ))
}

/// Single in-flight program handoff, independent of the ordinary control queue.
pub fn prepared_channel(
    command_records: usize,
    snapshot_slots: usize,
    snapshot_bytes: usize,
) -> Result<(Control, Execution), Error> {
    let (mut control, mut execution) = channel(command_records, snapshot_slots, snapshot_bytes)?;
    let (switches, consumer) = immediate::paired_channel(2);
    control.switches = Some(switches);
    execution.switches = Some(consumer);
    Ok((control, execution))
}
