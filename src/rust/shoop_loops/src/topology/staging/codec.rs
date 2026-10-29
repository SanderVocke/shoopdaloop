use super::*;
use crate::program::ProgramKind;

pub const RECORD_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Counts(pub [usize; 7]);
impl Counts {
    pub fn total(self) -> usize {
        self.0.iter().sum()
    }
    pub fn locate(self, mut index: usize) -> Option<(usize, usize)> {
        for (kind, count) in self.0.into_iter().enumerate() {
            if index < count {
                return Some((kind, index));
            }
            index -= count;
        }
        None
    }
    pub fn of(topology: &PreparedTopology) -> Self {
        Self([
            topology.nodes.len(),
            topology.triggers.len(),
            topology.programs.len(),
            topology.targets.len(),
            topology.segments.len(),
            topology.components.len(),
            topology.members.len(),
        ])
    }
    pub fn maximum(limits: Limits) -> Self {
        Self([
            limits.loop_slots,
            limits.relations,
            limits.loop_slots,
            limits.program_entries,
            limits.program_segments,
            limits.loop_slots,
            limits.loop_slots,
        ])
    }
}
fn put32(bytes: &mut [u8; RECORD_BYTES], at: usize, value: usize) {
    bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn get32(bytes: &[u8; RECORD_BYTES], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn put64(bytes: &mut [u8; RECORD_BYTES], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn get64(bytes: &[u8; RECORD_BYTES], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn put_id(bytes: &mut [u8; RECORD_BYTES], at: usize, value: LoopId) {
    put32(bytes, at, value.slot() as usize);
    put32(bytes, at + 4, value.generation() as usize);
}
fn get_id(bytes: &[u8; RECORD_BYTES], at: usize) -> Result<LoopId, Error> {
    LoopId::from_parts(get32(bytes, at) as u32, get32(bytes, at + 4) as u32)
}
fn put_range(bytes: &mut [u8; RECORD_BYTES], at: usize, value: TableRange) {
    put32(bytes, at, value.start);
    put32(bytes, at + 4, value.len);
}
fn get_range(bytes: &[u8; RECORD_BYTES], at: usize) -> TableRange {
    TableRange {
        start: get32(bytes, at),
        len: get32(bytes, at + 4),
    }
}
fn mode(value: u8) -> Result<Mode, Error> {
    match value {
        0 => Ok(Mode::Stopped),
        1 => Ok(Mode::Playing),
        2 => Ok(Mode::Recording),
        _ => Err(Error::InvalidProgram),
    }
}
fn mode_byte(value: Mode) -> u8 {
    match value {
        Mode::Stopped => 0,
        Mode::Playing => 1,
        Mode::Recording => 2,
    }
}

pub(super) fn encode_header(
    limits: Limits,
    counts: Counts,
    relations: usize,
    bytes: &mut [u8; RECORD_BYTES],
) {
    bytes.fill(0);
    bytes[..4].copy_from_slice(b"LTP1");
    put32(bytes, 4, counts.total());
    for (index, value) in [
        limits.loop_slots,
        limits.relations,
        limits.program_entries,
        limits.program_segments,
        limits.pending_requests,
        limits.journal_events,
    ]
    .into_iter()
    .enumerate()
    {
        put32(bytes, 8 + 4 * index, value);
    }
    for (index, value) in counts.0.into_iter().enumerate() {
        put32(bytes, 32 + 4 * index, value);
    }
    put32(bytes, 60, relations);
}
pub(super) fn decode_header(
    limits: Limits,
    bytes: &[u8; RECORD_BYTES],
) -> Result<(Counts, usize), Error> {
    if &bytes[..4] != b"LTP1" {
        return Err(Error::InvalidProgram);
    }
    let expected = [
        limits.loop_slots,
        limits.relations,
        limits.program_entries,
        limits.program_segments,
        limits.pending_requests,
        limits.journal_events,
    ];
    if expected
        .into_iter()
        .enumerate()
        .any(|(index, value)| get32(bytes, 8 + 4 * index) != value)
    {
        return Err(Error::InvalidConfig);
    }
    let counts = Counts(std::array::from_fn(|index| get32(bytes, 32 + 4 * index)));
    if counts
        .0
        .into_iter()
        .zip(Counts::maximum(limits).0)
        .any(|(value, max)| value > max)
        || get32(bytes, 60) > limits.relations
    {
        return Err(Error::Capacity);
    }
    if counts.total() != get32(bytes, 4)
        || counts.0[2] > counts.0[0]
        || counts.0[5] > counts.0[0]
        || counts.0[6] != counts.0[0]
    {
        return Err(Error::InvalidProgram);
    }
    Ok((counts, get32(bytes, 60)))
}

#[derive(Clone, Copy)]
pub(super) enum Record {
    Node(LoopSpec),
    Trigger(Trigger),
    Program(ProgramRecord),
    Target(TargetRecord),
    Segment(Segment),
    Component(TableRange),
    Member(LoopId),
}
impl Record {
    pub fn encode(self, bytes: &mut [u8; RECORD_BYTES]) {
        bytes.fill(0);
        match self {
            Self::Node(node) => {
                put_id(bytes, 0, node.id);
                bytes[8] = match node.kind {
                    LoopKind::Primitive => 0,
                    LoopKind::Composite => 1,
                };
                put64(bytes, 16, node.length);
                if let Some(source) = node.sync_source {
                    bytes[9] = 1;
                    put_id(bytes, 24, source);
                }
            }
            Self::Trigger(trigger) => {
                put_id(bytes, 0, trigger.source);
                put_id(bytes, 8, trigger.target);
                bytes[16] = mode_byte(trigger.mode);
            }
            Self::Program(program) => {
                put_id(bytes, 0, program.source);
                bytes[8] = match program.kind {
                    ProgramKind::Regular => 0,
                    ProgramKind::Explicit => 1,
                };
                put64(bytes, 16, program.nominal_unit_frames);
                put64(bytes, 24, program.iterations);
                put_range(bytes, 32, program.targets);
                put_range(bytes, 40, program.segments);
            }
            Self::Target(target) => {
                put_id(bytes, 0, target.target);
                put_range(bytes, 8, target.segments);
            }
            Self::Segment(segment) => {
                put64(bytes, 0, segment.start);
                put64(bytes, 8, segment.end);
                put64(bytes, 16, segment.occurrence_start);
                put32(bytes, 24, segment.ordinal as usize);
                bytes[28] = mode_byte(segment.mode);
            }
            Self::Component(range) => put_range(bytes, 0, range),
            Self::Member(id) => put_id(bytes, 0, id),
        }
    }
    pub fn decode(kind: usize, input: &[u8]) -> Result<Self, Error> {
        let bytes: &[u8; RECORD_BYTES] = input.try_into().map_err(|_| Error::InvalidProgram)?;
        let record = match kind {
            0 => Self::Node(LoopSpec {
                id: get_id(bytes, 0)?,
                kind: match bytes[8] {
                    0 => LoopKind::Primitive,
                    1 => LoopKind::Composite,
                    _ => return Err(Error::InvalidProgram),
                },
                length: get64(bytes, 16),
                sync_source: match bytes[9] {
                    0 => None,
                    1 => Some(get_id(bytes, 24)?),
                    _ => return Err(Error::InvalidProgram),
                },
            }),
            1 => Self::Trigger(Trigger {
                source: get_id(bytes, 0)?,
                target: get_id(bytes, 8)?,
                mode: mode(bytes[16])?,
            }),
            2 => Self::Program(ProgramRecord {
                source: get_id(bytes, 0)?,
                kind: match bytes[8] {
                    0 => ProgramKind::Regular,
                    1 => ProgramKind::Explicit,
                    _ => return Err(Error::InvalidProgram),
                },
                nominal_unit_frames: get64(bytes, 16),
                iterations: get64(bytes, 24),
                targets: get_range(bytes, 32),
                segments: get_range(bytes, 40),
            }),
            3 => Self::Target(TargetRecord {
                target: get_id(bytes, 0)?,
                segments: get_range(bytes, 8),
            }),
            4 => Self::Segment(Segment {
                start: get64(bytes, 0),
                end: get64(bytes, 8),
                occurrence_start: get64(bytes, 16),
                ordinal: get32(bytes, 24) as u32,
                mode: mode(bytes[28])?,
            }),
            5 => Self::Component(get_range(bytes, 0)),
            6 => Self::Member(get_id(bytes, 0)?),
            _ => return Err(Error::InvalidProgram),
        };
        let mut canonical = [0; RECORD_BYTES];
        record.encode(&mut canonical);
        if canonical != *bytes {
            return Err(Error::InvalidProgram);
        }
        Ok(record)
    }
    pub fn append(self, topology: &mut PreparedTopology) {
        match self {
            Self::Node(row) => topology.nodes.push(row),
            Self::Trigger(row) => topology.triggers.push(row),
            Self::Program(row) => topology.programs.push(row),
            Self::Target(row) => topology.targets.push(row),
            Self::Segment(row) => topology.segments.push(row),
            Self::Component(row) => topology.components.push(row),
            Self::Member(row) => topology.members.push(row),
        }
    }
}
impl PreparedTopology {
    pub fn encoded_records(&self) -> usize {
        Counts::of(self).total()
    }
    pub fn encode_header(&self, output: &mut [u8; RECORD_BYTES]) {
        encode_header(self.limits, Counts::of(self), self.relation_count, output);
    }
    pub fn encode_record(
        &self,
        index: usize,
        output: &mut [u8; RECORD_BYTES],
    ) -> Result<(), Error> {
        let (kind, index) = Counts::of(self).locate(index).ok_or(Error::InvalidId)?;
        let record = match kind {
            0 => Record::Node(self.nodes[index]),
            1 => Record::Trigger(self.triggers[index]),
            2 => Record::Program(self.programs[index]),
            3 => Record::Target(self.targets[index]),
            4 => Record::Segment(self.segments[index]),
            5 => Record::Component(self.components[index]),
            6 => Record::Member(self.members[index]),
            _ => unreachable!(),
        };
        record.encode(output);
        Ok(())
    }
}
