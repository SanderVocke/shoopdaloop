//! Experimental command groups. Identities and epochs are shared with snapshots.
use crate::{BusId, LoopSlotId, SlotId, StripId};
pub const COMMANDS: usize = 32;
pub const FIFO_BATCHES: usize = 4;
pub const STAGED_BATCHES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    LoopGain(LoopSlotId, f32),
    LoopBalance(LoopSlotId, f32),
    Start(LoopSlotId),
    Stop(LoopSlotId),
    InputGain(StripId, f32),
    InputBalance(StripId, f32),
    OutputGain(StripId, f32),
    OutputBalance(StripId, f32),
    OutputMute(StripId, bool),
    BusGain(BusId, f32),
    BusBalance(BusId, f32),
    BusMute(BusId, bool),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Batch {
    pub graph: u64,
    pub topology: u64,
    len: u8,
    commands: [Command; COMMANDS],
}
impl Batch {
    pub fn new(graph: u64, topology: u64, commands: &[Command]) -> Option<Self> {
        if commands.is_empty() || commands.len() > COMMANDS {
            return None;
        }
        let mut batch = Self {
            graph,
            topology,
            len: commands.len() as u8,
            commands: [Command::Start(LoopSlotId(SlotId::new(0, 0))); COMMANDS],
        };
        batch.commands[..commands.len()].copy_from_slice(commands);
        batch.valid().then_some(batch)
    }
    pub fn commands(&self) -> &[Command] {
        &self.commands[..usize::from(self.len)]
    }
    pub fn valid(&self) -> bool {
        self.commands().iter().enumerate().all(|(i, command)| {
            command.valid()
                && command.playback().is_none_or(|id| {
                    !self.commands()[..i]
                        .iter()
                        .any(|previous| previous.playback() == Some(id))
                })
        })
    }
}
impl Command {
    pub fn playback(self) -> Option<LoopSlotId> {
        match self {
            Self::Start(id) | Self::Stop(id) => Some(id),
            _ => None,
        }
    }
    pub fn valid(self) -> bool {
        match self {
            Self::LoopGain(_, v)
            | Self::InputGain(_, v)
            | Self::OutputGain(_, v)
            | Self::BusGain(_, v) => v.is_finite(),
            Self::LoopBalance(_, v)
            | Self::InputBalance(_, v)
            | Self::OutputBalance(_, v)
            | Self::BusBalance(_, v) => (-1.0..=1.0).contains(&v),
            _ => true,
        }
    }
}
