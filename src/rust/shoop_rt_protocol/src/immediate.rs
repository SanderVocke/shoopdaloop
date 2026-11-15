//! Experimental command groups. Identities and epochs are shared with snapshots.
pub const COMMANDS: usize = 32;
pub const FIFO_RECORDS: usize = 256;
pub const FIFO_SINGLETONS: usize = FIFO_RECORDS / 2;
pub const STAGED_BATCHES: usize = 4;

/// Cumulative refusals, never a receipt for an individual group.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    pub control_refused: u32,
    pub rt_refused: u32,
    /// 0: none, 1: unavailable/busy, 2: configuration, 3: target,
    /// 4: unsupported state, 5: invalid value.
    pub last_control: u8,
    pub last_rt: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    LoopGain(u32, f32),
    LoopBalance(u32, f32),
    Start(u32),
    Stop(u32),
    InputGain(u32, f32),
    InputBalance(u32, f32),
    OutputGain(u32, f32),
    OutputBalance(u32, f32),
    OutputMute(u32, bool),
    BusGain(u32, f32),
    BusBalance(u32, f32),
    BusMute(u32, bool),
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
            commands: [Command::Start(0); COMMANDS],
        };
        batch.commands[..commands.len()].copy_from_slice(commands);
        Some(batch)
    }
    pub fn commands(&self) -> &[Command] {
        &self.commands[..usize::from(self.len)]
    }
}
impl Command {
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
