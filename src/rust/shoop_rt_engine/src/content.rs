//! Prepared upload ownership. New content stays inaudible until a later commit.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use crate::endpoint::{
    completion_refusal, execution_outcome, ControlEndpoint, Purpose, RuntimeResult,
};
use rt_runtime::{executor::Command, resource, ResourceId, Revision};
use shoop_loops::runtime::{NodePairSourceId, NodePairSourceStatus, Operation, Reply};
use shoop_rt_protocol::{
    content::MAX_CONTENT_BYTES, ContentDescription, Outcome, Refusal, Request,
};

pub(crate) const RUNTIME_UPLOAD_BUFFERS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Step {
    Window,
    Create,
    Begin,
    Write,
    Finish,
    Commit,
    Snapshot,
    Validate,
    Release,
    Cancel,
    Remove,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Create,
    Begin,
    Receiving,
    Finish,
    Commit,
    Snapshot,
    Validate,
    Ready,
    Cleanup,
}
#[derive(Clone, Copy)]
struct Content {
    windows: Option<(u64, shoop_rt_protocol::PlaybackWindows)>,
    description: ContentDescription,
    resource: Option<ResourceId>,
    upload: Option<resource::UploadId>,
    source: Option<NodePairSourceId>,
    position: u64,
    pending: usize,
    correlation: Option<u64>,
    phase: Phase,
    failure: Option<Refusal>,
    abandoned: bool,
    blocked: bool,
}
pub(crate) struct Uploads {
    slots: Box<[Option<Content>]>,
    buffers: Vec<Box<[u8]>>,
    initials: Vec<Box<rt_runtime::midi_state::MidiState>>,
    last_id: u64,
}
impl Uploads {
    pub(crate) fn clear_blocked(&mut self) {
        for entry in self.slots.iter_mut().flatten() {
            entry.blocked = false;
        }
    }
    pub(crate) fn blocked(&self, id: u64) -> bool {
        self.slots
            .iter()
            .flatten()
            .any(|entry| entry.description.id == id && entry.blocked)
    }
    pub(crate) fn block(&mut self, id: u64) {
        if let Some(entry) = self
            .slots
            .iter_mut()
            .flatten()
            .find(|entry| entry.description.id == id)
        {
            entry.blocked = true;
        }
    }
    pub(crate) fn source(
        &self,
        id: u64,
        midi: bool,
        frames: u64,
    ) -> Result<NodePairSourceId, Refusal> {
        let entry = self.slots[self.index(id)?]
            .as_ref()
            .expect("located upload");
        if let Some(failure) = entry.failure {
            return Err(failure);
        }
        if entry.phase != Phase::Ready || entry.abandoned || entry.pending != 0 {
            return Err(Refusal::Busy);
        }
        if entry.description.midi != midi
            || entry
                .windows
                .map_or(entry.description.frames, |(frames, _)| frames)
                != frames
        {
            return Err(Refusal::InvalidValue);
        }
        entry.source.ok_or(Refusal::InvalidId)
    }
    pub(crate) fn source_raw(
        &self,
        id: u64,
        midi: bool,
        frames: u64,
    ) -> Result<NodePairSourceId, Refusal> {
        if self.slots[self.index(id)?]
            .as_ref()
            .ok_or(Refusal::InvalidId)?
            .description
            .frames
            != frames
        {
            return Err(Refusal::InvalidValue);
        }
        self.source(
            id,
            midi,
            self.slots[self.index(id)?]
                .unwrap()
                .windows
                .map_or(frames, |(frames, _)| frames),
        )
    }
    pub(crate) fn timing(
        &self,
        audio: Option<u64>,
        midi: Option<u64>,
        frames: u64,
    ) -> Result<shoop_rt_protocol::TimingState, Refusal> {
        let mut state = shoop_rt_protocol::TimingState {
            length: frames,
            ..Default::default()
        };
        let mut windows = None;
        let mut unset = false;
        for (id, midi) in [(audio, false), (midi, true)] {
            let Some(id) = id else {
                continue;
            };
            let entry = self.slots[self.index(id)?].ok_or(Refusal::InvalidId)?;
            match entry.windows {
                Some((length, window)) if length == frames => {
                    if windows.is_some_and(|previous| previous != window) {
                        return Err(Refusal::InvalidValue);
                    }
                    windows = Some(window);
                }
                Some(_) => return Err(Refusal::InvalidValue),
                None => unset = true,
            }
            if midi {
                state.raw_midi_frames = entry.description.frames;
            } else {
                state.raw_audio_frames = entry.description.frames;
            }
        }
        if windows.is_some() && unset {
            return Err(Refusal::InvalidValue);
        }
        state.windows = windows.unwrap_or_default();
        Ok(state)
    }
    pub(crate) fn source_timing(
        &self,
        audio: Option<NodePairSourceId>,
        midi: Option<NodePairSourceId>,
        frames: u64,
    ) -> Result<shoop_rt_protocol::TimingState, Refusal> {
        let token = |source: Option<NodePairSourceId>| -> Result<Option<u64>, Refusal> {
            source
                .map(|source| {
                    self.slots
                        .iter()
                        .flatten()
                        .find(|entry| entry.source == Some(source))
                        .map(|entry| entry.description.id)
                        .ok_or(Refusal::InvalidId)
                })
                .transpose()
        };
        self.timing(token(audio)?, token(midi)?, frames)
    }
    pub(crate) fn consumed(&mut self, source: NodePairSourceId) {
        if let Some(entry) = self
            .slots
            .iter_mut()
            .flatten()
            .find(|entry| entry.source == Some(source))
        {
            entry.source = None;
            entry.abandoned = true;
        }
    }
    pub(crate) fn idle(&self) -> bool {
        self.slots.iter().flatten().all(|entry| {
            entry.pending == 0
                && entry.correlation.is_none()
                && !entry.abandoned
                && entry.failure.is_none()
                && matches!(entry.phase, Phase::Receiving | Phase::Ready)
        })
    }
    pub(crate) fn new(slots: usize, chunk_bytes: usize) -> Self {
        Self {
            slots: vec![None; slots].into_boxed_slice(),
            buffers: (0..RUNTIME_UPLOAD_BUFFERS)
                .map(|_| vec![0; chunk_bytes].into_boxed_slice())
                .collect(),
            initials: (0..8)
                .map(|_| Box::new(rt_runtime::midi_state::MidiState::default()))
                .collect(),
            last_id: 0,
        }
    }
    fn index(&self, id: u64) -> Result<usize, Refusal> {
        self.slots
            .iter()
            .position(|entry| entry.is_some_and(|entry| entry.description.id == id))
            .ok_or(Refusal::InvalidId)
    }
    pub(crate) fn return_buffer(&mut self, bytes: Box<[u8]>) {
        assert!(self.buffers.len() < self.buffers.capacity());
        self.buffers.push(bytes);
    }
    pub(crate) fn return_initial(&mut self, initial: Box<rt_runtime::midi_state::MidiState>) {
        assert!(self.initials.len() < self.initials.capacity());
        self.initials.push(initial);
    }
    pub(crate) fn disconnect(&mut self) {
        for entry in self.slots.iter_mut().flatten() {
            entry.abandoned = true;
            entry.correlation = None;
        }
    }
}

