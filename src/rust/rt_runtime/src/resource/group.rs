//! Fixed capture-group ownership and prepared take lifecycle.
use super::*;
use crate::midi_state::MidiState;
mod interval;
use interval::AppendReservation;
pub use interval::CaptureIntervalId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureGroupConfig {
    pub groups: usize,
    pub members_per_group: usize,
    pub staging_bytes_per_group: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureGroupId {
    scope: u32,
    slot: u32,
    generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureTakeId {
    group: CaptureGroupId,
    sequence: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureMember {
    pub resource: ResourceId,
    pub format: Format,
    pub midi_events: usize,
    pub midi_bytes: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturePhase {
    Idle,
    Prepared,
    /// First interval reserved without publishing new active media.
    PreparedReserved,
    Recording,
    Reserved,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureGroupInfo {
    pub phase: CapturePhase,
    pub take_sequence: u64,
    pub interval_sequence: u64,
    pub interval_frames: usize,
    pub members: usize,
    pub staging_bytes: usize,
    pub start_frame: Option<Frame>,
    pub next_frame: Option<Frame>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CaptureGroupUsage {
    pub groups: usize,
    pub free_groups: usize,
    pub staging_bytes: usize,
}
#[derive(Clone, Copy)]
struct Member {
    spec: CaptureMember,
    take: Option<UploadId>,
    lease: Option<SnapshotId>,
    staging_start: usize,
    staging_bytes: usize,
    append: Option<AppendReservation>,
}
#[derive(Default)]
pub(super) struct CapturePool {
    config: Option<CaptureGroupConfig>,
    groups: Vec<Slot<CaptureGroupInfo>>,
    free: Vec<usize>,
    members: Vec<Option<Member>>,
    staging: Vec<u8>,
}
impl CapturePool {
    fn new(config: CaptureGroupConfig, prepared: PreparedConfig) -> Result<Self, Error> {
        if config.groups == 0 || config.members_per_group == 0 {
            return Err(Error::InvalidConfig);
        }
        if config.groups > prepared.limits().resource_slots
            || config.members_per_group > prepared.limits().resource_slots
        {
            return Err(Error::Capacity);
        }
        let members = config
            .groups
            .checked_mul(config.members_per_group)
            .ok_or(Error::Capacity)?;
        let staging = config
            .groups
            .checked_mul(config.staging_bytes_per_group)
            .ok_or(Error::Capacity)?;
        let bytes = members
            .checked_mul(size_of::<Option<Member>>())
            .and_then(|n| {
                config
                    .groups
                    .checked_mul(size_of::<Slot<CaptureGroupInfo>>() + size_of::<usize>())
                    .and_then(|m| n.checked_add(m))
            })
            .and_then(|n| n.checked_add(staging))
            .ok_or(Error::Capacity)?;
        if bytes > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        let mut pool = Self {
            config: Some(config),
            groups: slots(config.groups),
            free: (0..config.groups).rev().collect(),
            members: vec![None; members],
            staging: vec![0; staging],
        };
        for slot in &mut pool.groups {
            *std::hint::black_box(slot) = Slot {
                generation: 1,
                value: None,
            };
        }
        for member in &mut pool.members {
            *std::hint::black_box(member) = None;
        }
        for byte in &mut pool.staging {
            *std::hint::black_box(byte) = 0;
        }
        Ok(pool)
    }
    fn member_index(&self, id: CaptureGroupId, member: usize) -> usize {
        id.slot as usize
            * self
                .config
                .expect("prepared capture groups")
                .members_per_group
            + member
    }
    fn member(&self, id: CaptureGroupId, member: usize) -> Member {
        self.members[self.member_index(id, member)].expect("bound group member")
    }
    fn set_member(&mut self, id: CaptureGroupId, index: usize, member: Member) {
        let slot = self.member_index(id, index);
        self.members[slot] = Some(member);
    }
}

impl ResourceStore {
    /// Prepare group storage before handing the store to a render runtime.
    pub fn with_capture_groups(
        config: PreparedConfig,
        indexing: Option<MidiIndexConfig>,
        groups: CaptureGroupConfig,
    ) -> Result<Self, Error> {
        let pool = CapturePool::new(groups, config)?;
        let mut store = match indexing {
            Some(indexing) => Self::with_midi_indexing(config, indexing)?,
            None => Self::new(config)?,
        };
        store.capture_groups = pool;
        Ok(store)
    }
    pub fn capture_group_usage(&self) -> CaptureGroupUsage {
        CaptureGroupUsage {
            groups: self.capture_groups.groups.len(),
            free_groups: self.capture_groups.free.len(),
            staging_bytes: self.capture_groups.staging.len(),
        }
    }
    pub fn capture_group_info(&self, id: CaptureGroupId) -> Result<CaptureGroupInfo, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        get(&self.capture_groups.groups, id.slot, id.generation)
    }
    /// Claim all members or none. Reads and existing immutable leases remain valid.
    pub fn bind_capture_group(
        &mut self,
        members: &[CaptureMember],
    ) -> Result<CaptureGroupId, Error> {
        let config = self.capture_groups.config.ok_or(Error::InvalidState)?;
        if members.is_empty() || members.len() > config.members_per_group {
            return Err(Error::InvalidShape);
        }
        if self.capture_groups.free.is_empty() {
            return Err(Error::Capacity);
        }
        let limits = self.config.limits();
        let mut staging = 0usize;
        for (index, member) in members.iter().enumerate() {
            let resource = self.check_writer(member.resource, None)?;
            if members[..index]
                .iter()
                .any(|m| m.resource == member.resource)
            {
                return Err(Error::InvalidShape);
            }
            if let Some(active) = resource.active {
                if get(&self.versions, active.slot, active.generation)?.state == State::Capturing {
                    return Err(Error::InvalidState);
                }
            }
            Description {
                format: member.format,
                frames: 0,
                bytes: 0,
            }
            .validate(self.config.resource_bytes())?;
            let bytes = match member.format {
                Format::Audio { channels } => {
                    if member.midi_events != 0 || member.midi_bytes != 0 {
                        return Err(Error::InvalidShape);
                    }
                    limits
                        .max_frames
                        .checked_mul(channels as usize)
                        .and_then(|n| n.checked_mul(4))
                        .ok_or(Error::Capacity)?
                }
                Format::Midi => {
                    if member.midi_events
                        > limits
                            .midi_events_per_port
                            .max(crate::midi::ResetState::EVENTS)
                        || member.midi_bytes
                            > limits
                                .midi_bytes_per_port
                                .max(crate::midi::ResetState::BYTES)
                    {
                        return Err(Error::InvalidShape);
                    }
                    member
                        .midi_events
                        .checked_mul(12)
                        .and_then(|n| n.checked_add(member.midi_bytes))
                        .ok_or(Error::Capacity)?
                }
            };
            staging = staging.checked_add(bytes).ok_or(Error::Capacity)?;
        }
        if staging > config.staging_bytes_per_group {
            return Err(Error::Capacity);
        }
        let slot = self
            .capture_groups
            .free
            .pop()
            .expect("preflighted group slot");
        let id = CaptureGroupId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.capture_groups.groups[slot].generation,
        };
        let mut staging_start = slot * config.staging_bytes_per_group;
        for (index, &spec) in members.iter().enumerate() {
            let staging_bytes = match spec.format {
                Format::Audio { channels } => limits.max_frames * channels as usize * 4,
                Format::Midi => spec.midi_events * 12 + spec.midi_bytes,
            };
            self.capture_groups.set_member(
                id,
                index,
                Member {
                    spec,
                    take: None,
                    lease: None,
                    staging_start,
                    staging_bytes,
                    append: None,
                },
            );
            staging_start += staging_bytes;
            self.resources[spec.resource.slot() as usize]
                .value
                .as_mut()
                .expect("validated member")
                .claim = Some(id);
        }
        self.capture_groups.groups[slot].value = Some(CaptureGroupInfo {
            phase: CapturePhase::Idle,
            take_sequence: 0,
            interval_sequence: 0,
            interval_frames: 0,
            members: members.len(),
            staging_bytes: staging,
            start_frame: None,
            next_frame: None,
        });
        Ok(id)
    }
    pub fn release_capture_group(&mut self, id: CaptureGroupId) -> Result<(), Error> {
        let group = self.capture_group_info(id)?;
        if group.phase != CapturePhase::Idle {
            return Err(Error::InvalidState);
        }
        for index in 0..group.members {
            self.check_writer(
                self.capture_groups.member(id, index).spec.resource,
                Some(id),
            )?;
        }
        for index in 0..group.members {
            let member = self.capture_groups.member(id, index);
            self.resources[member.spec.resource.slot() as usize]
                .value
                .as_mut()
                .expect("claimed member")
                .claim = None;
            let slot = self.capture_groups.member_index(id, index);
            self.capture_groups.members[slot] = None;
        }
        release(
            &mut self.capture_groups.groups,
            &mut self.capture_groups.free,
            id.slot as usize,
        );
        Ok(())
    }
    /// Initial states correspond to MIDI members in declaration order. Reserve all
    /// version/index/final-lease credits without changing any active revision.
    pub fn prepare_capture_take(
        &mut self,
        id: CaptureGroupId,
        initial: &[MidiState],
    ) -> Result<CaptureTakeId, Error> {
        let mut group = self.capture_group_info(id)?;
        if group.phase != CapturePhase::Idle {
            return Err(Error::InvalidState);
        }
        let sequence = group.take_sequence.checked_add(1).ok_or(Error::Exhausted)?;
        let mut midi = 0;
        for index in 0..group.members {
            let member = self.capture_groups.member(id, index);
            let resource = self.check_writer(member.spec.resource, Some(id))?;
            resource.revision.checked_next()?;
            if member.spec.format == Format::Midi {
                midi += 1;
            }
        }
        if initial.len() != midi {
            return Err(Error::InvalidShape);
        }
        if self.free_versions.len() < group.members || self.free_snapshots.len() < group.members {
            return Err(Error::Capacity);
        }
        if midi != 0 {
            self.midi_indexes.prepare()?;
            if self.midi_indexes.usage().free_versions < midi {
                return Err(Error::Capacity);
            }
        }
        let mut midi = 0;
        for index in 0..group.members {
            let mut member = self.capture_groups.member(id, index);
            let expected = self
                .revision(member.spec.resource)
                .expect("claimed resource");
            let take = self
                .begin_upload_with(
                    member.spec.resource,
                    expected,
                    Description {
                        format: member.spec.format,
                        frames: 0,
                        bytes: 0,
                    },
                    Some(id),
                )
                .expect("preflighted take version");
            let owner = if member.spec.format == Format::Midi {
                let owner = self.midi_indexes.begin(take, &initial[midi]);
                midi += 1;
                Some(owner)
            } else {
                None
            };
            let version = self.versions[take.slot as usize]
                .value
                .as_mut()
                .expect("prepared version");
            version.state = State::PreparedCapture;
            version.midi = owner;
            let slot = self.free_snapshots.pop().expect("preflighted final lease");
            member.take = Some(take);
            member.lease = Some(SnapshotId {
                slot: slot as u32,
                generation: self.snapshots[slot].generation,
            });
            self.capture_groups.set_member(id, index, member);
        }
        group.phase = CapturePhase::Prepared;
        group.start_frame = None;
        group.next_frame = None;
        group.interval_frames = 0;
        group.take_sequence = sequence;
        self.capture_groups.groups[id.slot as usize].value = Some(group);
        Ok(CaptureTakeId {
            group: id,
            sequence,
        })
    }
    fn capture_take_info(&self, take: CaptureTakeId) -> Result<CaptureGroupInfo, Error> {
        let group = self.capture_group_info(take.group)?;
        if group.take_sequence != take.sequence {
            return Err(Error::InvalidId);
        }
        Ok(group)
    }
    pub fn cancel_capture_take(&mut self, take: CaptureTakeId) -> Result<(), Error> {
        let mut group = self.capture_take_info(take)?;
        let id = take.group;
        if group.phase != CapturePhase::Prepared {
            return Err(Error::InvalidState);
        }
        for index in 0..group.members {
            let mut member = self.capture_groups.member(id, index);
            let take = member.take.take().expect("prepared take");
            let lease = member.lease.take().expect("reserved final lease");
            self.unreference(take).expect("prepared version reference");
            release(
                &mut self.snapshots,
                &mut self.free_snapshots,
                lease.slot as usize,
            );
            self.capture_groups.set_member(id, index, member);
        }
        group.phase = CapturePhase::Idle;
        self.capture_groups.groups[id.slot as usize].value = Some(group);
        Ok(())
    }
    pub fn activate_capture_take(&mut self, take: CaptureTakeId, at: Frame) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let mut group = self.capture_take_info(take)?;
        let id = take.group;
        if !matches!(
            group.phase,
            CapturePhase::Prepared | CapturePhase::PreparedReserved
        ) {
            return Err(Error::InvalidState);
        }
        let reserved = group.phase == CapturePhase::PreparedReserved;
        if reserved && group.next_frame != Some(at) {
            return Err(Error::InvalidFrame);
        }
        for index in 0..group.members {
            let member = self.capture_groups.member(id, index);
            let resource = self.check_writer(member.spec.resource, Some(id))?;
            let take = member.take.ok_or(Error::InvalidState)?;
            let version = get(&self.versions, take.slot, take.generation)?;
            if version.state != State::PreparedCapture {
                return Err(Error::InvalidState);
            }
            if resource.revision != version.expected_revision {
                return Err(Error::StaleRevision);
            }
            resource.revision.checked_next()?;
        }
        for index in 0..group.members {
            let member = self.capture_groups.member(id, index);
            let take = member.take.expect("prepared take");
            let mut version =
                get(&self.versions, take.slot, take.generation).expect("validated version");
            let resource = self.resources[member.spec.resource.slot() as usize]
                .value
                .as_mut()
                .expect("claimed resource");
            let previous = resource.active.replace(take);
            resource.revision = resource
                .revision
                .checked_next()
                .expect("validated revision");
            version.revision = resource.revision;
            version.state = State::Capturing;
            version.capture = Some(CaptureInfo {
                start_frame: at,
                next_frame: at,
                active: true,
                incomplete: None,
            });
            self.versions[take.slot as usize].value = Some(version);
            if let Some(previous) = previous {
                self.detach(previous).expect("previous active reference");
            }
        }
        group.phase = if reserved {
            CapturePhase::Reserved
        } else {
            CapturePhase::Recording
        };
        group.start_frame = Some(at);
        group.next_frame = Some(at);
        self.capture_groups.groups[id.slot as usize].value = Some(group);
        Ok(())
    }
    /// Freeze at the common boundary and publish the previously reserved leases.
    /// Output slots must be empty; overwriting one could lose a caller's owner.
    pub fn freeze_capture_take(
        &mut self,
        take: CaptureTakeId,
        at: Frame,
        incomplete: Option<Error>,
        output: &mut [Option<Snapshot>],
    ) -> Result<(), Error> {
        self.note_operation(WorkClass::Render);
        let mut group = self.capture_take_info(take)?;
        let id = take.group;
        if group.phase != CapturePhase::Recording {
            return Err(Error::InvalidState);
        }
        if group.next_frame != Some(at) {
            return Err(Error::InvalidFrame);
        }
        if output.len() != group.members {
            return Err(Error::InvalidShape);
        }
        if output.iter().any(Option::is_some) {
            return Err(Error::InvalidState);
        }
        for index in 0..group.members {
            let member = self.capture_groups.member(id, index);
            let resource = self.check_writer(member.spec.resource, Some(id))?;
            let take = member.take.ok_or(Error::InvalidState)?;
            let version = get(&self.versions, take.slot, take.generation)?;
            if resource.active != Some(take)
                || version.state != State::Capturing
                || version.capture.ok_or(Error::InvalidState)?.next_frame != at
            {
                return Err(Error::InvalidState);
            }
            version.references.checked_add(1).ok_or(Error::Exhausted)?;
            let lease = member.lease.ok_or(Error::InvalidState)?;
            let slot = self
                .snapshots
                .get(lease.slot as usize)
                .ok_or(Error::InvalidId)?;
            if slot.generation != lease.generation || slot.value.is_some() {
                return Err(Error::InvalidId);
            }
        }
        for (index, output) in output.iter_mut().enumerate() {
            let mut member = self.capture_groups.member(id, index);
            let take = member.take.take().expect("validated take");
            let lease = member.lease.take().expect("validated lease reservation");
            let mut version =
                get(&self.versions, take.slot, take.generation).expect("validated version");
            version.state = State::Active;
            let capture = version.capture.as_mut().expect("capture metadata");
            capture.active = false;
            capture.incomplete = incomplete;
            version.references += 1;
            let snapshot = Snapshot {
                id: lease,
                resource: member.spec.resource,
                revision: version.revision,
                description: version.description,
                cut_frame: at,
                capture: version.capture,
            };
            self.snapshots[lease.slot as usize].value = Some(Lease {
                snapshot,
                version: take,
                private_tail: None,
                tail_start: version.written,
            });
            self.versions[take.slot as usize].value = Some(version);
            self.capture_groups.set_member(id, index, member);
            *output = Some(snapshot);
        }
        group.phase = CapturePhase::Idle;
        self.capture_groups.groups[id.slot as usize].value = Some(group);
        Ok(())
    }
}
