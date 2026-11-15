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
    /// 4: unsupported state, 5: invalid value, 6: deadline missed.
    pub last_control: u8,
    pub last_rt: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TransitionMode {
    Stopped = 0,
    Playing = 1,
    Recording = 2,
}
impl TransitionMode {
    pub fn from_raw(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Stopped,
            1 => Self::Playing,
            2 => Self::Recording,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Grab {
    pub sync: Option<u32>,
    pub reverse_start_cycle: Option<u32>,
    pub cycles_length: Option<u32>,
    pub go_to_cycle: Option<u32>,
    pub mode: TransitionMode,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrabGesture {
    pub sync: u32,
    pub target: Option<u32>,
    pub cycles: u32,
    pub play_after_record: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    LoopGain(u32, f32),
    LoopBalance(u32, f32),
    Transition(u32, TransitionMode),
    Grab(u32, Grab),
    GrabGesture(u32, GrabGesture),
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
    /// Latest processing frame, inclusive. Application commands default to None.
    pub deadline: Option<u64>,
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
            deadline: None,
            len: commands.len() as u8,
            commands: [Command::Transition(0, TransitionMode::Playing); COMMANDS],
        };
        batch.commands[..commands.len()].copy_from_slice(commands);
        Some(batch)
    }
    pub fn commands(&self) -> &[Command] {
        &self.commands[..usize::from(self.len)]
    }
    pub fn with_deadline(mut self, deadline: u64) -> Self {
        self.deadline = Some(deadline);
        self
    }
    pub fn header(&self) -> Header {
        Header::new(self.graph, self.topology, self.deadline)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Code {
    Unprocessed = 0,
    Applied = 1,
    DeadlineMissed = 2,
    StaleConfiguration = 3,
    TargetUnavailable = 4,
    InvalidValue = 5,
    Unsupported = 6,
    CannotApply = 7,
    CaptureUnavailable = 8,
    HistoryUnavailable = 9,
}
impl Code {
    pub fn from_raw(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Unprocessed,
            1 => Self::Applied,
            2 => Self::DeadlineMissed,
            3 => Self::StaleConfiguration,
            4 => Self::TargetUnavailable,
            5 => Self::InvalidValue,
            6 => Self::Unsupported,
            7 => Self::CannotApply,
            8 => Self::CaptureUnavailable,
            9 => Self::HistoryUnavailable,
            _ => return None,
        })
    }
    pub fn diagnostic(self) -> Option<u8> {
        match self {
            Self::Unprocessed | Self::Applied => None,
            Self::StaleConfiguration => Some(2),
            Self::TargetUnavailable => Some(3),
            Self::Unsupported | Self::CannotApply => Some(4),
            Self::InvalidValue => Some(5),
            Self::DeadlineMissed => Some(6),
            Self::CaptureUnavailable | Self::HistoryUnavailable => Some(1),
        }
    }
}

/// Little-endian words: graph, topology, deadline-present, deadline, attempt frame,
/// header result (u32), reserved zero (u32). Applied means the header was accepted;
/// individual command results can still report failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Header([u8; 48]);
impl Header {
    pub fn new(graph: u64, topology: u64, deadline: Option<u64>) -> Self {
        let mut value = Self([0; 48]);
        value.0[..8].copy_from_slice(&graph.to_le_bytes());
        value.0[8..16].copy_from_slice(&topology.to_le_bytes());
        value.0[16..24].copy_from_slice(&u64::from(deadline.is_some()).to_le_bytes());
        value.0[24..32].copy_from_slice(&deadline.unwrap_or(0).to_le_bytes());
        value
    }
    pub fn graph(self) -> u64 {
        self.word(0)
    }
    pub fn topology(self) -> u64 {
        self.word(8)
    }
    pub fn deadline(self) -> Option<u64> {
        (self.word(16) == 1).then(|| self.word(24))
    }
    pub fn frame(self) -> u64 {
        self.word(32)
    }
    pub fn code(self) -> Code {
        Code::from_raw(u32::from_le_bytes(self.0[40..44].try_into().unwrap())).unwrap()
    }
    pub fn complete(mut self, frame: u64, code: Code) -> Self {
        self.0[32..40].copy_from_slice(&frame.to_le_bytes());
        self.0[40..44].copy_from_slice(&(code as u32).to_le_bytes());
        self
    }
    fn word(self, start: usize) -> u64 {
        u64::from_le_bytes(self.0[start..start + 8].try_into().unwrap())
    }
    pub fn bytes(&self) -> &[u8; 48] {
        &self.0
    }
    pub fn from_bytes(bytes: [u8; 48]) -> Option<Self> {
        let value = Self(bytes);
        (value.word(16) <= 1
            && (value.word(16) == 1 || value.word(24) == 0)
            && value.0[44..] == [0; 4]
            && Code::from_raw(u32::from_le_bytes(value.0[40..44].try_into().unwrap())).is_some())
        .then_some(value)
    }
}

/// Eight little-endian u32 words: opcode, target, five argument words, result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct CommandRecord([u8; 32]);
impl CommandRecord {
    pub fn new(command: Command) -> Self {
        use Command::*;
        let words = match command {
            Grab(id, grab) => [
                13,
                id,
                grab.sync.unwrap_or(0),
                grab.reverse_start_cycle.unwrap_or(0),
                grab.cycles_length.unwrap_or(0),
                grab.go_to_cycle.unwrap_or(0),
                u32::from(grab.sync.is_some())
                    | (u32::from(grab.reverse_start_cycle.is_some()) << 1)
                    | (u32::from(grab.cycles_length.is_some()) << 2)
                    | (u32::from(grab.go_to_cycle.is_some()) << 3)
                    | ((grab.mode as u32) << 4),
                0,
            ],
            GrabGesture(id, grab) => [
                14,
                id,
                grab.sync,
                grab.target.unwrap_or(0),
                grab.cycles,
                u32::from(grab.play_after_record),
                u32::from(grab.target.is_some()),
                0,
            ],
            _ => {
                let (op, id, value) = match command {
                    LoopGain(id, v) => (0, id, v.to_bits()),
                    LoopBalance(id, v) => (1, id, v.to_bits()),
                    Transition(id, mode) => (2, id, mode as u32),
                    InputGain(id, v) => (4, id, v.to_bits()),
                    InputBalance(id, v) => (5, id, v.to_bits()),
                    OutputGain(id, v) => (6, id, v.to_bits()),
                    OutputBalance(id, v) => (7, id, v.to_bits()),
                    OutputMute(id, v) => (8, id, u32::from(v)),
                    BusGain(id, v) => (9, id, v.to_bits()),
                    BusBalance(id, v) => (10, id, v.to_bits()),
                    BusMute(id, v) => (11, id, u32::from(v)),
                    Grab(..) | GrabGesture(..) => unreachable!(),
                };
                [op, id, value, 0, 0, 0, 0, 0]
            }
        };
        let mut record = Self([0; 32]);
        for (bytes, word) in record.0.chunks_exact_mut(4).zip(words) {
            bytes.copy_from_slice(&word.to_le_bytes());
        }
        record
    }
    fn word(self, index: usize) -> u32 {
        u32::from_le_bytes(self.0[index * 4..index * 4 + 4].try_into().unwrap())
    }
    fn decode(self) -> Option<Command> {
        use Command::*;
        let id = self.word(1);
        let value = self.word(2);
        Some(match self.word(0) {
            0 => LoopGain(id, f32::from_bits(value)),
            1 => LoopBalance(id, f32::from_bits(value)),
            2 => Transition(id, TransitionMode::from_raw(value)?),
            4 => InputGain(id, f32::from_bits(value)),
            5 => InputBalance(id, f32::from_bits(value)),
            6 => OutputGain(id, f32::from_bits(value)),
            7 => OutputBalance(id, f32::from_bits(value)),
            8 if value <= 1 => OutputMute(id, value == 1),
            9 => BusGain(id, f32::from_bits(value)),
            10 => BusBalance(id, f32::from_bits(value)),
            11 if value <= 1 => BusMute(id, value == 1),
            13 => {
                let flags = self.word(6);
                if flags & !63 != 0 {
                    return None;
                }
                Grab(
                    id,
                    self::Grab {
                        sync: (flags & 1 != 0).then_some(value),
                        reverse_start_cycle: (flags & 2 != 0).then_some(self.word(3)),
                        cycles_length: (flags & 4 != 0).then_some(self.word(4)),
                        go_to_cycle: (flags & 8 != 0).then_some(self.word(5)),
                        mode: TransitionMode::from_raw(flags >> 4)?,
                    },
                )
            }
            14 if self.word(5) <= 1 && self.word(6) <= 1 => GrabGesture(
                id,
                self::GrabGesture {
                    sync: value,
                    target: (self.word(6) == 1).then_some(self.word(3)),
                    cycles: self.word(4),
                    play_after_record: self.word(5) == 1,
                },
            ),
            _ => return None,
        })
    }
    pub fn command(self) -> Command {
        self.decode().unwrap()
    }
    pub fn code(self) -> Code {
        Code::from_raw(self.word(7)).unwrap()
    }
    pub fn complete(mut self, code: Code) -> Self {
        self.0[28..].copy_from_slice(&(code as u32).to_le_bytes());
        self
    }
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        let record = Self(bytes);
        let command = record.decode()?;
        (Self::new(command).0[..28] == bytes[..28] && Code::from_raw(record.word(7)).is_some())
            .then_some(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response {
    pub header: Header,
    len: u8,
    records: [CommandRecord; COMMANDS],
}
impl Response {
    pub fn new(header: Header, records: &[CommandRecord]) -> Option<Self> {
        if header.code() == Code::Unprocessed
            || records.is_empty()
            || records.len() > COMMANDS
            || records
                .iter()
                .any(|record| record.code() == Code::Unprocessed)
        {
            return None;
        }
        let mut response = Self {
            header,
            len: records.len() as u8,
            records: [CommandRecord::new(Command::Transition(0, TransitionMode::Playing));
                COMMANDS],
        };
        response.records[..records.len()].copy_from_slice(records);
        Some(response)
    }
    pub fn records(&self) -> &[CommandRecord] {
        &self.records[..usize::from(self.len)]
    }
}
impl Command {
    pub fn valid(self) -> bool {
        match self {
            Self::Grab(_, grab) => [
                grab.reverse_start_cycle,
                grab.cycles_length,
                grab.go_to_cycle,
            ]
            .into_iter()
            .flatten()
            .all(|v| v <= i32::MAX as u32),
            Self::GrabGesture(_, grab) => grab.cycles <= i32::MAX as u32,
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
