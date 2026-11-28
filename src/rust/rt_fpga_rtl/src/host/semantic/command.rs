use super::*;
use rt_fpga_reference::fixed::Fixed;
use shoop_rt_protocol::immediate::{
    ClearPlanned, Code, IterationTransition, SwitchProgram, TransitionPlan,
};

/// Width-checked immutable operands, not an allocation lease or live-state authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundTarget {
    slot: u16,
    generation: u32,
    gain: Option<u16>,
    self_sync: bool,
}
impl BoundTarget {
    pub fn new(
        slot: u32,
        generation: u32,
        gain: Option<u32>,
        self_sync: bool,
    ) -> Result<Self, CodecError> {
        if slot >= SEMANTIC_LOOP_LIMIT || generation == 0 {
            return Err(CodecError::Binding);
        }
        Ok(Self {
            slot: slot as u16,
            generation,
            gain: gain
                .map(|address| u16::try_from(address).map_err(|_| CodecError::Binding))
                .transpose()?,
            self_sync,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandRecord([u32; COMMAND_WORDS]);
impl CommandRecord {
    fn bound(kind: u32, target: Option<BoundTarget>, args: [u32; 5]) -> Self {
        let Some(target) = target else {
            return Self::refusal(6, Code::TargetUnavailable);
        };
        let mut words = [0; COMMAND_WORDS];
        words[0] = kind;
        words[1] = u32::from(target.slot);
        words[2] = target.generation;
        words[3] = u32::from(target.self_sync);
        words[6..11].copy_from_slice(&args);
        Self(words)
    }
    pub fn transition(target: Option<BoundTarget>, plan: TransitionPlan) -> Self {
        Self::bound(
            0,
            target,
            [
                plan.now.map_or(u32::MAX, |mode| mode as u32),
                plan.first.map_or(u32::MAX, |step| step.mode as u32),
                plan.first.map_or(0, |step| step.skip),
                plan.second.map_or(u32::MAX, |step| step.mode as u32),
                plan.second.map_or(0, |step| step.skip),
            ],
        )
    }
    pub fn clear(target: Option<BoundTarget>, mask: ClearPlanned) -> Self {
        Self::bound(1, target, [mask as u32, 0, 0, 0, 0])
    }
    pub fn repeat(target: Option<BoundTarget>, enabled: bool) -> Self {
        Self::bound(2, target, [u32::from(enabled), 0, 0, 0, 0])
    }
    pub fn gain(target: Option<BoundTarget>, value: f32) -> Self {
        if !value.is_finite() {
            return Self::refusal(6, Code::InvalidValue);
        }
        let Some(target) = target else {
            return Self::refusal(6, Code::TargetUnavailable);
        };
        let Some(address) = target.gain else {
            return Self::refusal(6, Code::Unsupported);
        };
        let mut record = Self::bound(
            3,
            Some(target),
            [Fixed::from_f32(value).0 as u32, 0, 0, 0, 0],
        );
        record.0[1] |= u32::from(address) << 16;
        record.0[5] = value.to_bits();
        record
    }
    pub fn iteration(target: Option<BoundTarget>, request: IterationTransition) -> Self {
        let [low, high] = split(request.iteration);
        Self::bound(
            4,
            target,
            [
                low,
                high,
                u32::from(request.retrigger) | (u32::from(request.seek) << 1),
                0,
                0,
            ],
        )
    }
    pub fn switch(request: SwitchProgram, candidate_has_composites: bool) -> Self {
        let mut words = [0; COMMAND_WORDS];
        words[0] = 5;
        words[6..8].copy_from_slice(&split(request.revision));
        words[8] = u32::from(request.region) | (u32::from(request.instructions) << 16);
        words[9] = u32::from(request.sync_count) | (u32::from(request.states) << 16);
        words[10] = u32::from(request.buffers) | (u32::from(candidate_has_composites) << 16);
        Self(words)
    }
    fn refusal(kind: u32, code: Code) -> Self {
        let mut words = [0; COMMAND_WORDS];
        words[0] = kind;
        words[4] = code as u32;
        Self(words)
    }
    fn checked_refusal(kind: u32, code: Code) -> Result<Self, CodecError> {
        match code as u32 {
            4..=9 | 11..=29 => Ok(Self::refusal(kind, code)),
            _ => Err(CodecError::Refusal),
        }
    }
    pub fn refused(code: Code) -> Result<Self, CodecError> {
        Self::checked_refusal(6, code)
    }
    pub fn refused_switch(code: Code) -> Result<Self, CodecError> {
        Self::checked_refusal(5, code)
    }
    pub fn words(&self) -> &[u32; COMMAND_WORDS] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupIdentity {
    epoch: u32,
    sequence: u64,
    count: usize,
}
impl GroupIdentity {
    pub fn new(epoch: u32, sequence: u64, count: usize) -> Result<Self, CodecError> {
        if epoch == 0 || sequence == 0 {
            return Err(CodecError::Identity);
        }
        if !(1..=MAX_COMMANDS).contains(&count) {
            return Err(CodecError::Length);
        }
        Ok(Self {
            epoch,
            sequence,
            count,
        })
    }
    pub fn epoch(self) -> u32 {
        self.epoch
    }
    pub fn sequence(self) -> u64 {
        self.sequence
    }
    pub fn count(self) -> usize {
        self.count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupHeader {
    pub epoch: u32,
    pub sequence: u64,
    pub graph: u64,
    pub topology: u64,
    pub deadline: Option<u64>,
}

/// One complete group. Encoding neither splits it nor resolves attempt-time checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupPacket {
    identity: GroupIdentity,
    words: [u32; MAX_GROUP_WORDS],
}
impl GroupPacket {
    pub fn new(header: GroupHeader, records: &[CommandRecord]) -> Result<Self, CodecError> {
        let identity = GroupIdentity::new(header.epoch, header.sequence, records.len())?;
        let mut words = [0; MAX_GROUP_WORDS];
        words[0] = SEMANTIC_ID;
        words[1] = SEMANTIC_VERSION | ((identity.count as u32) << 16);
        words[2] = header.epoch;
        words[3] = u32::from(header.deadline.is_some());
        words[4..6].copy_from_slice(&split(header.sequence));
        words[6..8].copy_from_slice(&split(header.graph));
        words[8..10].copy_from_slice(&split(header.topology));
        words[10..12].copy_from_slice(&split(header.deadline.unwrap_or(0)));
        for (record, output) in records
            .iter()
            .zip(words[HEADER_WORDS..].chunks_exact_mut(COMMAND_WORDS))
        {
            output.copy_from_slice(record.words());
        }
        Ok(Self { identity, words })
    }
    pub fn identity(&self) -> GroupIdentity {
        self.identity
    }
    pub fn words(&self) -> &[u32] {
        &self.words[..HEADER_WORDS + self.identity.count * COMMAND_WORDS]
    }
}