impl ControlEndpoint {
    pub(crate) fn content_request(
        &mut self,
        correlation: u64,
        request: Request,
    ) -> Result<Option<Outcome>, Refusal> {
        match request {
            Request::ContentWindow {
                id,
                frames,
                windows,
            } => {
                if frames > u32::MAX as u64 || frames % u64::from(self.limits.quantum) != 0 {
                    return Err(Refusal::InvalidValue);
                }
                let slot = self.uploads.index(id)?;
                let entry = self.uploads.slots[slot]
                    .as_ref()
                    .ok_or(Refusal::InvalidId)?;
                if let Some(failure) = entry.failure {
                    return Err(failure);
                }
                if entry.pending != 0 || entry.phase != Phase::Ready || entry.abandoned {
                    return Err(Refusal::Busy);
                }
                if let Some(previous) = entry.windows {
                    return if previous == (frames, windows) {
                        Ok(Some(Outcome::Done))
                    } else {
                        Err(Refusal::InvalidValue)
                    };
                }
                let source = entry.source.ok_or(Refusal::InvalidId)?;
                self.submit_loop(
                    self.at(),
                    Operation::NodePairSourceWindows {
                        id: source,
                        frames,
                        windows: crate::timing::loop_windows(windows),
                    },
                    Purpose::Content {
                        slot,
                        step: Step::Window,
                        correlation: Some(correlation),
                    },
                )?;
                let entry = self.uploads.slots[slot]
                    .as_mut()
                    .expect("validated content");
                entry.windows = Some((frames, windows));
                entry.pending += 1;
                Ok(None)
            }
            Request::ContentBegin(description) => {
                let shape = description.midi
                    || description
                        .frames
                        .checked_mul(self.settings.channels as u64)
                        .and_then(|n| n.checked_mul(4))
                        == Some(description.bytes);
                if description.id == 0 || description.id <= self.uploads.last_id {
                    return Err(Refusal::InvalidId);
                }
                if !shape
                    || description.bytes > MAX_CONTENT_BYTES
                    || description.frames > u32::MAX as u64
                    || (description.midi && description.frames == 0 && description.bytes != 0)
                {
                    return Err(Refusal::InvalidValue);
                }
                let retained: u64 = self
                    .uploads
                    .slots
                    .iter()
                    .flatten()
                    .map(|entry| entry.description.bytes)
                    .sum();
                if description.bytes > MAX_CONTENT_BYTES - retained {
                    return Err(Refusal::Capacity);
                }
                let slot = self
                    .uploads
                    .slots
                    .iter()
                    .position(Option::is_none)
                    .ok_or(Refusal::Capacity)?;
                self.uploads.last_id = description.id;
                self.uploads.slots[slot] = Some(Content {
                    windows: None,
                    description,
                    resource: None,
                    upload: None,
                    source: None,
                    position: 0,
                    pending: 0,
                    correlation: Some(correlation),
                    phase: Phase::Create,
                    failure: None,
                    abandoned: false,
                    blocked: false,
                });
                Ok(None)
            }
            Request::ContentInitial { id, message } => {
                let slot = self.uploads.index(id)?;
                let entry = self.uploads.slots[slot].as_ref().expect("located upload");
                if let Some(failure) = entry.failure {
                    return Err(failure);
                }
                if entry.phase != Phase::Receiving || entry.position != 0 || !entry.description.midi
                {
                    return Err(Refusal::InvalidValue);
                }
                if !self.room(1) {
                    return Err(Refusal::Overloaded);
                }
                let bytes = message.as_bytes();
                if bytes.len() > 3
                    || !matches!(bytes.first(), Some(0x80..=0xef | 0xff))
                    || rt_runtime::midi::Message::parse(bytes, 3).is_err()
                {
                    return Err(Refusal::InvalidValue);
                }
                let mut packet = [0; 3];
                packet[..bytes.len()].copy_from_slice(bytes);
                self.submit(
                    Command::resource(resource::Request::MidiInitial {
                        upload: entry.upload.expect("receiving resource"),
                        packet,
                        len: bytes.len() as u8,
                    }),
                    Purpose::Content {
                        slot,
                        step: Step::Write,
                        correlation: Some(correlation),
                    },
                    false,
                )?;
                self.uploads.slots[slot]
                    .as_mut()
                    .expect("located upload")
                    .pending += 1;
                Ok(None)
            }
            Request::ContentWrite { id, offset, data } => {
                let slot = self.uploads.index(id)?;
                let entry = self.uploads.slots[slot].as_ref().expect("located upload");
                if let Some(failure) = entry.failure {
                    return Err(failure);
                }
                if entry.phase != Phase::Receiving {
                    return Err(if entry.pending > 0 {
                        Refusal::Overloaded
                    } else {
                        Refusal::Busy
                    });
                }
                if entry.description.midi && entry.pending != 0 {
                    return Err(Refusal::Overloaded);
                }
                let count = data.as_bytes().len();
                let end = offset
                    .checked_add(count as u64)
                    .ok_or(Refusal::InvalidValue)?;
                if offset != entry.position
                    || end > entry.description.bytes
                    || count > usize::from(self.limits.content_chunk_bytes)
                {
                    return Err(Refusal::InvalidValue);
                }
                let upload = entry.upload.expect("receiving resource");
                if !self.room(1) {
                    return Err(Refusal::Overloaded);
                }
                let mut bytes = self.uploads.buffers.pop().ok_or(Refusal::Overloaded)?;
                bytes[..count].copy_from_slice(data.as_bytes());
                self.submit(
                    Command::resource(resource::Request::WritePrefix {
                        upload,
                        offset: offset as usize,
                        bytes,
                        used: count,
                    }),
                    Purpose::Content {
                        slot,
                        step: Step::Write,
                        correlation: Some(correlation),
                    },
                    false,
                )?;
                let entry = self.uploads.slots[slot].as_mut().expect("located upload");
                entry.position = end;
                entry.pending += 1;
                Ok(None)
            }
            Request::ContentFinish { id } | Request::ContentRelease { id } => {
                let slot = self.uploads.index(id)?;
                let entry = self.uploads.slots[slot].as_mut().expect("located upload");
                if entry.pending > 0 {
                    return Err(Refusal::Overloaded);
                }
                if matches!(request, Request::ContentRelease { .. }) {
                    entry.phase = Phase::Cleanup;
                    entry.abandoned = true;
                    entry.correlation = Some(correlation);
                } else {
                    if entry.phase == Phase::Ready {
                        return Ok(Some(Outcome::Done));
                    }
                    if let Some(failure) = entry.failure {
                        return Err(failure);
                    }
                    if entry.phase != Phase::Receiving || entry.position != entry.description.bytes
                    {
                        return Err(Refusal::InvalidValue);
                    }
                    entry.phase = Phase::Finish;
                    entry.correlation = Some(correlation);
                }
                Ok(None)
            }
            _ => Err(Refusal::Internal),
        }
    }

