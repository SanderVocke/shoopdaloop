//! Bounded staging of media replacements that publish at one musical cut.
use crate::{
    endpoint::{loop_id, ControlEndpoint},
    jobs::Job,
};
use alloc::{boxed::Box, vec};
use shoop_loops::runtime::ContentFamilyEntry;
use shoop_rt_protocol::{ContentFamilyEdit, LoopSlotId, Outcome, Refusal, Request};

#[derive(Clone, Copy)]
pub(crate) struct Entry {
    pub edit: ContentFamilyEdit,
    pub media: Option<shoop_loops::media::MediaId>,
    pub revision: Option<rt_runtime::Revision>,
}

pub(crate) struct Stage {
    pub id: Option<u64>,
    pub root: Option<LoopSlotId>,
    pub epoch: rt_runtime::Epoch,
    pub entries: Box<[Option<Entry>]>,
    pub expected: usize,
    pub committing: bool,
    pub cancel_requested: bool,
    pub payload: Option<Box<[u8]>>,
    last_id: u64,
}
impl Stage {
    pub fn new(loops: usize) -> Self {
        Self {
            id: None,
            root: None,
            epoch: rt_runtime::Epoch(0),
            entries: vec![None; loops].into_boxed_slice(),
            expected: 0,
            committing: false,
            cancel_requested: false,
            last_id: 0,
            payload: Some(vec![0; loops * crate::topology::RECORD_BYTES].into_boxed_slice()),
        }
    }
    pub fn clear(&mut self) {
        self.id = None;
        self.root = None;
        self.entries.fill(None);
        self.expected = 0;
        self.committing = false;
        self.cancel_requested = false;
    }
}

impl ControlEndpoint {
    pub(crate) fn family_request(
        &mut self,
        correlation: u64,
        request: Request,
    ) -> Result<Option<Outcome>, Refusal> {
        if self.job.is_some() {
            return Err(Refusal::Busy);
        }
        if let Request::ContentFamilyBegin {
            id,
            root,
            count,
            sample_rate,
            quantum,
        } = request
        {
            if self.family.id.is_some() || self.replacement.id.is_some() {
                return Err(Refusal::Busy);
            }
            if id == 0 || id <= self.family.last_id || self.model.find_loop(root).is_none() {
                return Err(Refusal::InvalidId);
            }
            if count == 0 || count as usize > self.family.entries.len() {
                return Err(Refusal::Capacity);
            }
            if quantum != self.limits.quantum || sample_rate != self.cells.driver().sample_rate {
                return Err(Refusal::InvalidValue);
            }
            self.family.last_id = id;
            self.family.id = Some(id);
            self.family.root = Some(root);
            self.family.expected = count as usize;
            self.family.epoch = self.topology_epoch;
            return Ok(Some(Outcome::Done));
        }
        let id = match request {
            Request::ContentFamilyEdit { id, .. }
            | Request::ContentFamilyCommit { id }
            | Request::ContentFamilyCancel { id } => id,
            _ => return Err(Refusal::Internal),
        };
        if self.family.id != Some(id) {
            return Err(Refusal::InvalidId);
        }
        if self.family.committing {
            return Err(Refusal::Busy);
        }
        match request {
            Request::ContentFamilyEdit { edit, .. } => {
                let slot = edit.loop_id.0.index as usize;
                let owner = self
                    .model
                    .find_loop(edit.loop_id)
                    .ok_or(Refusal::InvalidId)?;
                if owner.sync.unwrap_or(owner.id) != edit.sync_source
                    || self.family.entries[slot].is_some()
                {
                    return Err(Refusal::InvalidId);
                }
                if self.family.entries.iter().flatten().count() >= self.family.expected {
                    return Err(Refusal::Capacity);
                }
                if edit.frames > u32::MAX as u64
                    || edit.frames % u64::from(self.limits.quantum) != 0
                    || (edit.replace && !edit.fence_content)
                    || (!edit.replace
                        && (edit.audio.is_some()
                            || edit.dry_audio.is_some()
                            || edit.recorded_state.is_some()
                            || edit.midi.is_some()
                            || edit.restart))
                {
                    return Err(Refusal::InvalidValue);
                }
                self.validate_recorded(edit.loop_id, edit.recorded_state)?;
                let audio = edit
                    .audio
                    .map(|id| self.uploads.source(id, false, edit.frames))
                    .transpose()?;
                let midi = edit
                    .midi
                    .map(|id| self.uploads.source(id, true, edit.frames))
                    .transpose()?;
                let auxiliary = edit
                    .dry_audio
                    .map(|id| self.uploads.source(id, false, edit.frames))
                    .transpose()?;
                self.family.entries[slot] = Some(Entry {
                    edit,
                    media: None,
                    revision: None,
                });
                if !edit.replace {
                    return Ok(Some(Outcome::Done));
                }
                self.touched_loops[slot] = crate::queue::HARD;
                let mut job = Job::family_append(
                    slot,
                    edit.loop_id,
                    edit.frames,
                    audio,
                    auxiliary,
                    midi,
                    edit.recorded_state,
                );
                job.correlation = Some(correlation);
                self.job = Some(job);
                Ok(None)
            }
            Request::ContentFamilyCommit { .. } => {
                if self.family.entries.iter().flatten().count() != self.family.expected
                    || self.family.cancel_requested
                    || self
                        .family
                        .entries
                        .iter()
                        .flatten()
                        .any(|entry| entry.edit.replace && entry.media.is_none())
                {
                    return Err(Refusal::Busy);
                }
                for (slot, entry) in self.family.entries.iter().enumerate() {
                    if entry.is_some_and(|entry| entry.edit.replace) {
                        self.touched_loops[slot] = crate::queue::HARD;
                    }
                }
                self.family.committing = true;
                let mut job = Job::family(true);
                job.correlation = Some(correlation);
                self.job = Some(job);
                Ok(None)
            }
            Request::ContentFamilyCancel { .. } => {
                self.family.cancel_requested = true;
                let mut job = Job::family(false);
                job.correlation = Some(correlation);
                self.job = Some(job);
                Ok(None)
            }
            _ => Err(Refusal::Internal),
        }
    }

    pub(crate) fn family_payload(&mut self) -> Result<Box<[u8]>, Refusal> {
        let mut payload = self.family.payload.take().ok_or(Refusal::Busy)?;
        payload.fill(0);
        for (entry, output) in self
            .family
            .entries
            .iter()
            .flatten()
            .zip(payload.chunks_exact_mut(crate::topology::RECORD_BYTES))
        {
            ContentFamilyEntry {
                target: loop_id(entry.edit.loop_id),
                sync_source: loop_id(entry.edit.sync_source),
                expected_revision: rt_runtime::Revision(entry.edit.expected_revision),
                expected_length: entry.edit.expected_frames,
                media: entry.media,
                restart: entry.edit.restart,
                fence_content: entry.edit.fence_content,
            }
            .encode(output)
            .expect("sized family payload");
        }
        Ok(payload)
    }
}
