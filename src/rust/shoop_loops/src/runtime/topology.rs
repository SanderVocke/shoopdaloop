use super::*;
use crate::topology::staging::{Progress, StageId, Stager, Status, RECORD_BYTES};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TopologySettings {
    pub candidates: usize,
    pub records_per_callback: usize,
    pub validation_steps_per_callback: usize,
}
struct Slot {
    stager: Stager,
    id: Option<StageId>,
    ready: Option<PreparedTopology>,
    retired: Option<PreparedTopology>,
    program: Option<RequestId>,
    discard: bool,
    progress: Progress,
}
pub(super) struct Topologies {
    settings: TopologySettings,
    slots: Box<[Slot]>,
    callback: Option<Frame>,
    records: usize,
    steps: usize,
    operation_bytes: usize,
}
impl Topologies {
    pub(super) fn prepare(
        topology: PreparedTopology,
        settings: TopologySettings,
    ) -> Result<(Self, PreparedTopology), Failure> {
        let enabled = settings.candidates != 0;
        if enabled != (settings.records_per_callback != 0)
            || enabled != (settings.validation_steps_per_callback != 0)
        {
            return Err(Error::InvalidConfig.into());
        }
        let operation_bytes = settings
            .records_per_callback
            .checked_mul(RECORD_BYTES)
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let limits = topology.limits();
        let bytes = if enabled {
            Stager::required_storage_bytes(limits)?
                .checked_add(size_of::<Slot>())
                .and_then(|n| n.checked_mul(settings.candidates))
                .and_then(|n| n.checked_add(limits.topology_storage_bytes().ok()?))
                .ok_or(Error::Capacity)?
        } else {
            0
        };
        if bytes > i32::MAX as usize {
            return Err(Error::Capacity.into());
        }
        let mut slots = Vec::with_capacity(settings.candidates);
        for _ in 0..settings.candidates {
            slots.push(Slot {
                stager: Stager::prepare(limits)?,
                id: None,
                ready: None,
                retired: None,
                program: None,
                discard: false,
                progress: Progress {
                    used: 0,
                    total_steps: 0,
                    ready: false,
                },
            });
        }
        let topology = if enabled {
            let mut initial = Stager::prepare(limits)?;
            let mut bytes = [0; RECORD_BYTES];
            topology.encode_header(&mut bytes);
            let id = initial.begin(&bytes)?;
            for record in 0..topology.encoded_records() {
                topology.encode_record(record, &mut bytes)?;
                initial.append(id, record, &bytes, 1)?;
            }
            while !initial.validate(id, usize::MAX)?.ready {}
            initial.take(id)?
        } else {
            topology
        };
        Ok((
            Self {
                settings,
                slots: slots.into_boxed_slice(),
                callback: None,
                records: 0,
                steps: 0,
                operation_bytes,
            },
            topology,
        ))
    }
    pub(super) fn operation_bytes(&self) -> usize {
        self.operation_bytes
    }
    pub(super) fn callback(&mut self, frame: Frame) {
        if self.callback != Some(frame) {
            self.callback = Some(frame);
            self.records = self.settings.records_per_callback;
            self.steps = self.settings.validation_steps_per_callback;
        }
    }
    fn slot(&self, id: StageId) -> Result<usize, Failure> {
        self.slots
            .iter()
            .position(|s| s.id == Some(id))
            .ok_or(crate::Error::InvalidId.into())
    }
    pub(super) fn begin(&mut self, slot: usize, bytes: &[u8]) -> Result<StageId, Failure> {
        let header = bytes.try_into().map_err(|_| Error::InvalidShape)?;
        let slot = self.slots.get_mut(slot).ok_or(Error::InvalidId)?;
        let id = slot.stager.begin(header)?;
        slot.id = Some(id);
        slot.progress = Progress {
            used: 0,
            total_steps: 0,
            ready: false,
        };
        Ok(id)
    }
    pub(super) fn status(&self, id: StageId) -> Result<Status, Failure> {
        let slot = &self.slots[self.slot(id)?];
        Ok(if slot.ready.is_some() {
            Status::Ready
        } else {
            slot.stager.status()
        })
    }
    pub(super) fn append(
        &mut self,
        id: StageId,
        first: usize,
        bytes: &[u8],
    ) -> Result<Status, Failure> {
        let slot = self.slot(id)?;
        let allowance = self.records;
        match self.slots[slot].stager.append(id, first, bytes, allowance) {
            Ok(()) => self.records -= bytes.len() / RECORD_BYTES,
            Err(error) => {
                self.records = 0;
                return Err(error.into());
            }
        }
        self.status(id)
    }
    pub(super) fn validate(&mut self, id: StageId) -> Result<Progress, Failure> {
        let index = self.slot(id)?;
        let slot = &mut self.slots[index];
        if slot.ready.is_some() {
            return Ok(Progress {
                used: 0,
                ..slot.progress
            });
        }
        let progress = match slot.stager.validate(id, self.steps) {
            Ok(progress) => progress,
            Err(error) => {
                self.steps = 0;
                return Err(error.into());
            }
        };
        self.steps -= progress.used;
        slot.progress = progress;
        if progress.ready {
            slot.ready = Some(slot.stager.take(id)?);
        }
        Ok(progress)
    }
    pub(super) fn cancel(&mut self, id: StageId, resolver: &mut Resolver) -> Result<(), Failure> {
        let index = self.slot(id)?;
        let slot = &mut self.slots[index];
        if let Some(request) = slot.program {
            if resolver
                .completion(request)?
                .is_some_and(|result| matches!(result.outcome, crate::Outcome::Applied { .. }))
            {
                return Err(Error::InvalidState.into());
            }
            resolver.cancel(request)?;
            slot.discard = true;
            return Ok(());
        }
        if let Some(owner) = slot.ready.take() {
            if let Err((error, owner)) = slot.stager.recycle(owner) {
                slot.ready = Some(owner);
                return Err(error.into());
            }
        } else {
            slot.stager.cancel(id)?;
        }
        Ok(())
    }
    pub(super) fn recycle(&mut self, id: StageId) -> Result<(), Failure> {
        let index = self.slot(id)?;
        let slot = &mut self.slots[index];
        let owner = slot.retired.take().ok_or(Error::InvalidState)?;
        if let Err((error, owner)) = slot.stager.recycle(owner) {
            slot.retired = Some(owner);
            return Err(error.into());
        }
        Ok(())
    }
    pub(super) fn activate(
        &mut self,
        id: StageId,
        expected: Epoch,
        resolver: &mut Resolver,
    ) -> Result<Epoch, Failure> {
        let index = self.slot(id)?;
        let slot = &mut self.slots[index];
        let candidate = slot.ready.as_ref().ok_or(Error::InvalidState)?;
        if slot.retired.is_some() {
            return Err(Error::InvalidState.into());
        }
        let current = resolver.topology().nodes();
        if candidate.nodes().len() != current.len() {
            return Err(Error::InvalidShape.into());
        }
        for (new, old) in candidate.nodes().iter().zip(current) {
            if new.id != old.id {
                return Err(Error::InvalidId.into());
            }
            if new.kind != old.kind {
                return Err(Error::InvalidShape.into());
            }
        }
        self.activate_configuration(id, expected, resolver)
    }
    pub(super) fn candidate(&self, id: StageId) -> Result<&PreparedTopology, Failure> {
        let slot = &self.slots[self.slot(id)?];
        if slot.retired.is_some() {
            return Err(Error::InvalidState.into());
        }
        slot.ready.as_ref().ok_or(Error::InvalidState.into())
    }
    pub(super) fn activate_configuration(
        &mut self,
        id: StageId,
        expected: Epoch,
        resolver: &mut Resolver,
    ) -> Result<Epoch, Failure> {
        self.candidate(id)?;
        let index = self.slot(id)?;
        let slot = &mut self.slots[index];
        let candidate = slot.ready.take().expect("validated candidate");
        match resolver.replace_topology(expected, candidate) {
            Ok(retired) => slot.retired = Some(retired),
            Err((error, candidate)) => {
                slot.ready = Some(candidate);
                return Err(error.into());
            }
        }
        Ok(resolver.epoch())
    }
    pub(super) fn stage_program(
        &mut self,
        id: StageId,
        target: LoopId,
        expected: Epoch,
        sequence: Sequence,
        at: Frame,
        resolver: &mut Resolver,
    ) -> Result<RequestId, Failure> {
        let index = self.slot(id)?;
        let slot = &mut self.slots[index];
        if slot.program.is_some() || slot.retired.is_some() {
            return Err(Error::InvalidState.into());
        }
        let candidate = slot.ready.take().ok_or(Error::InvalidState)?;
        match resolver.stage_program(expected, target, sequence, at, candidate) {
            Ok(request) => {
                slot.program = Some(request);
                slot.discard = false;
                Ok(request)
            }
            Err((error, candidate)) => {
                slot.ready = Some(candidate);
                Err(error.into())
            }
        }
    }
    pub(super) fn collect_program(
        &mut self,
        id: StageId,
        request: RequestId,
        resolver: &mut Resolver,
    ) -> Result<(), Failure> {
        let index = self.slot(id)?;
        let slot = &mut self.slots[index];
        if slot.program != Some(request) {
            return Err(Error::InvalidId.into());
        }
        let result = resolver
            .take_program_change(request)?
            .ok_or(Error::InvalidState)?;
        slot.program = None;
        if result.activated_epoch.is_some() {
            slot.retired = Some(result.topology);
        } else {
            slot.ready = Some(result.topology);
        }
        if slot.discard {
            slot.discard = false;
            self.cancel(id, resolver)?;
        }
        Ok(())
    }
    pub(super) fn cleanup(&mut self, resolver: &mut Resolver) -> Result<(), Failure> {
        for index in 0..self.slots.len() {
            let Some(id) = self.slots[index].id else {
                continue;
            };
            if let Some(request) = self.slots[index].program {
                self.collect_program(id, request, resolver)?;
            }
            if self.slots[index].retired.is_some() {
                self.recycle(id)?;
            } else if self.status(id)? != Status::Idle {
                self.cancel(id, resolver)?;
            }
        }
        Ok(())
    }
}