    pub(crate) fn advance_content(&mut self) {
        for slot in 0..self.uploads.slots.len() {
            let Some(mut entry) = self.uploads.slots[slot] else {
                continue;
            };
            if self
                .preview
                .as_ref()
                .is_some_and(|preview| preview.content == entry.description.id)
            {
                continue;
            }
            if entry.pending != 0 || !self.room(1) {
                continue;
            }
            if entry.failure.is_some() || entry.abandoned {
                entry.phase = Phase::Cleanup;
            }
            let (command, step, expects) = match entry.phase {
                Phase::Create => (
                    Command::resource(resource::Request::Create),
                    Step::Create,
                    false,
                ),
                Phase::Begin if entry.description.midi => {
                    let Some(initial) = self.uploads.initials.pop() else {
                        continue;
                    };
                    (
                        Command::resource(resource::Request::BeginIndexedMidiUpload {
                            resource: entry.resource.expect("created resource"),
                            expected_revision: Revision(0),
                            description: resource::Description {
                                format: resource::Format::Midi,
                                frames: entry.description.frames,
                                bytes: entry.description.bytes as usize,
                            },
                            initial,
                        }),
                        Step::Begin,
                        false,
                    )
                }
                Phase::Begin => (
                    Command::resource(resource::Request::BeginUpload {
                        resource: entry.resource.expect("created resource"),
                        expected_revision: Revision(0),
                        description: resource::Description {
                            format: if entry.description.midi {
                                resource::Format::Midi
                            } else {
                                resource::Format::Audio {
                                    channels: self.settings.channels as u32,
                                }
                            },
                            frames: entry.description.frames,
                            bytes: entry.description.bytes as usize,
                        },
                    }),
                    Step::Begin,
                    false,
                ),
                Phase::Finish => (
                    Command::resource(resource::Request::Finish {
                        upload: entry.upload.expect("receiving upload"),
                    }),
                    Step::Finish,
                    false,
                ),
                Phase::Commit => (
                    Command::resource(resource::Request::Commit {
                        upload: entry.upload.expect("finished upload"),
                    }),
                    Step::Commit,
                    false,
                ),
                Phase::Snapshot => (
                    self.loops.command(
                        self.at(),
                        Operation::NodePairSnapshot {
                            resource: entry.resource.expect("committed resource"),
                        },
                    ),
                    Step::Snapshot,
                    true,
                ),
                Phase::Validate => (
                    self.loops.command(
                        self.at(),
                        Operation::NodePairSourceValidate {
                            id: entry.source.expect("owned source"),
                        },
                    ),
                    Step::Validate,
                    true,
                ),
                Phase::Cleanup => {
                    if let Some(id) = entry.source {
                        (
                            self.loops
                                .command(self.at(), Operation::NodePairSourceRelease { id }),
                            Step::Release,
                            true,
                        )
                    } else if let Some(upload) = entry.upload {
                        (
                            Command::resource(resource::Request::CancelUpload { upload }),
                            Step::Cancel,
                            false,
                        )
                    } else if let Some(resource) = entry.resource {
                        (
                            Command::resource(resource::Request::Remove { resource }),
                            Step::Remove,
                            false,
                        )
                    } else {
                        self.uploads.slots[slot] = None;
                        if let Some(correlation) = entry.correlation {
                            self.answer(
                                correlation,
                                entry.failure.map_or(Outcome::Done, Outcome::Refused),
                            );
                        }
                        continue;
                    }
                }
                Phase::Receiving | Phase::Ready => {
                    if let Some(correlation) = entry.correlation.take() {
                        self.answer(correlation, Outcome::Done);
                    }
                    self.uploads.slots[slot] = Some(entry);
                    continue;
                }
            };
            match self.submit(
                command,
                Purpose::Content {
                    slot,
                    step,
                    correlation: None,
                },
                expects,
            ) {
                Ok(_) => entry.pending += 1,
                Err(failure) => entry.failure = Some(failure),
            }
            self.uploads.slots[slot] = Some(entry);
        }
    }

