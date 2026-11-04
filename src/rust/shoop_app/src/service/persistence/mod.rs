use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::*;
use shoop_app_protocol::{
    persistence_image as image,
    persistence_transfer::{Assembly, Source},
};
mod capture;
mod mapping;
use capture::Cut;
use mapping::Candidate;

enum Captured {
    Session(BackendSessionData),
    Loop(BackendLoopContent),
}

pub(super) enum Input {
    Session {
        backend: BackendSessionData,
        sync: BTreeMap<u64, u64>,
        candidate: Candidate,
    },
    Loop {
        backend: BackendLoopId,
        update: BackendLoopContentUpdate,
    },
}
pub(super) struct Ready {
    authority: Authority,
    key: persistence::Key,
    target: persistence::Target,
    image: Arc<image::Image>,
    input: Input,
    started: bool,
}
pub(super) enum Preparing {
    Validation(image::Validation),
    Assembly(Assembly),
}
impl Preparing {
    fn step(self) -> Result<core::result::Result<image::ValidatedImage, Self>, Refusal> {
        match self {
            Self::Validation(mut validation) => {
                if !validation.step(256 * 1024)? {
                    return Ok(Err(Self::Validation(validation)));
                }
                validation.finish().map(Ok)
            }
            Self::Assembly(mut assembly) => match assembly.step(256 * 1024)? {
                Some(validated) => Ok(Ok(validated)),
                None => Ok(Err(Self::Assembly(assembly))),
            },
        }
    }
}
pub(super) enum Job {
    Prepare {
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
        work: Preparing,
    },
    ValidateCapture {
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
        local: bool,
        validation: image::Validation,
    },
    Upload {
        authority: Authority,
        receiver: persistence::Receiver,
    },
    Ready(Ready),
    Capture {
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
        local: bool,
        cut: Option<Cut>,
    },
    Captured {
        authority: Authority,
        key: persistence::Key,
        source: Source,
    },
    Commit(Ready),
}
impl Job {
    fn authority(&self) -> Authority {
        match self {
            Self::Upload { authority, .. }
            | Self::Prepare { authority, .. }
            | Self::ValidateCapture { authority, .. }
            | Self::Capture { authority, .. }
            | Self::Captured { authority, .. } => *authority,
            Self::Ready(ready) | Self::Commit(ready) => ready.authority,
        }
    }
    fn key(&self) -> persistence::Key {
        match self {
            Self::Upload { receiver, .. } => receiver.description().key,
            Self::Capture { key, .. }
            | Self::Captured { key, .. }
            | Self::Prepare { key, .. }
            | Self::ValidateCapture { key, .. } => *key,
            Self::Ready(ready) | Self::Commit(ready) => ready.key,
        }
    }
}
impl Service {
    fn target_current(
        &self,
        key: persistence::Key,
        target: persistence::Target,
    ) -> Result<(), Refusal> {
        if !key.valid() || key.epoch != self.persistence_epoch {
            return Err(Refusal::Identity);
        }
        match target {
            persistence::Target::Session { revision } if revision == self.revision => Ok(()),
            persistence::Target::Loop {
                loop_id,
                content_revision,
            } if self
                .loops
                .get(&loop_id)
                .is_some_and(|entry| entry.view.content_revision == content_revision) =>
            {
                Ok(())
            }
            _ => Err(Refusal::RevisionConflict),
        }
    }
    fn persistence_begin(
        &mut self,
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
    ) -> Result<(), Refusal> {
        if !self.is_ready() || !self.lease.as_ref().is_some_and(|lease| lease.ready) {
            return Err(Refusal::NotReady);
        }
        self.target_current(key, target)?;
        if self.persistence_job.is_some() {
            return Err(Refusal::Busy);
        }
        if self
            .persistence_last
            .as_ref()
            .is_some_and(|(owner, transfer)| *owner == authority && key.transfer.0 <= *transfer)
        {
            return Err(Refusal::Identity);
        }
        self.persistence_last = Some((authority, key.transfer.0));
        Ok(())
    }
    fn persistence_reply(&mut self, authority: Authority, reply: persistence::Reply) {
        self.emit(ServiceMessage::Persistence { authority, reply });
    }
    pub(super) fn stage_image(
        &mut self,
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
        image: Arc<image::Image>,
    ) -> Result<(), Refusal> {
        self.persistence_begin(authority, key, target)?;
        let work = Preparing::Validation(image::Validation::new(image)?);
        self.persistence_job = Some(Job::Prepare {
            authority,
            key,
            target,
            work,
        });
        Ok(())
    }
    fn prepare_image(
        &mut self,
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
        validated: image::ValidatedImage,
    ) -> Result<Ready> {
        let image = validated.into_image();
        shoop_app_protocol::codec::encode_manifest(&image.manifest)
            .map_err(|_| anyhow!("persistence metadata exceeds its 8 MiB budget"))?;
        if self.persistence_epoch == u64::MAX {
            return Err(anyhow!("session epoch exhausted"));
        }
        let input = match (&image.manifest, target) {
            (image::Manifest::Session(session), persistence::Target::Session { .. }) => {
                let (candidate, backend, sync) = Candidate::prepare(self, &image, session)?;
                Input::Session {
                    candidate,
                    backend,
                    sync,
                }
            }
            (
                image::Manifest::Update { sample_rate, .. },
                persistence::Target::Loop { loop_id, .. },
            ) => {
                if *sample_rate != self.observed.status.sample_rate {
                    return Err(anyhow!("host sample rate changed"));
                }
                Input::Loop {
                    backend: self
                        .loops
                        .get(&loop_id)
                        .ok_or_else(|| anyhow!("stale target loop"))?
                        .backend,
                    update: mapping::update(&image)?,
                }
            }
            _ => return Err(anyhow!("persistence image does not match its target")),
        };
        Ok(Ready {
            authority,
            key,
            target,
            image,
            input,
            started: false,
        })
    }
    pub(super) fn capture_image(
        &mut self,
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
        local: bool,
    ) -> Result<(), Refusal> {
        self.persistence_begin(authority, key, target)?;
        if self.capture_recording_busy(target) {
            return Err(Refusal::Busy);
        }
        self.media_pending = None;
        self.media_loop = None;
        self.backend.cancel_media_reads();
        self.persistence_job = Some(Job::Capture {
            authority,
            key,
            target,
            local,
            cut: None,
        });
        Ok(())
    }
    pub(super) fn persistence_request(
        &mut self,
        authority: Authority,
        request: persistence::Request,
    ) -> Result<(), Refusal> {
        if !request.valid() {
            return Err(Refusal::Invalid);
        }
        let key = request.key();
        match request {
            persistence::Request::Begin(description) => {
                self.persistence_begin(authority, key, description.target)?;
                self.persistence_job = Some(Job::Upload {
                    authority,
                    receiver: persistence::Receiver::new(authority, description)?,
                });
                self.persistence_reply(authority, persistence::Reply::Begun { key });
                return Ok(());
            }
            persistence::Request::Capture { target, .. } => {
                return self.capture_image(authority, key, target, false)
            }
            persistence::Request::Cancel { .. }
                if self.persistence_closed == Some((authority, key))
                    && self
                        .persistence_job
                        .as_ref()
                        .is_none_or(|job| job.key() != key) =>
            {
                self.persistence_reply(authority, persistence::Reply::Closed { key });
                return Ok(());
            }
            _ => {}
        }
        let mut job = self.persistence_job.take().ok_or(Refusal::NotReady)?;
        if job.authority() != authority || job.key() != key {
            self.persistence_job = Some(job);
            return Err(Refusal::Identity);
        }
        let result = match request {
            persistence::Request::Cancel { .. } => {
                if matches!(job, Job::Commit(_)) {
                    self.persistence_job = Some(job);
                    return Err(Refusal::Busy);
                }
                self.backend.cancel_media_reads();
                self.persistence_closed = Some((authority, key));
                self.persistence_reply(authority, persistence::Reply::Closed { key });
                return Ok(());
            }
            request @ (persistence::Request::Object { .. }
            | persistence::Request::Chunk { .. }
            | persistence::Request::FinishObject { .. }
            | persistence::Request::Finish { .. }) => {
                if let Job::Upload { receiver, .. } = &mut job {
                    receiver.apply(authority, request)
                } else {
                    Err(Refusal::NotReady)
                }
            }
            persistence::Request::Describe { object, .. }
            | persistence::Request::Read { object, .. } => {
                if let Job::Captured { source, .. } = &mut job {
                    match request {
                        persistence::Request::Read { offset, .. } => source
                            .chunk(object, offset)
                            .map(|bytes| persistence::Reply::Chunk {
                                key,
                                object,
                                offset,
                                bytes,
                            }),
                        _ => source
                            .object(object)
                            .map(|object| persistence::Reply::Object { key, object }),
                    }
                } else {
                    Err(Refusal::NotReady)
                }
            }
            _ => Err(Refusal::Invalid),
        };
        if matches!(&job, Job::Upload { receiver, .. } if receiver.ready()) {
            let Job::Upload { receiver, .. } = job else {
                unreachable!()
            };
            let target = receiver.description().target;
            let work = Preparing::Assembly(Assembly::new(receiver.into_objects()?)?);
            self.persistence_job = Some(Job::Prepare {
                authority,
                key,
                target,
                work,
            });
            return Ok(());
        } else {
            self.persistence_job = Some(job);
        }
        match result {
            Ok(reply) => {
                self.persistence_reply(authority, reply);
                Ok(())
            }
            Err(reason) => Err(reason),
        }
    }
    pub(super) fn commit_persistence(
        &mut self,
        key: persistence::Key,
    ) -> Result<Effect, CommandError> {
        let job = self
            .persistence_job
            .take()
            .ok_or_else(|| CommandError::invalid("no prepared persistence image"))?;
        let Job::Ready(ready) = job else {
            self.persistence_job = Some(job);
            return Err(CommandError::invalid("persistence image is not ready"));
        };
        if ready.key != key
            || self
                .lease
                .as_ref()
                .is_none_or(|lease| lease.authority != ready.authority)
        {
            self.persistence_job = Some(Job::Ready(ready));
            return Err(CommandError::invalid("stale persistence owner or key"));
        }
        self.target_current(key, ready.target)
            .map_err(|reason| CommandError {
                reason,
                message: "session or target changed while preparing the file".to_owned(),
            })?;
        if self.pending.len() != 1 {
            self.persistence_job = Some(Job::Ready(ready));
            return Err(CommandError::invalid(
                "wait for admitted changes before committing persistence",
            ));
        }
        self.persistence_job = Some(Job::Commit(ready));
        Ok(Effect::Persistence(key))
    }
    pub(super) fn persistence_busy(&self, command: Option<&Command>) -> bool {
        match self.persistence_job {
            Some(Job::Commit(_)) => true,
            Some(Job::Capture { .. }) => !matches!(
                command,
                Some(
                    Command::SelectLoops { .. }
                        | Command::SetTarget(_)
                        | Command::ResetXruns
                        | Command::SetMasterAutoConnect(_)
                        | Command::Track {
                            action: TrackAction::NameChanged(_)
                                | TrackAction::OutputGainChanged(_)
                                | TrackAction::OutputBalanceChanged(_)
                                | TrackAction::OutputMuteChanged(_)
                                | TrackAction::InputGainChanged(_)
                                | TrackAction::InputBalanceChanged(_),
                            ..
                        }
                        | Command::Loop {
                            action: LoopAction::NameChanged(_)
                                | LoopAction::GainChanged(_)
                                | LoopAction::BalanceChanged(_)
                                | LoopAction::IconClicked(_)
                                | LoopAction::IconDoubleClicked,
                            ..
                        }
                        | Command::Bus {
                            action: BusAction::GainChanged(_)
                                | BusAction::BalanceChanged(_)
                                | BusAction::MuteChanged(_),
                            ..
                        }
                )
            ),
            _ => false,
        }
    }
    pub(super) fn persistence_reads_busy(&self) -> bool {
        matches!(
            self.persistence_job,
            Some(Job::Capture { .. } | Job::Commit(_))
        )
    }
    fn capture_recording_busy(&self, target: persistence::Target) -> bool {
        self.loops.iter().any(|(id, entry)| {
            let selected = matches!(target, persistence::Target::Session { .. })
                || matches!(target, persistence::Target::Loop { loop_id, .. } if loop_id == *id);
            selected
                && (matches!(
                    entry.view.mode,
                    LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
                ) || matches!(
                    entry.view.next_mode,
                    LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
                ))
        })
    }
    pub(super) fn persistence_detach(&mut self) {
        if !matches!(self.persistence_job, Some(Job::Commit(_))) {
            self.persistence_job = None;
            self.backend.cancel_media_reads();
        }
    }
    pub(super) fn advance_persistence(&mut self) {
        let Some(job) = self.persistence_job.take() else {
            return;
        };
        match job {
            Job::Prepare {
                authority,
                key,
                target,
                work,
            } => match work.step() {
                Ok(Err(work)) => {
                    self.persistence_job = Some(Job::Prepare {
                        authority,
                        key,
                        target,
                        work,
                    })
                }
                Ok(Ok(validated)) => {
                    let result = self.target_current(key, target).and_then(|()| {
                        self.prepare_image(authority, key, target, validated)
                            .map_err(|_| Refusal::Invalid)
                    });
                    match result {
                        Ok(ready) => {
                            self.persistence_job = Some(Job::Ready(ready));
                            self.persistence_reply(authority, persistence::Reply::Ready { key });
                        }
                        Err(reason) => self.persistence_reply(
                            authority,
                            persistence::Reply::Refused { key, reason },
                        ),
                    }
                }
                Err(reason) => {
                    self.persistence_reply(authority, persistence::Reply::Refused { key, reason })
                }
            },
            Job::ValidateCapture {
                authority,
                key,
                target,
                local,
                mut validation,
            } => match validation.step(256 * 1024) {
                Ok(false) => {
                    self.persistence_job = Some(Job::ValidateCapture {
                        authority,
                        key,
                        target,
                        local,
                        validation,
                    })
                }
                Ok(true) => match validation.finish().and_then(Source::from_validated) {
                    Ok(source) => {
                        let image = Arc::clone(source.image());
                        let reply = (!local)
                            .then(|| persistence::Reply::Captured(source.description(key, target)));
                        self.persistence_job = Some(Job::Captured {
                            authority,
                            key,
                            source,
                        });
                        if let Some(reply) = reply {
                            self.persistence_reply(authority, reply);
                        } else {
                            self.emit(ServiceMessage::CapturedImage {
                                authority,
                                key,
                                image,
                            });
                        }
                    }
                    Err(reason) => self
                        .persistence_reply(authority, persistence::Reply::Refused { key, reason }),
                },
                Err(reason) => {
                    self.persistence_reply(authority, persistence::Reply::Refused { key, reason })
                }
            },

            Job::Capture {
                authority,
                key,
                mut target,
                local,
                mut cut,
            } => {
                if cut.is_none() && self.capture_recording_busy(target) {
                    self.persistence_reply(
                        authority,
                        persistence::Reply::Refused {
                            key,
                            reason: Refusal::Busy,
                        },
                    );
                    return;
                }
                if cut.is_none() && (!self.pending.is_empty() || !self.is_ready()) {
                    self.persistence_job = Some(Job::Capture {
                        authority,
                        key,
                        target,
                        local,
                        cut,
                    });
                    return;
                }
                if cut.is_none() {
                    let busy = self.loops.iter().any(|(id, entry)| {
                        let selected = matches!(target, persistence::Target::Session { .. }) || matches!(target, persistence::Target::Loop { loop_id, .. } if loop_id == *id);
                        selected && (matches!(entry.view.mode, LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet) || matches!(entry.view.next_mode, LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet))
                    });
                    if busy {
                        self.persistence_reply(
                            authority,
                            persistence::Reply::Refused {
                                key,
                                reason: Refusal::Busy,
                            },
                        );
                        return;
                    }
                    if let persistence::Target::Session { revision } = &mut target {
                        *revision = self.revision;
                    }
                    if self.target_current(key, target).is_err() {
                        self.persistence_reply(
                            authority,
                            persistence::Reply::Refused {
                                key,
                                reason: Refusal::RevisionConflict,
                            },
                        );
                        return;
                    }
                    cut = Some(Cut::new(self));
                }
                let captured = match target {
                    persistence::Target::Session { .. } => self
                        .backend
                        .capture_session_async()
                        .map(|result| match result {
                            BackendAsyncResult::Pending(progress) => {
                                BackendAsyncResult::Pending(progress)
                            }
                            BackendAsyncResult::Ready(data) => {
                                BackendAsyncResult::Ready(Captured::Session(data))
                            }
                        }),
                    persistence::Target::Loop { loop_id, .. } => self
                        .backend
                        .capture_loop_content_async(self.loops[&loop_id].backend)
                        .map(|result| match result {
                            BackendAsyncResult::Pending(progress) => {
                                BackendAsyncResult::Pending(progress)
                            }
                            BackendAsyncResult::Ready(data) => {
                                BackendAsyncResult::Ready(Captured::Loop(data))
                            }
                        }),
                };
                match captured {
                    Ok(BackendAsyncResult::Pending(_)) => {
                        self.persistence_job = Some(Job::Capture {
                            authority,
                            key,
                            target,
                            local,
                            cut,
                        })
                    }
                    Ok(BackendAsyncResult::Ready(capture)) => {
                        let cut = cut.unwrap();
                        let image = match (capture, target) {
                            (Captured::Session(capture), _) => cut.finish(capture, target),
                            (
                                Captured::Loop(content),
                                persistence::Target::Loop { loop_id, .. },
                            ) => cut.finish_loop(content, loop_id),
                            _ => unreachable!(),
                        };
                        match image {
                            Ok(image) => match image::Validation::for_transfer(image) {
                                Ok(validation) => {
                                    self.persistence_job = Some(Job::ValidateCapture {
                                        authority,
                                        key,
                                        target,
                                        local,
                                        validation,
                                    })
                                }
                                Err(reason) => self.persistence_reply(
                                    authority,
                                    persistence::Reply::Refused { key, reason },
                                ),
                            },
                            Err(_) => self.persistence_reply(
                                authority,
                                persistence::Reply::Refused {
                                    key,
                                    reason: Refusal::Unsupported,
                                },
                            ),
                        }
                    }
                    Err(_) => self.persistence_reply(
                        authority,
                        persistence::Reply::Refused {
                            key,
                            reason: Refusal::Busy,
                        },
                    ),
                }
            }
            Job::Commit(mut ready) => {
                if !ready.started
                    && self
                        .backend
                        .mutations_pending_through(self.backend.mutation_cursor())
                {
                    self.persistence_job = Some(Job::Commit(ready));
                    return;
                }
                ready.started = true;
                let result = match &mut ready.input {
                    Input::Session { backend, sync, .. } => self
                        .backend
                        .replace_session_with_sync_async(backend, sync)
                        .map(|result| match result {
                            BackendAsyncResult::Pending(_) => None,
                            BackendAsyncResult::Ready(mapping) => Some(mapping),
                        }),
                    Input::Loop { backend, update } => self
                        .backend
                        .replace_loop_content_async(*backend, update)
                        .map(|result| match result {
                            BackendAsyncResult::Pending(_) => None,
                            BackendAsyncResult::Ready(()) => {
                                Some(BackendSessionReplacement::default())
                            }
                        }),
                };
                if matches!(result, Ok(None)) {
                    self.persistence_job = Some(Job::Commit(ready));
                    return;
                }
                let key = ready.key;
                let outcome = result.and_then(|mapping| {
                    let observed = self.backend.poll()?;
                    match ready.input {
                        Input::Session { candidate, .. } => {
                            candidate.publish(self, &mapping.unwrap())?
                        }
                        Input::Loop { .. } => {
                            if let persistence::Target::Loop { loop_id, .. } = ready.target {
                                let revision = observed
                                    .loops
                                    .get(&self.loops[&loop_id].backend)
                                    .map_or(0, |entry| entry.content_revision);
                                for ((id, _, _), (version, _)) in &mut self.channel_annotations {
                                    if *id == loop_id {
                                        *version = revision;
                                    }
                                }
                                if let image::Manifest::Update { audio, midi, .. } =
                                    &ready.image.manifest
                                {
                                    for channel in audio {
                                        self.channel_annotations.insert(
                                            (loop_id, false, channel.channel),
                                            (
                                                revision,
                                                channel.content.recording_started_at.clone(),
                                            ),
                                        );
                                    }
                                    for channel in midi {
                                        self.channel_annotations.insert(
                                            (loop_id, true, channel.channel),
                                            (
                                                revision,
                                                channel
                                                    .content
                                                    .channel
                                                    .recording_started_at
                                                    .clone(),
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    self.apply_observation(observed);
                    Ok(())
                });
                self.persistence_closed = Some((ready.authority, key));
                let operation = self.pending.iter().find_map(|(id, pending)| {
                    matches!(pending.effect, Effect::Persistence(candidate) if candidate == key)
                        .then_some(*id)
                });
                if let Some(id) = operation {
                    if let Err(error) = outcome {
                        self.pending.get_mut(&id).unwrap().error = Some(CommandError::from(error));
                    }
                    self.finish(id);
                }
            }
            job => self.persistence_job = Some(job),
        }
    }
}

impl Service {
    pub(super) fn new_session_image(&mut self) -> Result<Effect, CommandError> {
        if self.persistence_job.is_some() || self.pending.len() != 1 {
            return Err(CommandError::invalid(
                "finish or cancel existing operations before creating a new session",
            ));
        }
        let make_port = |id: u64,
                         name: String,
                         owner: ApplicationPortOwner,
                         data_type,
                         direction,
                         role,
                         external_connections| image::Port {
            state: ApplicationPortState {
                publication: None,
                id: PortId::from_raw(id),
                name,
                owner,
                data_type,
                direction,
                role,
                connection_policy: ConnectionPolicy::UserManaged,
            },
            external_connections,
        };
        let mut channels = Vec::new();
        let mut sinks = self
            .observed
            .connections
            .host_ports
            .values()
            .filter(|p| {
                p.data_type == BackendPortDataType::Audio
                    && p.direction == BackendPortDirection::Input
            })
            .collect::<Vec<_>>();
        sinks.sort_by_key(|p| {
            (
                !p.preferred_playback,
                super::commands::host_port_order(&p.id),
            )
        });
        for (index, label) in MASTER_BUS_CHANNEL_LABELS.into_iter().enumerate() {
            let external = if self.options.master_auto_connect {
                sinks
                    .get(index)
                    .or_else(|| sinks.last())
                    .map(|p| vec![p.id.clone()])
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            channels.push(image::BusChannel {
                id: BusChannelId::from_raw(index as u64 + 1),
                label: label.to_owned(),
                port: make_port(
                    index as u64 + 3,
                    format!("master_out_{}", index + 1),
                    ApplicationPortOwner::Bus {
                        bus_id: BusId::from_raw(1),
                    },
                    PortDataType::Audio,
                    PortDirection::Output,
                    PortRole::AudioOutput,
                    external,
                ),
            });
        }
        let image = Arc::new(image::Image {
            manifest: image::Manifest::Session(image::Session {
                sample_rate: self.observed.status.sample_rate,
                global: GlobalControlState::default(),
                selected: Vec::new(),
                target: None,
                routes: Vec::new(),
                tracks: vec![image::Track {
                    id: TrackId::from_raw(1),
                    name: "Sync".to_owned(),
                    port_name_base: "sync_loop".to_owned(),
                    sync: true,
                    audio_channels: 1,
                    midi: false,
                    controls: image::Controls {
                        input_gain_db: 0.,
                        input_balance: 0.,
                        monitoring: false,
                        output_gain_db: 0.,
                        output_balance: 0.,
                        muted: false,
                    },
                    ports: vec![
                        make_port(
                            1,
                            "sync_loop_direct_in".to_owned(),
                            ApplicationPortOwner::Track {
                                track_id: TrackId::from_raw(1),
                                kind: TrackPortOwnerKind::Sync,
                            },
                            PortDataType::Audio,
                            PortDirection::Input,
                            PortRole::AudioInput,
                            Vec::new(),
                        ),
                        make_port(
                            2,
                            "sync_loop_direct_out".to_owned(),
                            ApplicationPortOwner::Track {
                                track_id: TrackId::from_raw(1),
                                kind: TrackPortOwnerKind::Sync,
                            },
                            PortDataType::Audio,
                            PortDirection::Output,
                            PortRole::AudioOutput,
                            Vec::new(),
                        ),
                    ],
                    loops: vec![image::Loop {
                        id: LoopId::from_raw(1),
                        name: "Sync".to_owned(),
                        sync: true,
                        frames: 0,
                        gain: 1.,
                        balance: 0.,
                        audio: vec![image::Channel {
                            object: None,
                            frames: 0,
                            gain: 1.,
                            start_offset: 0,
                            capture_alignment: 0,
                            preplay: 0,
                            recording_started_at: None,
                        }],
                        midi: Vec::new(),
                    }],
                }],
                bus_display_order: vec![BusId::from_raw(1)],
                buses: vec![image::Bus {
                    id: BusId::from_raw(1),
                    name: "Master".to_owned(),
                    gain_db: 0.,
                    balance: 0.,
                    muted: false,
                    channels,
                }],
                global_ports: vec![make_port(
                    5,
                    "Global FX Control MIDI In".to_owned(),
                    ApplicationPortOwner::GlobalFxControl,
                    PortDataType::Midi,
                    PortDirection::Input,
                    PortRole::MidiInput,
                    Vec::new(),
                )],
            }),
            payloads: Vec::new(),
        });
        let authority = self
            .lease
            .as_ref()
            .ok_or_else(|| CommandError::invalid("no attached client"))?
            .authority;
        let key = persistence::Key {
            transfer: TransferId(u64::MAX),
            epoch: self.persistence_epoch,
        };
        let target = persistence::Target::Session {
            revision: self.revision,
        };
        let mut validation = image::Validation::new(image)
            .map_err(|_| CommandError::invalid("invalid new session image"))?;
        if !validation
            .step(256 * 1024)
            .map_err(|_| CommandError::invalid("invalid new session image"))?
        {
            return Err(CommandError::invalid(
                "new session image unexpectedly contains media",
            ));
        }
        let ready = self.prepare_image(
            authority,
            key,
            target,
            validation
                .finish()
                .map_err(|_| CommandError::invalid("incomplete new session image"))?,
        )?;
        self.persistence_job = Some(Job::Commit(ready));
        Ok(Effect::Persistence(key))
    }
}
