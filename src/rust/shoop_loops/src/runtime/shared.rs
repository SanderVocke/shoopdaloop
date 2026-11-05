use super::*;
use core::sync::atomic::{AtomicU32, Ordering};
use rt_runtime::buffer::{AudioPorts, MidiBuffer};

static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug)]
pub struct HistorySettings {
    pub source: u32,
    pub audio: Option<shared_audio::Config>,
    pub midi: Option<shared_midi::Config>,
}
#[derive(Debug, Default)]
pub struct SharedSettings {
    pub histories: Vec<HistorySettings>,
    pub candidates: usize,
    pub edit_steps_per_callback: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryId {
    scope: u32,
    graph: Epoch,
    source: u32,
    generation: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryStatus {
    pub id: HistoryId,
    pub start: Frame,
    pub end: Frame,
    pub failure: Option<Error>,
}
/// Each resource is an independent caller-owned editable owner. Copying this
/// descriptor acquires no ownership; remove each resource exactly once.
/// Permanent planner shutdown reclaims any of these resources still alive,
/// including owners whose result was never read by the controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedTake {
    pub frames: u64,
    pub audio: Option<rt_runtime::ResourceId>,
    pub midi: Option<rt_runtime::ResourceId>,
    pub audio_store: Option<AudioStoreId>,
    pub midi_store: Option<MidiStoreId>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateId {
    scope: u32,
    slot: u32,
    generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateStatus {
    Building,
    Ready,
    Failed(Error),
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SharedWork {
    pub history_ticks: usize,
    pub history_failures: usize,
    pub edit_steps: usize,
    pub jobs_scanned: usize,
    pub background_bytes_remaining: usize,
    pub background_steps_remaining: usize,
}
/// Prepared storage without allocator overhead. Reserved payload is a subset of
/// the ordinary arena; MIDI state is a subset of store metadata.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SharedCapacity {
    pub coordinator_bytes: usize,
    pub store_metadata_bytes: usize,
    pub midi_state_bytes: usize,
    pub reserved_payload_bytes: usize,
    pub arena_payload_bytes: usize,
    pub sources: usize,
    pub candidates: usize,
}
struct History {
    committed_graph: Option<Epoch>,
    settings: HistorySettings,
    audio: Option<AudioStoreId>,
    midi: Option<MidiStoreId>,
    tracker: Option<midi::MidiCapture>,
    audio_scratch: Box<[f32]>,
    token: Option<u64>,
    midi_token: Option<u64>,
    generation: u64,
    observed: Frame,
    completed: Frame,
    staged: Option<Frame>,
    restart: bool,
    failure: Option<Error>,
    audio_closed: bool,
    midi_closed: bool,
}
#[derive(Clone, Copy)]
pub(super) struct Fence {
    pub timing_revision: u64,
    pub target: LoopId,
    pub revision: Revision,
    pub length: u64,
    pub graph: Epoch,
    pub topology: Epoch,
}
#[derive(Clone, Copy)]
pub(super) struct Candidate {
    pub steps: usize,
    pub dirty: bool,
    pub fence: Fence,
    pub original: SharedTake,
    pub original_audio: Option<Revision>,
    pub original_midi: Option<Revision>,
    pub take: SharedTake,
    pub midi_edit: Option<MidiEditId>,
    pub midi_import: Option<MidiImportId>,
    pub failure: Option<Error>,
    pub packet_bytes: usize,
}
struct Job {
    generation: u32,
    value: Option<Candidate>,
}
pub(super) struct Shared {
    scope: u32,
    histories: Vec<History>,
    lookup: Box<[Option<usize>]>,
    max_frames: usize,
    jobs: Box<[Job]>,
    owned: Box<[Option<rt_runtime::ResourceId>]>,
    building: usize,
    turn: usize,
    callback: Option<Frame>,
    allowance: usize,
    remaining: usize,
    audio_patch: Box<[f32]>,
    bytes: Box<[u8]>,
    midi_patch: MidiBuffer,
    work: SharedWork,
    closed: bool,
    capacity: SharedCapacity,
}

impl Shared {
    pub fn prepare(resources: &mut ResourceStore, settings: SharedSettings) -> Result<Self, Error> {
        let c = resources.config().limits();
        if settings.histories.len() > c.max_nodes
            || settings.candidates > c.resource_slots
            || settings.edit_steps_per_callback > i32::MAX as usize
            || settings
                .edit_steps_per_callback
                .checked_mul(settings.candidates)
                .is_none()
            || (settings.candidates > 0 && settings.edit_steps_per_callback == 0)
        {
            return Err(Error::InvalidConfig);
        }
        settings
            .candidates
            .checked_mul(size_of::<Job>())
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        c.max_nodes
            .checked_mul(size_of::<Option<usize>>())
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let scope = NEXT_SCOPE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        let mut histories = Vec::with_capacity(settings.histories.len());
        let mut lookup = vec![None; c.max_nodes].into_boxed_slice();
        let mut channels = 0;
        let mut events = 1;
        let mut bytes = 3;
        let mut packet = 3;
        for setting in settings.histories {
            if setting.source as usize >= lookup.len() {
                return Err(Error::InvalidConfig);
            }
            if histories
                .iter()
                .any(|h: &History| h.settings.source == setting.source)
                || (setting.audio.is_none() && setting.midi.is_none())
            {
                return Err(Error::InvalidConfig);
            }
            let count = setting.audio.map_or(0, |a| a.channels as usize);
            channels = channels.max(count);
            let audio = setting
                .audio
                .map(|a| resources.prepare_audio_store(a, Frame(0)))
                .transpose()?;
            let midi = setting
                .midi
                .map(|m| resources.prepare_midi_store(m, Frame(0), &MidiState::default()))
                .transpose()?;
            let tracker = setting
                .midi
                .map(|m| {
                    events = events.max(m.max_events);
                    bytes = bytes.max(m.max_bytes);
                    packet = packet.max(m.max_packet);
                    midi::MidiCapture::prepare(
                        c.max_frames,
                        c.midi_events_per_port,
                        c.midi_bytes_per_port,
                    )
                })
                .transpose()?;
            let samples = count
                .checked_mul(c.max_frames)
                .filter(|&n| n <= i32::MAX as usize / 4)
                .ok_or(Error::Capacity)?;
            lookup[setting.source as usize] = Some(histories.len());
            histories.push(History {
                committed_graph: None,
                settings: setting,
                audio,
                midi,
                tracker,
                audio_scratch: vec![0.; samples].into_boxed_slice(),
                token: None,
                midi_token: None,
                generation: 1,
                observed: Frame(0),
                completed: Frame(0),
                staged: None,
                restart: false,
                failure: None,
                audio_closed: false,
                midi_closed: false,
            });
        }
        let samples = channels
            .checked_mul(c.max_frames)
            .filter(|&n| n <= i32::MAX as usize / 4)
            .ok_or(Error::Capacity)?;
        let mut result = Self {
            scope,
            histories,
            lookup,
            max_frames: c.max_frames,
            jobs: (0..settings.candidates)
                .map(|_| Job {
                    generation: 1,
                    value: None,
                })
                .collect(),
            building: 0,
            owned: vec![None; c.resource_slots].into_boxed_slice(),
            turn: 0,
            callback: None,
            allowance: settings.edit_steps_per_callback,
            remaining: 0,
            audio_patch: vec![0.; samples].into_boxed_slice(),
            bytes: vec![0; (samples * 4).max(packet)].into_boxed_slice(),
            midi_patch: MidiBuffer::new(events, bytes)?,
            work: SharedWork::default(),
            closed: false,
            capacity: SharedCapacity::default(),
        };
        let mut capacity = SharedCapacity {
            coordinator_bytes: size_of::<Self>(),
            arena_payload_bytes: c
                .resource_pages
                .checked_mul(c.page_bytes)
                .ok_or(Error::Capacity)?,
            sources: result.histories.len(),
            candidates: result.jobs.len(),
            ..SharedCapacity::default()
        };
        let add = |total: &mut usize, count: usize, bytes: usize| -> Result<(), Error> {
            *total = total
                .checked_add(count.checked_mul(bytes).ok_or(Error::Capacity)?)
                .ok_or(Error::Capacity)?;
            Ok(())
        };
        for (count, bytes) in [
            (result.histories.capacity(), size_of::<History>()),
            (result.lookup.len(), size_of::<Option<usize>>()),
            (result.jobs.len(), size_of::<Job>()),
            (
                result.owned.len(),
                size_of::<Option<rt_runtime::ResourceId>>(),
            ),
            (result.audio_patch.len(), size_of::<f32>()),
            (result.bytes.len(), 1),
            (result.midi_patch.prepared_bytes(), 1),
        ] {
            add(&mut capacity.coordinator_bytes, count, bytes)?;
        }
        for history in &result.histories {
            add(
                &mut capacity.coordinator_bytes,
                history.audio_scratch.len(),
                size_of::<f32>(),
            )?;
            if history.tracker.is_some() {
                add(&mut capacity.coordinator_bytes, 1, size_of::<MidiState>())?;
            }
            if let Some(id) = history.audio {
                let c = resources.audio_store_capacity(id)?;
                add(&mut capacity.store_metadata_bytes, 1, c.metadata_bytes)?;
                add(
                    &mut capacity.store_metadata_bytes,
                    1,
                    size_of::<shared_audio::AudioStore>(),
                )?;
                add(
                    &mut capacity.reserved_payload_bytes,
                    history.settings.audio.expect("audio settings").pages,
                    resources.config().limits().page_bytes,
                )?;
            }
            if let Some(id) = history.midi {
                let c = resources.midi_store_capacity(id)?;
                add(&mut capacity.store_metadata_bytes, 1, c.metadata_bytes)?;
                add(&mut capacity.midi_state_bytes, 1, c.state_bytes)?;
                add(
                    &mut capacity.reserved_payload_bytes,
                    history.settings.midi.expect("MIDI settings").pages,
                    resources.config().limits().page_bytes,
                )?;
            }
        }
        capacity
            .coordinator_bytes
            .checked_add(capacity.store_metadata_bytes)
            .ok_or(Error::Capacity)?;
        result.capacity = capacity;
        for h in &mut result.histories {
            for v in h.audio_scratch.iter_mut() {
                core::hint::black_box(v);
            }
        }
        for v in result.audio_patch.iter_mut() {
            core::hint::black_box(v);
        }
        for v in result.bytes.iter_mut() {
            core::hint::black_box(v);
        }
        for j in result.jobs.iter_mut() {
            core::hint::black_box(j);
        }
        Ok(result)
    }
    fn source(&self, key: u32) -> Result<&History, Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        let slot = self
            .lookup
            .get(key as usize)
            .copied()
            .flatten()
            .ok_or(Error::InvalidId)?;
        Ok(&self.histories[slot])
    }
    pub fn stores(&self, key: u32) -> Result<(Option<AudioStoreId>, Option<MidiStoreId>), Error> {
        let h = self.source(key)?;
        Ok((h.audio, h.midi))
    }
    pub fn status(
        &self,
        key: u32,
        graph: Epoch,
        io: &ResourceIo<'_>,
    ) -> Result<HistoryStatus, Error> {
        let h = self.source(key)?;
        let eligible = h.committed_graph == Some(graph);
        let mut start = h.completed;
        let mut any = false;
        for range in [
            h.audio.map(|id| io.audio_coverage(id)).transpose()?,
            h.midi.map(|id| io.midi_coverage(id)).transpose()?,
        ]
        .into_iter()
        .flatten()
        {
            start = if any {
                start.max(range.start)
            } else {
                range.start
            };
            any = true;
        }
        Ok(HistoryStatus {
            id: HistoryId {
                scope: self.scope,
                graph,
                source: key,
                generation: h.generation,
            },
            start: if eligible {
                start.min(h.completed)
            } else {
                h.completed
            },
            end: h.completed,
            failure: h
                .failure
                .or(if eligible { None } else { Some(Error::Busy) }),
        })
    }
    pub fn observe(
        &mut self,
        source: u32,
        token: u64,
        midi_token: u64,
        frame: Frame,
        q: usize,
        ports: &mut AudioPorts<'_>,
        midi: Option<&MidiBuffer>,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        let Some(slot) = self.lookup.get(source as usize).copied().flatten() else {
            return Ok(());
        };
        if q == 0 {
            return Ok(());
        }
        if q > self.max_frames {
            return Err(Error::InvalidFrame);
        }
        let h = &mut self.histories[slot];
        if token == 0 || midi_token == 0 {
            return Err(Error::Exhausted);
        }
        let end = frame.checked_add(q)?;
        if h.token == Some(token) && h.observed > frame {
            return Err(Error::InvalidFrame);
        }
        let changed = h.token.is_some_and(|old| old != token);
        if h.midi_token.is_some_and(|old| old != midi_token) {
            if let Some(tracker) = &mut h.tracker {
                tracker.reset();
            }
        }
        if changed || h.restart || h.observed != frame {
            h.generation = h.generation.checked_add(1).ok_or(Error::Exhausted)?;
            if let Some(audio) = h.audio {
                io.render().audio_history_discontinuity(audio, frame)?;
            }
            if let (Some(store), Some(tracker)) = (h.midi, &h.tracker) {
                io.render()
                    .midi_history_discontinuity(store, frame, tracker.state())?;
            }
            h.completed = frame;
            h.restart = false;
        }
        h.token = Some(token);
        h.midi_token = Some(midi_token);
        h.observed = end;
        h.staged = None;
        self.work.history_ticks = self.work.history_ticks.saturating_add(1);
        let result = (|| {
            if let Some(tracker) = &mut h.tracker {
                tracker.process(&mut io.render(), midi.ok_or(Error::InvalidShape)?, q, None)?;
            }
            if let Some(store) = h.audio {
                let channels = h.settings.audio.expect("audio settings").channels as usize;
                for channel in 0..channels {
                    let input = ports.output(channel).ok_or(Error::InvalidShape)?;
                    if input.len() != q {
                        return Err(Error::InvalidShape);
                    }
                    for sample in 0..q {
                        h.audio_scratch[sample * channels + channel] = input[sample];
                    }
                }
                io.render()
                    .append_audio_history(store, frame, &h.audio_scratch[..q * channels])?;
            }
            if let Some(store) = h.midi {
                io.render().append_midi_history(
                    store,
                    frame,
                    q,
                    midi.ok_or(Error::InvalidShape)?,
                )?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                h.staged = Some(end);
                h.failure = None;
            }
            Err(error) => {
                h.restart = true;
                h.failure = Some(error);
                self.work.history_failures = self.work.history_failures.saturating_add(1);
            }
        }
        Ok(())
    }
    pub fn finish(&mut self, end: Frame, graph: Epoch) {
        for h in &mut self.histories {
            if h.staged.take() == Some(end) {
                h.completed = end;
                h.committed_graph = Some(graph);
            }
        }
    }
    pub fn grab(
        &mut self,
        id: HistoryId,
        start: Frame,
        end: Frame,
        q: usize,
        io: &mut ResourceIo<'_>,
    ) -> Result<SharedTake, Error> {
        let status = self.status(id.source, id.graph, io)?;
        if status.id != id {
            return Err(Error::InvalidId);
        }
        if start >= end
            || start < status.start
            || end > status.end
            || (end.0 - start.0) % q as u64 != 0
        {
            return Err(Error::InvalidFrame);
        }
        let h = self.source(id.source)?;
        let audio = h
            .audio
            .map(|store| io.grab_audio(store, start..end))
            .transpose()?;
        let midi = match h
            .midi
            .map(|store| io.grab_midi(store, start..end))
            .transpose()
        {
            Ok(midi) => midi,
            Err(error) => {
                if let Some(audio) = audio {
                    io.remove(audio)?;
                }
                return Err(error);
            }
        };
        let take = SharedTake {
            frames: end.0 - start.0,
            audio,
            midi,
            audio_store: h.audio,
            midi_store: h.midi,
        };
        self.track(take);
        Ok(take)
    }
    pub fn candidate(&self, id: CandidateId) -> Result<Candidate, Error> {
        if self.closed || id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.jobs
            .get(id.slot as usize)
            .filter(|j| j.generation == id.generation)
            .and_then(|j| j.value)
            .ok_or(Error::InvalidId)
    }
    pub fn candidate_status(&self, id: CandidateId) -> Result<CandidateStatus, Error> {
        let j = self.candidate(id)?;
        Ok(j.failure.map_or(
            if j.midi_edit.is_some() || j.midi_import.is_some() {
                CandidateStatus::Building
            } else {
                CandidateStatus::Ready
            },
            CandidateStatus::Failed,
        ))
    }
    pub fn begin(
        &mut self,
        fence: Fence,
        source: SharedTake,
        packet_bytes: usize,
        io: &mut ResourceIo<'_>,
        frame: Frame,
    ) -> Result<CandidateId, Error> {
        if self.closed || source.frames != fence.length || source.frames == 0 {
            return Err(Error::InvalidFrame);
        }
        let slot = self
            .jobs
            .iter()
            .position(|j| j.value.is_none() && j.generation != 0)
            .ok_or(Error::Capacity)?;
        if let Some(store) = source.midi_store {
            let h = self
                .histories
                .iter()
                .find(|h| h.midi == Some(store))
                .ok_or(Error::InvalidId)?;
            if h.settings.midi.expect("MIDI settings").max_packet > packet_bytes {
                return Err(Error::Capacity);
            }
        }
        let original_audio = source
            .audio
            .map(|id| io.info(id).map(|i| i.revision))
            .transpose()?;
        let original_midi = source
            .midi
            .map(|id| io.info(id).map(|i| i.revision))
            .transpose()?;
        let fork_audio =
            |io: &mut ResourceIo<'_>| -> Result<Option<rt_runtime::ResourceId>, Error> {
                match (source.audio_store, source.audio) {
                    (Some(store), Some(resource)) => {
                        let snap = io.snapshot(resource, frame)?;
                        let result = io.fork_audio(store, snap.id);
                        io.release_snapshot(snap.id)?;
                        result.map(Some)
                    }
                    (None, None) => Ok(None),
                    _ => Err(Error::InvalidShape),
                }
            };
        let audio = fork_audio(io)?;
        let mut midi_import = None;
        let midi = match (source.midi_store, source.midi) {
            (Some(store), Some(resource)) => (|| {
                let snap = io.snapshot(resource, frame)?;
                let result = match io.fork_midi(store, snap.id) {
                    Ok(resource) => Ok(Some(resource)),
                    Err(Error::InvalidShape) => io.prepare_midi_import(store, snap.id).map(|id| {
                        midi_import = Some(id);
                        None
                    }),
                    Err(error) => Err(error),
                };
                io.release_snapshot(snap.id)?;
                result
            })(),
            (None, None) => Ok(None),
            _ => Err(Error::InvalidShape),
        };
        let midi = match midi {
            Ok(midi) => midi,
            Err(error) => {
                if let Some(audio) = audio {
                    io.remove(audio)?;
                }
                return Err(error);
            }
        };
        self.jobs[slot].value = Some(Candidate {
            steps: 0,
            dirty: false,
            fence,
            original: source,
            original_audio,
            original_midi,
            take: SharedTake {
                audio,
                midi,
                ..source
            },
            midi_edit: None,
            midi_import,
            failure: None,
            packet_bytes,
        });
        if midi_import.is_some() {
            self.building += 1;
        }
        self.track(SharedTake {
            audio,
            midi,
            ..source
        });
        Ok(CandidateId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.jobs[slot].generation,
        })
    }
    pub fn audio(
        &mut self,
        id: CandidateId,
        position: u64,
        input: SnapshotId,
        offset: u64,
        frames: usize,
        q: usize,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let job = self.candidate(id)?;
        if self.candidate_status(id)? != CandidateStatus::Ready {
            return Err(Error::Busy);
        }
        if frames > q {
            return Err(Error::InvalidFrame);
        }
        let resource = job.take.audio.ok_or(Error::InvalidShape)?;
        let info = io.info(resource)?;
        let Format::Audio { channels } = info.description.format else {
            return Err(Error::InvalidShape);
        };
        let index = io.read_index(input)?;
        let snap = io.render().snapshot(index)?;
        if snap.description.format != info.description.format
            || offset
                .checked_add(frames as u64)
                .is_none_or(|end| end > snap.description.frames)
        {
            return Err(Error::InvalidShape);
        }
        let count = frames
            .checked_mul(channels as usize)
            .filter(|&n| n <= self.audio_patch.len())
            .ok_or(Error::Capacity)?;
        let bytes = count
            .checked_mul(4)
            .filter(|&n| n <= self.bytes.len())
            .ok_or(Error::Capacity)?;
        let at = usize::try_from(offset)
            .ok()
            .and_then(|n| n.checked_mul(channels as usize * 4))
            .ok_or(Error::Capacity)?;
        let mut cursor = io.render().cursor_at(index, at)?;
        let mut copied = 0;
        while copied < bytes {
            let n = io
                .render()
                .read(&mut cursor, &mut self.bytes[copied..bytes])?;
            if n == 0 {
                return Err(Error::InvalidState);
            }
            copied += n;
        }
        for i in 0..count {
            self.audio_patch[i] =
                f32::from_le_bytes(self.bytes[i * 4..i * 4 + 4].try_into().expect("four bytes"));
        }
        io.render().replace_audio(
            resource,
            info.revision,
            position,
            &self.audio_patch[..count],
        )?;
        if frames > 0 {
            self.jobs[id.slot as usize]
                .value
                .as_mut()
                .expect("candidate")
                .dirty = true;
        }
        Ok(())
    }
    pub fn midi(
        &mut self,
        id: CandidateId,
        position: u64,
        input: SnapshotId,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let mut job = self.candidate(id)?;
        if self.candidate_status(id)? != CandidateStatus::Ready {
            return Err(Error::Busy);
        }
        let resource = job.take.midi.ok_or(Error::InvalidShape)?;
        let index = io.read_index(input)?;
        let snap = io.render().snapshot(index)?;
        if snap.description.format != Format::Midi {
            return Err(Error::InvalidShape);
        }
        if snap.description.frames == 0 {
            return Ok(());
        }
        let initial = io.render().midi_seek(index, 0)?;
        let mut cursor = initial.cursor;
        self.midi_patch.clear();
        while let Some(event) = io.render().midi_next(&mut cursor)? {
            if event.byte_len() > job.packet_bytes || event.byte_len() > self.bytes.len() {
                return Err(Error::Capacity);
            }
            let count = io
                .render()
                .midi_read(event, &mut self.bytes[..event.byte_len()])?;
            let offset = u32::try_from(event.frame()).map_err(|_| Error::InvalidFrame)?;
            self.midi_patch.push(
                offset,
                usize::try_from(snap.description.frames).map_err(|_| Error::Capacity)?,
                &self.bytes[..count],
            )?;
        }
        let revision = io.info(resource)?.revision;
        job.midi_edit = Some(io.render().prepare_midi_replace(
            resource,
            revision,
            position,
            usize::try_from(snap.description.frames).map_err(|_| Error::Capacity)?,
            &self.midi_patch,
            &initial.state,
        )?);
        job.dirty = true;
        self.building += 1;
        self.jobs[id.slot as usize].value = Some(job);
        Ok(())
    }
    pub fn progress(&mut self, callback: Frame, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        if self.callback != Some(callback) {
            self.callback = Some(callback);
            self.remaining = self.allowance;
            self.work = SharedWork::default();
        }
        while self.remaining > 0 && self.building > 0 {
            let mut selected = None;
            for _ in 0..self.jobs.len() {
                let slot = self.turn;
                self.turn = (slot + 1) % self.jobs.len();
                self.work.jobs_scanned = self.work.jobs_scanned.saturating_add(1);
                if self.jobs[slot].value.is_some_and(|j| {
                    (j.midi_edit.is_some() || j.midi_import.is_some()) && j.failure.is_none()
                }) {
                    selected = Some(slot);
                    break;
                }
            }
            let Some(slot) = selected else {
                break;
            };
            self.remaining -= 1;
            self.work.edit_steps += 1;
            let mut job = self.jobs[slot].value.expect("selected job");
            job.steps = job.steps.saturating_add(1);
            if let Some(import) = job.midi_import {
                match io.render().progress_midi_import(import, 1) {
                    Ok(shared_midi::EditStatus::Building) => {}
                    Ok(shared_midi::EditStatus::Ready) => match io.finish_midi_import(import) {
                        Ok(resource) => {
                            job.take.midi = Some(resource);
                            job.midi_import = None;
                            self.track(job.take);
                        }
                        Err(error) => job.failure = Some(error),
                    },
                    Err(error) => job.failure = Some(error),
                }
            } else {
                let edit = job.midi_edit.expect("building edit");
                match io.render().progress_midi_edit(edit, 1) {
                    Ok(shared_midi::EditStatus::Building) => {}
                    Ok(shared_midi::EditStatus::Ready) => {
                        match io.render().commit_midi_edit(edit) {
                            Ok(_) => job.midi_edit = None,
                            Err(error) => job.failure = Some(error),
                        }
                    }
                    Err(error) => job.failure = Some(error),
                }
            }
            self.jobs[slot].value = Some(job);
            if (job.midi_edit.is_none() && job.midi_import.is_none()) || job.failure.is_some() {
                self.building -= 1;
            }
        }
        Ok(())
    }
    pub fn work(&self) -> SharedWork {
        self.work
    }
    pub(super) fn audio_store(&self, source: u32) -> Result<AudioStoreId, Error> {
        self.lookup
            .get(source as usize)
            .copied()
            .flatten()
            .and_then(|index| self.histories[index].audio)
            .ok_or(Error::InvalidShape)
    }
    pub fn capacity(&self) -> SharedCapacity {
        self.capacity
    }
    pub fn validate_original(job: Candidate, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        for (resource, revision) in [
            (job.original.audio, job.original_audio),
            (job.original.midi, job.original_midi),
        ] {
            if let (Some(resource), Some(revision)) = (resource, revision) {
                io.validate_writer(resource, revision)?;
            }
        }
        Ok(())
    }
    pub fn take(&mut self, id: CandidateId) -> Result<Candidate, Error> {
        let value = self.candidate(id)?;
        if (value.midi_edit.is_some() || value.midi_import.is_some()) && value.failure.is_none() {
            self.building -= 1;
        }
        self.jobs[id.slot as usize].value = None;
        self.jobs[id.slot as usize].generation = self.jobs[id.slot as usize]
            .generation
            .checked_add(1)
            .unwrap_or(0);
        Ok(value)
    }
    pub fn cancel(&mut self, id: CandidateId, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        let job = self.candidate(id)?;
        if let Some(edit) = job.midi_edit {
            io.cancel_midi_edit(edit)?;
        }
        if let Some(import) = job.midi_import {
            io.cancel_midi_import(import)?;
        }
        for resource in [job.take.audio, job.take.midi].into_iter().flatten() {
            io.remove(resource)?;
            self.owned[resource.slot() as usize] = None;
        }
        self.take(id)?;
        Ok(())
    }
    fn track(&mut self, take: SharedTake) {
        for resource in [take.audio, take.midi].into_iter().flatten() {
            self.owned[resource.slot() as usize] = Some(resource);
        }
    }
    pub fn cleanup(&mut self, resources: &mut ResourceStore) -> Result<(), Error> {
        for slot in 0..self.jobs.len() {
            if self.jobs[slot].value.is_some() {
                self.cancel(
                    CandidateId {
                        scope: self.scope,
                        slot: slot as u32,
                        generation: self.jobs[slot].generation,
                    },
                    &mut resources.io(),
                )?;
            }
        }
        for slot in 0..self.owned.len() {
            if let Some(resource) = self.owned[slot] {
                match resources.info(resource) {
                    Ok(_) => resources.remove_with_uploads(resource)?,
                    Err(Error::InvalidId) => {}
                    Err(error) => return Err(error),
                }
                self.owned[slot] = None;
            }
        }
        let mut busy = false;
        for h in &mut self.histories {
            if let Some(audio) = h.audio.filter(|_| !h.audio_closed) {
                match resources.cleanup_audio_store(audio) {
                    Ok(()) => h.audio_closed = true,
                    Err(Error::Busy) => busy = true,
                    Err(e) => return Err(e),
                }
            }
            if let Some(midi) = h.midi.filter(|_| !h.midi_closed) {
                match resources.cleanup_midi_store(midi) {
                    Ok(()) => h.midi_closed = true,
                    Err(Error::Busy) => busy = true,
                    Err(e) => return Err(e),
                }
            }
        }
        if busy {
            return Err(Error::Busy);
        }
        self.closed = true;
        Ok(())
    }
}

impl LoopPlanner {
    pub(super) fn shared_operation(
        &mut self,
        operation: Operation,
        frame: Frame,
        graph: Epoch,
        io: &mut ResourceIo<'_>,
    ) -> Result<Reply, Failure> {
        let q = self.resolver.quantum().frames();
        match operation {
            Operation::SharedCapacity => Ok(Reply::SharedCapacity(
                self.shared.as_ref().ok_or(Error::InvalidState)?.capacity(),
            )),
            Operation::SharedWork => {
                let mut work = self.shared.as_ref().ok_or(Error::InvalidState)?.work();
                work.background_bytes_remaining = io.copy_remaining();
                work.background_steps_remaining = io.steps_remaining();
                Ok(Reply::SharedWork(work))
            }
            Operation::HistoryStatus { source } => Ok(Reply::History(
                self.shared
                    .as_ref()
                    .ok_or(Error::InvalidState)?
                    .status(source, graph, io)?,
            )),
            Operation::HistoryGrab { source, start, end } => {
                if source.graph != graph {
                    return Err(Error::StaleEpoch.into());
                }
                if end > frame {
                    return Err(Error::InvalidFrame.into());
                }
                Ok(Reply::Grabbed(
                    self.shared
                        .as_mut()
                        .ok_or(Error::InvalidState)?
                        .grab(source, start, end, q, io)?,
                ))
            }
            Operation::CandidateBegin { target, take } => {
                let row = self.row(target)?;
                let state = self.resolver.state(target)?;
                if row.dry_wet
                    || row.active.is_some()
                    || row.prepared.is_some()
                    || state.mode.records()
                    || (row.binding.channels > 0) != take.audio.is_some()
                    || row.binding.midi != take.midi.is_some()
                {
                    return Err(Error::Busy.into());
                }
                if let Some(audio) = take.audio {
                    let info = io.info(audio)?;
                    if info.description.format
                        != (Format::Audio {
                            channels: row.binding.channels as u32,
                        })
                        || info.description.frames != take.frames
                    {
                        return Err(Error::InvalidShape.into());
                    }
                }
                if let Some(midi) = take.midi {
                    let info = io.info(midi)?;
                    if info.description.format != Format::Midi
                        || info.description.frames != take.frames
                    {
                        return Err(Error::InvalidShape.into());
                    }
                }
                let fence = Fence {
                    timing_revision: row.timing_revision,
                    target,
                    revision: state.content_revision,
                    length: state.content_length,
                    graph,
                    topology: self.resolver.epoch(),
                };
                let shared = self.shared.as_mut().ok_or(Error::InvalidState)?;
                let id = shared.begin(fence, take, row.binding.packet_bytes, io, frame)?;
                Ok(Reply::Candidate {
                    id,
                    status: shared.candidate_status(id)?,
                    steps: 0,
                })
            }
            Operation::CandidateAudio {
                id,
                position,
                input,
                offset,
                frames,
            } => {
                let shared = self.shared.as_mut().ok_or(Error::InvalidState)?;
                shared.audio(id, position, input, offset, frames, q, io)?;
                Ok(Reply::Candidate {
                    id,
                    status: shared.candidate_status(id)?,
                    steps: shared.candidate(id)?.steps,
                })
            }
            Operation::CandidateMidi {
                id,
                position,
                input,
            } => {
                let shared = self.shared.as_mut().ok_or(Error::InvalidState)?;
                shared.midi(id, position, input, io)?;
                Ok(Reply::Candidate {
                    id,
                    status: shared.candidate_status(id)?,
                    steps: shared.candidate(id)?.steps,
                })
            }
            Operation::CandidateStatus { id } => Ok(Reply::Candidate {
                id,
                steps: self
                    .shared
                    .as_ref()
                    .ok_or(Error::InvalidState)?
                    .candidate(id)?
                    .steps,
                status: self
                    .shared
                    .as_ref()
                    .ok_or(Error::InvalidState)?
                    .candidate_status(id)?,
            }),
            Operation::CandidateCancel { id } => {
                self.shared
                    .as_mut()
                    .ok_or(Error::InvalidState)?
                    .cancel(id, io)?;
                Ok(Reply::Done)
            }
            Operation::CandidatePublish { id } => self.publish_candidate(id, frame, graph, io),
            _ => Err(Error::InvalidEvent.into()),
        }
    }
    fn publish_candidate(
        &mut self,
        id: CandidateId,
        frame: Frame,
        graph: Epoch,
        io: &mut ResourceIo<'_>,
    ) -> Result<Reply, Failure> {
        let shared = self.shared.as_ref().ok_or(Error::InvalidState)?;
        if shared.candidate_status(id)? != CandidateStatus::Ready {
            return Err(Error::Busy.into());
        }
        let job = shared.candidate(id)?;
        if !job.dirty {
            return Err(Error::InvalidState.into());
        }
        let row = self.row(job.fence.target)?;
        let state = self.resolver.state(job.fence.target)?;
        if graph != job.fence.graph || self.resolver.epoch() != job.fence.topology {
            return Err(Error::StaleEpoch.into());
        }
        if row.timing_revision != job.fence.timing_revision
            || state.content_revision != job.fence.revision
            || state.content_length != job.fence.length
        {
            return Err(Error::StaleRevision.into());
        }
        if row.active.is_some()
            || row.prepared.is_some()
            || row.interval.is_some()
            || state.mode.records()
        {
            return Err(Error::Busy.into());
        }
        Shared::validate_original(job, io)?;
        let revision = state.revision_watermark.checked_next()?;
        let source = |io: &mut ResourceIo<'_>,
                      resource: Option<rt_runtime::ResourceId>|
         -> Result<Option<media::Source>, Error> {
            resource
                .map(|resource| {
                    let snapshot = io.snapshot(resource, frame)?;
                    match io.read_index(snapshot.id) {
                        Ok(index) => Ok(media::Source { snapshot, index }),
                        Err(error) => {
                            io.release_snapshot(snapshot.id)?;
                            Err(error)
                        }
                    }
                })
                .transpose()
        };
        let audio = source(io, job.take.audio)?;
        let midi = match source(io, job.take.midi) {
            Ok(midi) => midi,
            Err(error) => {
                if let Some(audio) = audio {
                    io.release_snapshot(audio.snapshot.id)?;
                }
                return Err(error.into());
            }
        };
        let input = media::Input {
            auxiliary: None,
            description: media::Description {
                auxiliary_channels: 0,
                owner: job.fence.target,
                revision,
                frames: job.take.frames,
                channels: row.binding.channels as u32,
                midi: row.binding.midi,
            },
            audio,
            midi,
        };
        let media = match self.ledger.adopt(io, input) {
            Ok(media) => media,
            Err(rejected) => {
                for source in [rejected.input.audio, rejected.input.midi]
                    .into_iter()
                    .flatten()
                {
                    io.release_snapshot(source.snapshot.id)?;
                }
                return Err(rejected.error.into());
            }
        };
        let result = (|| -> Result<(), Error> {
            let activation = self.ledger.prepare_playback_activation(
                io,
                core::iter::once(media::PlaybackChange {
                    owner: job.fence.target,
                    media,
                }),
            )?;
            self.resolver
                .replace_content(crate::ContentRetime {
                    target: job.fence.target,
                    expected_revision: job.fence.revision,
                    expected_length: job.fence.length,
                    revision,
                    length: job.take.frames,
                    restart: false,
                })
                .map_err(runtime_error)?;
            activation.commit();
            Ok(())
        })();
        if let Err(error) = result {
            self.ledger.retire(io, media)?;
            return Err(error.into());
        }
        self.ledger.retire(io, media)?;
        self.shared.as_mut().expect("shared state").take(id)?;
        Ok(Reply::CandidatePublished {
            id,
            take: job.take,
            media,
            frame,
        })
    }
    /// Host preparation only, before ownership transfers to the fixed runtime.
    pub fn prepare_shared(
        &mut self,
        resources: &mut ResourceStore,
        settings: SharedSettings,
    ) -> Result<(), Error> {
        if self.shared.is_some() {
            return Err(Error::InvalidState);
        }
        self.shared = Some(Shared::prepare(resources, settings)?);
        Ok(())
    }
    pub fn shared_stores(
        &self,
        source: u32,
    ) -> Result<(Option<AudioStoreId>, Option<MidiStoreId>), Error> {
        self.shared
            .as_ref()
            .ok_or(Error::InvalidState)?
            .stores(source)
    }
    /// Inspect only committed coverage, including after a terminal render fault.
    pub fn history_status(&self, source: u32, io: &ResourceIo<'_>) -> Result<HistoryStatus, Error> {
        self.shared
            .as_ref()
            .ok_or(Error::InvalidState)?
            .status(source, self.graph_epoch, io)
    }
    /// Observe the prepared processor's capture-feed outputs once per whole tick.
    pub fn observe_history(
        &mut self,
        source: u32,
        token: u64,
        midi_token: u64,
        frame: Frame,
        q: usize,
        ports: &mut AudioPorts<'_>,
        midi: Option<&MidiBuffer>,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        if let Some(shared) = &mut self.shared {
            shared.observe(source, token, midi_token, frame, q, ports, midi, io)?;
        }
        Ok(())
    }
}