    pub(crate) fn content_result(
        &mut self,
        slot: usize,
        step: Step,
        correlation: Option<u64>,
        finished: bool,
        result: RuntimeResult,
    ) {
        let mut entry = self.uploads.slots[slot].expect("pending content owns its slot");
        let failure = match &result {
            RuntimeResult::Completion(outcome, ..) => completion_refusal(*outcome),
            RuntimeResult::Execution(execution) => match execution_outcome(execution) {
                Outcome::Done => None,
                Outcome::TimingApplied(_)
                | Outcome::ContentApplied { .. }
                | Outcome::ContentPrepared { .. }
                | Outcome::Input(_)
                | Outcome::InputCapacity(_) => Some(Refusal::Internal),
                Outcome::Refused(reason) => Some(reason),
            },
        };
        if let Some(failure) = failure {
            entry.failure.get_or_insert(failure);
        }
        match result {
            RuntimeResult::Completion(_, command, retired) => {
                if let Command::Resource(command) = &command {
                    if failure.is_none() {
                        match (step, command.reply()) {
                            (Step::Create, Some(resource::Reply::Created(id))) => {
                                entry.resource = Some(id);
                                entry.phase = Phase::Begin;
                            }
                            (Step::Begin, Some(resource::Reply::Upload(id))) => {
                                entry.upload = Some(id);
                                entry.phase = Phase::Receiving;
                            }
                            (Step::Finish, _) => entry.phase = Phase::Commit,
                            (Step::Commit, _) => {
                                entry.upload = None;
                                entry.phase = Phase::Snapshot;
                            }
                            _ => {}
                        }
                    }
                }
                self.reclaim(command);
                self.retire(retired);
            }
            RuntimeResult::Execution(execution) => {
                if failure.is_none() {
                    match execution.operation_result {
                        Some(Reply::NodePairSource { id, status, .. }) => {
                            entry.source = Some(id);
                            entry.phase = if matches!(status, NodePairSourceStatus::Ready { .. }) {
                                Phase::Ready
                            } else {
                                Phase::Validate
                            };
                        }
                        Some(Reply::NodePairSourceStatus { status, .. }) => {
                            entry.phase = if matches!(status, NodePairSourceStatus::Ready { .. }) {
                                Phase::Ready
                            } else {
                                Phase::Validate
                            };
                            if let NodePairSourceStatus::Failed(error) = status {
                                entry.failure =
                                    Some(crate::endpoint::failure_refusal(error.into()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if finished {
            entry.pending -= 1;
            match step {
                Step::Release => entry.source = None,
                Step::Cancel => entry.upload = None,
                Step::Remove => entry.resource = None,
                _ => {}
            }
            if let Some(correlation) = correlation {
                self.answer(correlation, failure.map_or(Outcome::Done, Outcome::Refused));
            }
        }
        self.uploads.slots[slot] = Some(entry);
    }
}
