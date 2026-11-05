//! Paired input ownership and all-target settlement at one completed cut.

use super::*;
use crate::grab::WindowSpec;

pub const INPUT_BATCH_RECORD_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputAction {
    Mode {
        mode: Mode,
        position: Option<u64>,
    },
    Grab {
        source: u32,
        history: Option<HistoryId>,
        sync: Option<LoopId>,
        window: WindowSpec,
        mode: Mode,
        maximum_frames: u64,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEdit {
    pub target: LoopId,
    pub action: InputAction,
}
impl InputEdit {
    pub fn encode(self, bytes: &mut [u8]) -> Result<(), Error> {
        if bytes.len() != INPUT_BATCH_RECORD_BYTES {
            return Err(Error::InvalidShape);
        }
        bytes.fill(0);
        let mode = match self.action {
            InputAction::Mode { mode, .. } | InputAction::Grab { mode, .. } => mode,
        };
        bytes[9] = match mode {
            Mode::Stopped => 0,
            Mode::Playing => 1,
            Mode::Recording => 2,
            Mode::Replacing => 3,
        };
        bytes[..4].copy_from_slice(&self.target.slot().to_le_bytes());
        bytes[4..8].copy_from_slice(&self.target.generation().to_le_bytes());
        match self.action {
            InputAction::Mode { position, .. } => {
                if position.is_some() && matches!(mode, Mode::Stopped | Mode::Recording) {
                    return Err(Error::InvalidEvent);
                }
                bytes[10] = u8::from(position.is_some());
                bytes[24..32].copy_from_slice(&position.unwrap_or(0).to_le_bytes());
            }
            InputAction::Grab {
                source,
                history,
                sync,
                window,
                mode,
                maximum_frames,
            } => {
                if window.continue_recording != (mode == Mode::Recording)
                    || mode != Mode::Recording && maximum_frames != 0
                {
                    return Err(Error::InvalidShape);
                }
                bytes[8] = 1;
                bytes[24..32].copy_from_slice(&maximum_frames.to_le_bytes());
                bytes[12..16].copy_from_slice(&source.to_le_bytes());
                if let Some(history) = history {
                    if history.source != source || history.scope == 0 || history.generation == 0 {
                        return Err(Error::InvalidId);
                    }
                    bytes[10] |= 16;
                    bytes[44..48].copy_from_slice(&history.scope.to_le_bytes());
                    bytes[48..56].copy_from_slice(&history.graph.0.to_le_bytes());
                    bytes[56..64].copy_from_slice(&history.generation.to_le_bytes());
                }
                if let Some(sync) = sync {
                    bytes[10] |= 8;
                    bytes[16..20].copy_from_slice(&sync.slot().to_le_bytes());
                    bytes[20..24].copy_from_slice(&sync.generation().to_le_bytes());
                }
                for (index, value) in [
                    window.reverse_start_cycle,
                    window.cycles_length,
                    window.go_to_cycle,
                ]
                .into_iter()
                .enumerate()
                {
                    if let Some(value) = value {
                        bytes[10] |= 1 << index;
                        bytes[32 + index * 4..36 + index * 4].copy_from_slice(&value.to_le_bytes());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != INPUT_BATCH_RECORD_BYTES {
            return Err(Error::InvalidShape);
        }
        if bytes[10] & !31 != 0 {
            return Err(Error::InvalidEvent);
        }
        let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let mode = match bytes[9] {
            0 => Mode::Stopped,
            1 => Mode::Playing,
            2 => Mode::Recording,
            3 => Mode::Replacing,
            _ => return Err(Error::InvalidEvent),
        };
        let edit = Self {
            target: LoopId::from_parts(u32_at(0), u32_at(4)).map_err(runtime_error)?,
            action: match bytes[8] {
                0 => InputAction::Mode {
                    mode,
                    position: (bytes[10] & 1 != 0)
                        .then(|| u64::from_le_bytes(bytes[24..32].try_into().unwrap())),
                },
                1 => InputAction::Grab {
                    source: u32_at(12),
                    history: if bytes[10] & 16 != 0 {
                        Some(HistoryId {
                            scope: u32_at(44),
                            source: u32_at(12),
                            graph: Epoch(u64::from_le_bytes(bytes[48..56].try_into().unwrap())),
                            generation: u64::from_le_bytes(bytes[56..64].try_into().unwrap()),
                        })
                    } else {
                        None
                    },
                    sync: if bytes[10] & 8 != 0 {
                        Some(LoopId::from_parts(u32_at(16), u32_at(20)).map_err(runtime_error)?)
                    } else {
                        None
                    },
                    window: WindowSpec {
                        reverse_start_cycle: (bytes[10] & 1 != 0).then(|| u32_at(32)),
                        cycles_length: (bytes[10] & 2 != 0).then(|| u32_at(36)),
                        go_to_cycle: (bytes[10] & 4 != 0).then(|| u32_at(40)),
                        continue_recording: mode == Mode::Recording,
                    },
                    mode,
                    maximum_frames: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
                },
                _ => return Err(Error::InvalidEvent),
            },
        };
        let mut canonical = [0; INPUT_BATCH_RECORD_BYTES];
        edit.encode(&mut canonical)?;
        if bytes != canonical {
            return Err(Error::InvalidEvent);
        }
        Ok(edit)
    }
}
#[derive(Clone, Copy)]
struct Entry {
    edit: InputEdit,
    media: Option<media::MediaId>,
    pin: Option<media::PinId>,
    writer: Option<LiveWriterId>,
}
pub(super) struct InputBatch {
    entries: Vec<Entry>,
    requests: Vec<crate::Request>,
    ids: Box<[Option<RequestId>]>,
    witnesses: Vec<State>,
    requested: Frame,
}
impl InputBatch {
    pub fn prepare(slots: usize, dummy: LoopId) -> Result<Self, Error> {
        slots
            .checked_mul(
                size_of::<Entry>()
                    + size_of::<crate::Request>()
                    + size_of::<Option<RequestId>>()
                    + size_of::<State>(),
            )
            .filter(|bytes| *bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let mut entries = vec![
            Entry {
                edit: InputEdit {
                    target: dummy,
                    action: InputAction::Mode {
                        mode: Mode::Stopped,
                        position: None
                    }
                },
                media: None,
                pin: None,
                writer: None
            };
            slots
        ];
        let mut requests = vec![
            crate::Request {
                target: dummy,
                sequence: Sequence(0),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Stop
            };
            slots
        ];
        let mut witnesses = vec![State::new(dummy, 0); slots];
        for entry in &mut entries {
            core::hint::black_box(entry);
        }
        for request in &mut requests {
            core::hint::black_box(request);
        }
        for witness in &mut witnesses {
            core::hint::black_box(witness);
        }
        entries.clear();
        requests.clear();
        witnesses.clear();
        let mut ids = vec![None; slots].into_boxed_slice();
        for id in &mut ids {
            *core::hint::black_box(id) = None;
        }
        Ok(Self {
            entries,
            requests,
            ids,
            witnesses,
            requested: Frame(0),
        })
    }
}
impl LoopPlanner {
    fn prepare_grab_entry(
        &mut self,
        index: usize,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<Transition, Failure> {
        let entry = self.input_batch.entries[index];
        let InputAction::Grab {
            source,
            history,
            sync,
            window,
            mode,
            maximum_frames,
        } = entry.edit.action
        else {
            return Err(Error::InvalidEvent.into());
        };
        let row = self.row(entry.edit.target)?;
        let state = self.resolver.state(entry.edit.target)?;
        if row.prepared.is_some() {
            return Err(Error::Busy.into());
        }
        let sync_state = sync
            .map(|id| {
                self.resolver
                    .decisions()?
                    .iter()
                    .find(|decision| decision.state.id == id)
                    .map(|decision| decision.state)
                    .ok_or(crate::Error::InvalidId)
            })
            .transpose()?;
        if let Some(state) = sync_state {
            if !self
                .input_batch
                .witnesses
                .iter()
                .any(|old| old.id == state.id)
            {
                self.input_batch.witnesses.push(state);
            }
        }
        let shared = self.shared.as_mut().ok_or(Error::InvalidState)?;
        let status = shared.status(source, self.graph_epoch, io)?;
        if history.is_some_and(|history| history != status.id) {
            return Err(Error::StaleRevision.into());
        }
        if status.end != frame {
            return Err(Error::Busy.into());
        }
        let window = window.resolve(
            status.start..status.end,
            sync_state.map_or(0, |state| state.content_length),
            sync_state.map_or(0, |state| state.position),
        )?;
        if matches!(mode, Mode::Playing | Mode::Replacing)
            && (window.frames == 0 || window.go_to_frame >= window.frames)
            || mode == Mode::Recording
                && maximum_frames != 0
                && window
                    .frames
                    .checked_add(self.resolver.quantum().frames() as u64)
                    .is_none_or(|frames| maximum_frames < frames)
        {
            return Err(Error::InvalidFrame.into());
        }
        let take = shared.grab_window(status.id, &window, self.resolver.quantum().frames(), io)?;
        let mut snapshots = [None; 2];
        let result = (|| -> Result<Transition, Failure> {
            if (row.binding.channels > 0) != take.audio.is_some()
                || row.binding.midi != take.midi.is_some()
            {
                return Err(Error::InvalidShape.into());
            }
            let revision = state
                .revision_watermark
                .checked_next()
                .map_err(|_| Error::Exhausted)?;
            let mut windows = row.windows;
            for channel in windows
                .audio
                .iter_mut()
                .chain(core::iter::once(&mut windows.midi))
            {
                channel.start_offset = 0;
                channel.capture_alignment = 0;
                channel.raw_frames = None;
            }
            if matches!(mode, Mode::Recording | Mode::Replacing) {
                let fence = shared::Fence {
                    target: state.id,
                    revision: state.content_revision,
                    length: state.content_length,
                    timing_revision: row.timing_revision,
                    graph: self.graph_epoch,
                    topology: self.resolver.epoch(),
                };
                let kind = if mode == Mode::Recording {
                    indexed_midi::WriteKind::Growing { maximum_frames }
                } else {
                    indexed_midi::WriteKind::Fixed
                };
                let growth_owners = self.input_batch.entries[index..]
                    .iter()
                    .filter(|entry| {
                        matches!(
                            entry.edit.action,
                            InputAction::Grab {
                                mode: Mode::Recording,
                                maximum_frames: 0,
                                ..
                            }
                        )
                    })
                    .count()
                    .max(1);
                let id = self
                    .shared
                    .as_mut()
                    .ok_or(Error::InvalidState)?
                    .begin_live(
                        fence,
                        status.id,
                        take,
                        kind,
                        growth_owners,
                        windows,
                        frame,
                        io,
                    )?;
                self.input_batch.entries[index].writer = Some(id);
                if !self.shared.as_ref().unwrap().live_job(id)?.ready {
                    return Err(Error::Busy.into());
                }
            }
            for (member, resource) in [take.audio, take.midi].into_iter().enumerate() {
                if let Some(resource) = resource {
                    snapshots[member] = Some(io.snapshot(resource, frame)?);
                }
            }
            let source = |snapshot: Option<Snapshot>, io: &mut ResourceIo<'_>| {
                snapshot
                    .map(|snapshot| {
                        io.read_index(snapshot.id)
                            .map(|index| media::Source { snapshot, index })
                    })
                    .transpose()
            };
            let input = media::Input {
                description: media::Description {
                    owner: state.id,
                    revision,
                    frames: take.frames,
                    channels: row.binding.channels as u32,
                    midi: row.binding.midi,
                },
                audio: source(snapshots[0], io)?,
                midi: source(snapshots[1], io)?,
            };
            let media = self
                .ledger
                .adopt_windowed(io, input, windows)
                .map_err(|rejected| rejected.error)?;
            snapshots = [None; 2];
            self.input_batch.entries[index].media = Some(media);
            self.input_batch.entries[index].pin = Some(self.ledger.pin(media)?);
            Ok(Transition::Adopt {
                length: take.frames,
                revision,
                position: window.go_to_frame,
                mode,
            })
        })();
        for snapshot in snapshots.into_iter().flatten() {
            io.release_snapshot(snapshot.id)?;
        }
        for resource in [take.audio, take.midi].into_iter().flatten() {
            io.remove(resource)?;
        }
        result
    }
    pub(super) fn begin_input_batch(
        &mut self,
        count: usize,
        timing: Timing,
        payload: &[u8],
        requested: Frame,
        sequence: Sequence,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Failure> {
        if !self.input_batch.entries.is_empty() {
            return Err(Error::Busy.into());
        }
        if count == 0
            || count > self.rows.len()
            || payload.len() % INPUT_BATCH_RECORD_BYTES != 0
            || payload.len() > self.rows.len() * INPUT_BATCH_RECORD_BYTES
            || count
                .checked_mul(INPUT_BATCH_RECORD_BYTES)
                .is_none_or(|bytes| bytes > payload.len())
        {
            return Err(Error::InvalidShape.into());
        }
        if payload[count * INPUT_BATCH_RECORD_BYTES..]
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(Error::InvalidEvent.into());
        }
        let result = (|| -> Result<(), Failure> {
            for bytes in
                payload[..count * INPUT_BATCH_RECORD_BYTES].chunks_exact(INPUT_BATCH_RECORD_BYTES)
            {
                let edit = InputEdit::decode(bytes)?;
                if timing != Timing::At && matches!(edit.action, InputAction::Grab { .. }) {
                    return Err(Error::InvalidEvent.into());
                }
                self.row(edit.target)?;
                if self
                    .input_batch
                    .entries
                    .iter()
                    .any(|entry| entry.edit.target == edit.target)
                {
                    return Err(Error::InvalidId.into());
                }
                self.input_batch.entries.push(Entry {
                    edit,
                    media: None,
                    pin: None,
                    writer: None,
                });
            }
            self.resolver.preview_tick(&[])?;
            for index in 0..count {
                let entry = self.input_batch.entries[index];
                let transition = match entry.edit.action {
                    InputAction::Grab { .. } => self.prepare_grab_entry(index, frame, io)?,
                    InputAction::Mode { mode, position } => match mode {
                        Mode::Stopped => Transition::Stop,
                        Mode::Playing => Transition::Play {
                            position: position.unwrap_or(0),
                            retrigger: position.is_some(),
                        },
                        Mode::Recording => Transition::Record {
                            monitor_previous: false,
                        },
                        Mode::Replacing => Transition::ReplaceInput { position },
                    },
                };
                self.input_batch.requests.push(crate::Request {
                    target: entry.edit.target,
                    at: frame,
                    sequence,
                    timing,
                    transition,
                });
            }
            self.resolver.submit_batch(
                &self.input_batch.requests,
                &mut self.input_batch.ids[..count],
            )?;
            self.input_batch.requested = requested;
            Ok(())
        })();
        if result.is_err() {
            self.release_input_batch(io)?;
        }
        result
    }
    pub(super) fn input_batch_first_request(&self) -> Option<RequestId> {
        self.input_batch.ids.first().copied().flatten()
    }
    pub(super) fn input_batch_pin(&self, request: RequestId) -> Option<media::PinId> {
        self.input_batch
            .ids
            .iter()
            .position(|id| *id == Some(request))
            .and_then(|index| self.input_batch.entries.get(index))
            .and_then(|entry| entry.pin)
    }
    fn refuse_input_batch(&mut self) -> Result<bool, Error> {
        if self.failures.len() == self.failures.capacity() {
            return Err(Error::InvalidState);
        }
        let request = self.input_batch.ids[0].ok_or(Error::InvalidState)?;
        self.failures.push(crate::Observation::Refused {
            request,
            reason: crate::Error::InvalidState,
        });
        Ok(false)
    }
    pub(super) fn preflight_input_batch(&mut self) -> Result<bool, Error> {
        if self.input_batch.entries.is_empty()
            || self.failures.iter().any(|observation| {
                matches!(observation, crate::Observation::Refused { request, .. }
                if self.input_batch.ids.contains(&Some(*request)))
            })
        {
            return Ok(true);
        }
        if self
            .resolver
            .preview_program_change()
            .map_err(runtime_error)?
            .is_some()
        {
            return self.refuse_input_batch();
        }
        for witness in &self.input_batch.witnesses {
            if self
                .input_batch
                .entries
                .iter()
                .any(|entry| entry.edit.target == witness.id)
            {
                continue;
            }
            let decision = self
                .resolver
                .decisions()
                .map_err(runtime_error)?
                .iter()
                .find(|decision| decision.state.id == witness.id);
            if decision.is_none_or(|decision| decision.state != *witness) {
                return self.refuse_input_batch();
            }
        }
        Ok(true)
    }
    pub(super) fn input_batch_reply(&self) -> Result<Option<Reply>, Error> {
        let mut outcome = crate::Outcome::Applied {
            frame: self.resolver.frame(),
        };
        for id in &self.input_batch.ids[..self.input_batch.entries.len()] {
            let Some(completion) = self
                .resolver
                .completion(id.ok_or(Error::InvalidState)?)
                .map_err(runtime_error)?
            else {
                return Ok(None);
            };
            if !matches!(completion.outcome, crate::Outcome::Applied { .. }) {
                outcome = completion.outcome;
            }
        }
        Ok(Some(Reply::InputBatch {
            requested: self.input_batch.requested,
            effective: self.resolver.frame(),
            targets: self.input_batch.entries.len(),
            outcome,
        }))
    }
    pub(super) fn collect_input_batch(&mut self, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        for id in self.input_batch.ids[..self.input_batch.entries.len()]
            .iter()
            .flatten()
        {
            self.resolver
                .take_completion(*id)
                .map_err(runtime_error)?
                .ok_or(Error::InvalidState)?;
        }
        self.release_input_batch(io)
    }
    pub(super) fn release_input_batch(&mut self, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        for entry in &self.input_batch.entries {
            if let Some(writer) = entry.writer {
                let authoritative = self.rows[entry.edit.target.slot() as usize]
                    .and_then(|row| row.active)
                    .and_then(|take| take.writer)
                    == Some(writer);
                if !authoritative {
                    let shared = self.shared.as_mut().ok_or(Error::InvalidState)?;
                    match shared.live_job(writer) {
                        Ok(_) => shared.cancel_live(writer, io)?,
                        Err(Error::InvalidId) => {}
                        Err(error) => return Err(error),
                    }
                }
            }
            if let Some(pin) = entry.pin {
                self.ledger.release_pin(io, pin)?;
            }
            if let Some(media) = entry.media {
                self.ledger.retire(io, media)?;
            }
        }
        self.input_batch.entries.clear();
        self.input_batch.requests.clear();
        self.input_batch.ids.fill(None);
        self.input_batch.witnesses.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn input_records_require_canonical_tags_options_padding_and_exact_size() {
        let target = LoopId::from_parts(0, 1).unwrap();
        for edit in [
            InputEdit {
                target,
                action: InputAction::Mode {
                    mode: Mode::Stopped,
                    position: None,
                },
            },
            InputEdit {
                target,
                action: InputAction::Mode {
                    mode: Mode::Replacing,
                    position: Some(u64::MAX),
                },
            },
            InputEdit {
                target,
                action: InputAction::Grab {
                    source: u32::MAX,
                    history: None,
                    sync: Some(LoopId::from_parts(1, 2).unwrap()),
                    window: WindowSpec {
                        reverse_start_cycle: Some(0),
                        cycles_length: Some(u32::MAX),
                        go_to_cycle: Some(3),
                        continue_recording: true,
                    },
                    mode: Mode::Recording,
                    maximum_frames: u64::MAX,
                },
            },
        ] {
            assert_no_heap(|| {
                let mut record = [0; INPUT_BATCH_RECORD_BYTES];
                edit.encode(&mut record).unwrap();
                assert_eq!(InputEdit::decode(&record), Ok(edit));
                for offset in [8, 9, 10, 11, 44, 63] {
                    let mut invalid = record;
                    invalid[offset] = 255;
                    assert_eq!(InputEdit::decode(&invalid), Err(Error::InvalidEvent));
                }
                assert_eq!(InputEdit::decode(&record[..63]), Err(Error::InvalidShape));
            });
        }
    }
}
