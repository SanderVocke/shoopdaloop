//! Internal command/response FIFOs and complete-state publications.
use rt_runtime::{immediate, snapshot};
use shoop_rt_protocol::immediate::{Batch, CommandRecord, Header, Response, COMMANDS};

pub use rt_runtime::{Error, Frame};
pub type CommandProducer = immediate::Producer<Header, CommandRecord>;
pub type CommandConsumer = immediate::Consumer<Header, CommandRecord>;
pub type SnapshotReader = snapshot::Reader;
pub type SnapshotPublisher = snapshot::Publisher;

/// Nonblocking control-side access to the internal RT boundary.
pub trait ControlPort {
    fn submit(&mut self, batch: Batch) -> Result<(), Error>;
    fn response(&mut self) -> Option<Response>;
    /// Next unrendered processing frame, independent of host buffering.
    fn frame(&self) -> u64;
    fn snapshot(&self, bytes: &mut [u8]) -> Option<(u32, usize)>;
    fn snapshot_error(&self) -> Option<Error>;
}

pub struct Control {
    pub commands: CommandProducer,
    pub snapshots: SnapshotReader,
}
pub struct Execution {
    pub commands: CommandConsumer,
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
            snapshots: reader,
        },
        Execution {
            commands: consumer,
            snapshots,
        },
    ))
}
impl ControlPort for Control {
    fn submit(&mut self, batch: Batch) -> Result<(), Error> {
        let mut records = [CommandRecord::new(batch.commands()[0]); COMMANDS];
        for (record, &command) in records.iter_mut().zip(batch.commands()) {
            *record = CommandRecord::new(command);
        }
        self.commands
            .push(batch.header(), &records[..batch.commands().len()])
    }
    fn response(&mut self) -> Option<Response> {
        let group = self.commands.response()?;
        let mut records =
            [CommandRecord::new(shoop_rt_protocol::immediate::Command::BusMute(0, false));
                COMMANDS];
        let mut count = 0;
        for &record in group.commands() {
            records[count] = record;
            count += 1;
        }
        Some(Response::new(group.header, &records[..count]).expect("completed command group"))
    }
    fn frame(&self) -> u64 {
        self.commands.clock().frame().0
    }
    fn snapshot(&self, bytes: &mut [u8]) -> Option<(u32, usize)> {
        self.snapshots.read(bytes)
    }
    fn snapshot_error(&self) -> Option<Error> {
        self.snapshots.error()
    }
}
