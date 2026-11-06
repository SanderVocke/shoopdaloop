use super::*;
use crate::{DriverReply, DriverWork};
use alloc::format;
use persistence_jobs::{capture::Cut, mapping::Candidate};
use shoop_app_protocol::{audio_driver::Command as DriverCommand, persistence_image as image};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Discovery,
    Preflight,
    Confirmation,
    Capture,
    Conversion,
    Recheck,
    Starting,
    Restoration,
    Publication,
    Saving,
    Fatal,
}

pub(super) struct Job {
    pub(super) operation: Option<OperationId>,
    phase: Phase,
    ticket: u64,
    owner: Option<Authority>,
    elapsed: Duration,
    source: ResolvedAudioDriverConfig,
    target: Option<ResolvedAudioDriverConfig>,
    original: Option<Arc<image::Image>>,
    candidate: Option<Candidate>,
    data: Option<BackendSessionData>,
    sync: BTreeMap<u64, u64>,
    cut: Option<Cut>,
    failure: Option<String>,
    rollback: bool,
    reconfirm: Option<ResolvedAudioDriverConfig>,
}

impl Service {
    pub fn set_audio_driver_controller(
        &mut self,
        controller: Box<dyn crate::AudioDriverController>,
    ) -> Result<()> {
        if self.driver_controller.is_some() {
            return Err(anyhow!("an audio-driver controller is already installed"));
        }
        self.driver_state = self.observed.audio_drivers.clone();
        self.driver_state.supported = true;
        self.driver_controller = Some(controller);
        if let Some(active) = self.driver_state.active.as_ref() {
            self.audio_driver_command(DriverCommand::Discover(active.configured.clone()))
                .map_err(|error| anyhow!(error.message))?;
        }
        self.bump();
        Ok(())
    }

    pub(super) fn audio_driver_busy(&self, command: Option<&Command>) -> bool {
        self.driver_job.as_ref().is_some_and(|job| {
            if matches!(
                job.phase,
                Phase::Discovery | Phase::Preflight | Phase::Confirmation
            ) {
                matches!(
                    command,
                    Some(Command::AudioDriver(
                        DriverCommand::Discover(_) | DriverCommand::Request(_)
                    ))
                )
            } else if job.phase == Phase::Saving {
                matches!(command, Some(Command::AudioDriver(_)))
            } else {
                true
            }
        })
    }

    pub(super) fn driver_freezes_session(&self) -> bool {
        self.driver_job.as_ref().is_some_and(|job| {
            matches!(
                job.phase,
                Phase::Capture
                    | Phase::Conversion
                    | Phase::Recheck
                    | Phase::Starting
                    | Phase::Restoration
                    | Phase::Publication
                    | Phase::Fatal
            )
        })
    }

    fn driver_conflicts(&self, recording: bool) -> Result<(), CommandError> {
        if self.persistence_job.is_some()
            || self.composite_structure.is_some()
            || self
                .pending
                .values()
                .any(|p| !matches!(p.effect, Effect::AudioDriver) && p.first <= p.last)
            || recording
                && self.capture_recording_busy(persistence::Target::Session {
                    revision: self.revision,
                })
        {
            return Err(CommandError {
                reason: Refusal::Busy,
                message: "recording or another session operation prevents an audio-driver switch"
                    .into(),
            });
        }
        Ok(())
    }

    fn driver_work(&mut self, work: DriverWork) -> Result<u64> {
        self.driver_ticket = self
            .driver_ticket
            .checked_add(1)
            .ok_or_else(|| anyhow!("driver work identity exhausted"))?;
        self.driver_controller
            .as_mut()
            .ok_or_else(|| anyhow!("audio-driver switching is unavailable"))?
            .submit(self.driver_ticket, work)
            .map_err(anyhow::Error::msg)?;
        Ok(self.driver_ticket)
    }

    pub(super) fn audio_driver_command(
        &mut self,
        command: DriverCommand,
    ) -> Result<Effect, CommandError> {
        if self.driver_controller.is_none() {
            return Err(CommandError::unsupported(
                BackendCapability::AudioDriverSwitching,
            ));
        }
        let discovery = matches!(&command, DriverCommand::Discover(_));
        match command {
            DriverCommand::Discover(config) | DriverCommand::Request(config) => {
                self.driver_conflicts(false)?;
                let source = self.driver_state.active.clone().ok_or_else(|| {
                    CommandError::invalid("the active audio driver is unavailable")
                })?;
                let work = if discovery {
                    DriverWork::Discover(config)
                } else {
                    DriverWork::Preflight(config)
                };
                let ticket = self.driver_work(work)?;
                if !discovery {
                    self.driver_request = self.driver_request.checked_add(1).ok_or_else(|| {
                        CommandError::invalid("driver request identity exhausted")
                    })?;
                    self.driver_state.switch = AudioDriverSwitchState {
                        request_id: self.driver_request,
                        source: Some(source.clone()),
                        status: AudioDriverSwitchStatus::Switching,
                        message: "Checking the requested audio configuration".into(),
                        ..Default::default()
                    };
                }
                self.driver_job = Some(Job {
                    operation: None,
                    phase: if discovery {
                        Phase::Discovery
                    } else {
                        Phase::Preflight
                    },
                    ticket,
                    owner: self.lease.as_ref().map(|l| l.authority),
                    elapsed: Duration::ZERO,
                    source,
                    target: None,
                    original: None,
                    candidate: None,
                    data: None,
                    sync: BTreeMap::new(),
                    cut: None,
                    failure: None,
                    rollback: false,
                    reconfirm: None,
                });
                Ok(Effect::AudioDriver)
            }
            DriverCommand::Confirm { request_id, accept } => {
                let mut job = self.driver_job.take().ok_or_else(|| {
                    CommandError::invalid("no audio-driver confirmation is pending")
                })?;
                if job.phase != Phase::Confirmation
                    || self.driver_state.switch.request_id != request_id
                    || job.owner != self.lease.as_ref().map(|l| l.authority)
                {
                    self.driver_job = Some(job);
                    return Err(CommandError::invalid("stale audio-driver confirmation"));
                }
                if !accept {
                    self.driver_state.switch.status = AudioDriverSwitchStatus::Completed;
                    self.driver_state.switch.message =
                        "Audio-driver switch cancelled; runtime and preferences are unchanged"
                            .into();
                    return Ok(Effect::None);
                }
                if let Err(error) = self.driver_conflicts(true) {
                    self.driver_state.switch.status = AudioDriverSwitchStatus::Failed;
                    self.driver_state.switch.message = error.message.clone();
                    return Err(error);
                }
                self.backend.cancel_media_reads();
                self.media_loop = None;
                self.media_pending = None;
                job.operation = None;
                job.phase = Phase::Capture;
                job.elapsed = Duration::ZERO;
                self.driver_state.switch.status = AudioDriverSwitchStatus::Switching;
                self.driver_state.switch.message = "Capturing the current session".into();
                self.driver_job = Some(job);
                Ok(Effect::AudioDriver)
            }
            DriverCommand::RetryPersistence { request_id } => {
                if self.driver_job.is_some()
                    || self.driver_state.switch.request_id != request_id
                    || !self.driver_state.switch.persistence_retry_available
                {
                    return Err(CommandError::invalid(
                        "stale audio-driver persistence retry",
                    ));
                }
                let source = self
                    .driver_state
                    .active
                    .clone()
                    .ok_or_else(|| CommandError::invalid("the active driver is unavailable"))?;
                let ticket = self.driver_work(DriverWork::Persist(source.configured.clone()))?;
                self.driver_state.switch.status = AudioDriverSwitchStatus::Persisting;
                self.driver_state.switch.persistence_retry_available = false;
                self.driver_job = Some(Job {
                    operation: None,
                    phase: Phase::Saving,
                    ticket,
                    owner: None,
                    elapsed: Duration::ZERO,
                    source,
                    target: None,
                    original: None,
                    candidate: None,
                    data: None,
                    sync: BTreeMap::new(),
                    cut: None,
                    failure: None,
                    rollback: false,
                    reconfirm: None,
                });
                Ok(Effect::AudioDriver)
            }
        }
    }

    fn driver_finish(&mut self, job: &mut Job, error: Option<String>) {
        if let Some(id) = job.operation.take() {
            if let Some(pending) = self.pending.get_mut(&id) {
                pending.effect = Effect::None;
                pending.first = 1;
                pending.last = 0;
                pending.completed.clear();
                pending.error = error.map(|message| CommandError {
                    reason: Refusal::Engine,
                    message,
                });
            }
            self.finish(id);
        }
        self.bump();
    }

    fn driver_confirmation(&mut self, job: &mut Job, target: ResolvedAudioDriverConfig) {
        self.driver_state.switch.status = AudioDriverSwitchStatus::AwaitingConfirmation;
        self.driver_state.switch.target = Some(target.clone());
        self.driver_state.switch.message = format!("Switch {} ({}) to {} ({})? Audio processing will be interrupted and loops will stop. Sample rate: {} Hz to {} Hz; buffer: {} to {} frames. Audio, MIDI and loop timing will be converted together when required.", job.source.configured.kind().label(), job.source.instance_name, target.configured.kind().label(), target.instance_name, job.source.sample_rate, target.sample_rate, job.source.buffer_size, target.buffer_size);
        job.target = Some(target);
        job.phase = Phase::Confirmation;
        job.elapsed = Duration::ZERO;
        job.owner = self.lease.as_ref().map(|l| l.authority);
        self.driver_finish(job, None);
    }

    pub(super) fn detach_audio_driver(&mut self) {
        if self
            .driver_job
            .as_ref()
            .is_some_and(|job| job.phase == Phase::Confirmation)
        {
            self.driver_job = None;
            self.driver_state.switch.status = AudioDriverSwitchStatus::Completed;
            self.driver_state.switch.message =
                "Unconfirmed audio-driver switch cancelled on detach".into();
        }
    }

    fn driver_format(&self, rate: u32) -> ConversionTarget {
        ConversionTarget {
            sample_rate: rate,
            quantum_frames: self.observed.status.quantum_frames,
        }
    }

    fn driver_prepare(&mut self, job: &mut Job, image: &image::Image, rate: u32) -> Result<()> {
        let (candidate, data, sync) =
            Candidate::prepare_switch(self, image, self.driver_format(rate))?;
        job.candidate = Some(candidate);
        job.data = Some(data);
        job.sync = sync;
        Ok(())
    }

    fn driver_start(&mut self, job: &mut Job, profile: ResolvedAudioDriverConfig) -> Result<()> {
        job.phase = Phase::Starting;
        self.backend.close_connection();
        self.engine_closed = true;
        job.ticket = self.driver_work(DriverWork::Start(profile))?;
        Ok(())
    }

    fn driver_failed(&mut self, job: &mut Job, error: String) -> bool {
        if job.rollback {
            let message = format!(
                "{}; restoring the prior driver failed: {error}",
                job.failure
                    .as_deref()
                    .unwrap_or("audio-driver switch failed")
            );
            self.driver_state.active = None;
            self.driver_state.switch.status = AudioDriverSwitchStatus::Fatal;
            self.driver_state.switch.message = message.clone();
            self.observed.status.driver_state = BackendDriverState::Failed;
            self.init_error = Some(message.clone());
            self.driver_finish(job, Some(message));
            job.phase = Phase::Fatal;
            return true;
        }
        if matches!(
            job.phase,
            Phase::Starting | Phase::Restoration | Phase::Publication
        ) {
            job.rollback = true;
            job.failure = Some(error);
            self.driver_state.switch.status = AudioDriverSwitchStatus::Restoring;
            self.driver_state.switch.message = format!(
                "{}; restoring the prior audio driver and session",
                job.failure
                    .as_deref()
                    .unwrap_or("audio-driver switch failed")
            );
            let result = job
                .original
                .clone()
                .ok_or_else(|| anyhow!("rollback capture is unavailable"))
                .and_then(|image| self.driver_prepare(job, &image, job.source.sample_rate))
                .and_then(|()| self.driver_start(job, job.source.clone()));
            if let Err(error) = result {
                return self.driver_failed(job, error.to_string());
            }
            return true;
        }
        self.backend.cancel_media_reads();
        self.driver_state.switch.status = AudioDriverSwitchStatus::Failed;
        self.driver_state.switch.message = error.clone();
        self.driver_finish(job, Some(error));
        false
    }

    fn driver_publish_result(&mut self, job: &mut Job) -> Result<bool> {
        if let Some(error) = self.audio_driver_publication_error() {
            return Err(anyhow!(error));
        }
        if !self.session_publications_ready()
            || self
                .backend
                .mutations_pending_through(self.backend.mutation_cursor())
        {
            return Ok(true);
        }
        if job.rollback {
            if let Some(target) = job.reconfirm.take() {
                job.rollback = false;
                job.failure = None;
                job.original = None;
                job.data = None;
                self.driver_confirmation(job, target);
                return Ok(true);
            }
            let message = format!(
                "{}; the prior driver and session were restored",
                job.failure
                    .as_deref()
                    .unwrap_or("audio-driver switch failed")
            );
            self.driver_state.switch.status = AudioDriverSwitchStatus::Failed;
            self.driver_state.switch.message = message.clone();
            self.driver_finish(job, Some(message));
            return Ok(false);
        }
        self.driver_state.switch.status = AudioDriverSwitchStatus::Persisting;
        self.driver_state.switch.message =
            "Audio driver switched; saving its startup preference".into();
        job.original = None;
        job.data = None;
        job.phase = Phase::Saving;
        self.driver_finish(job, None);
        match self.driver_work(DriverWork::Persist(
            self.driver_state
                .active
                .as_ref()
                .unwrap()
                .configured
                .clone(),
        )) {
            Ok(ticket) => job.ticket = ticket,
            Err(error) => {
                self.driver_state.switch.status = AudioDriverSwitchStatus::Failed;
                self.driver_state.switch.persistence_retry_available = true;
                self.driver_state.switch.message = format!(
                    "The new driver is active, but its startup preference was not saved: {error}"
                );
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn tick_audio_driver(&mut self, elapsed: Duration) -> bool {
        let Some(mut job) = self.driver_job.take() else {
            return false;
        };
        job.elapsed = job.elapsed.saturating_add(elapsed);
        let mut frozen = matches!(
            job.phase,
            Phase::Capture
                | Phase::Conversion
                | Phase::Recheck
                | Phase::Starting
                | Phase::Restoration
                | Phase::Fatal
        );
        if job.phase == Phase::Fatal {
            self.driver_job = Some(job);
            return true;
        }
        let result = self.advance_audio_driver(&mut job, elapsed);
        match result {
            Ok(true) => self.driver_job = Some(job),
            Ok(false) => {}
            Err(error) => {
                let retain = self.driver_failed(&mut job, error.to_string());
                frozen = retain && job.phase != Phase::Confirmation;
                if retain {
                    self.driver_job = Some(job);
                }
            }
        }
        frozen
    }

    fn advance_audio_driver(&mut self, job: &mut Job, elapsed: Duration) -> Result<bool> {
        match job.phase {
            Phase::Confirmation => {
                if job.owner != self.lease.as_ref().map(|l| l.authority)
                    || job.elapsed >= Duration::from_secs(60)
                {
                    self.driver_state.switch.status = AudioDriverSwitchStatus::Completed;
                    self.driver_state.switch.message =
                        "Unconfirmed audio-driver switch expired; audio is unchanged".into();
                    return Ok(false);
                }
                return Ok(true);
            }
            Phase::Capture => {
                self.backend.advance(elapsed);
                let observed = self.backend.poll()?;
                self.apply_observation(observed);
                self.driver_conflicts(true)
                    .map_err(|e| anyhow!(e.message))?;
                if job.cut.is_none() {
                    job.cut = Some(Cut::new(self));
                }
                match self.backend.capture_session_async()? {
                    BackendAsyncResult::Pending(_) => {}
                    BackendAsyncResult::Ready(data) => {
                        if !job.cut.as_ref().unwrap().matches_composites(&data) {
                            return Err(anyhow!("composite version changed during driver capture"));
                        }
                        let image = job.cut.take().unwrap().finish(
                            data,
                            persistence::Target::Session {
                                revision: self.revision,
                            },
                        )?;
                        job.original = Some(image.clone());
                        let target = self.driver_format(job.target.as_ref().unwrap().sample_rate);
                        job.ticket = self.driver_work(DriverWork::Convert {
                            image,
                            target,
                            limits: self.resource_limits,
                        })?;
                        job.phase = Phase::Conversion;
                        self.driver_state.switch.status = AudioDriverSwitchStatus::Resampling;
                        self.driver_state.switch.message =
                            "Preparing audio, MIDI and loop timing for the target driver".into();
                    }
                }
                return Ok(true);
            }
            Phase::Restoration => {
                self.backend.advance(elapsed);
                self.backend.poll()?;
                let data = job.data.as_ref().unwrap();
                match self
                    .backend
                    .replace_session_with_sync_async(data, &job.sync)?
                {
                    BackendAsyncResult::Pending(_) => {}
                    BackendAsyncResult::Ready(mapping) => {
                        let observed = self.backend.poll()?;
                        job.candidate
                            .take()
                            .unwrap()
                            .publish(self, &mapping, &observed)?;
                        self.apply_observation(observed);
                        self.backend
                            .set_loop_smoothing_ms(self.options.loop_smoothing_ms)?;
                        self.init_through = self.backend.mutation_cursor();
                        self.driver_state.active = self.observed.audio_drivers.active.clone();
                        job.phase = Phase::Publication;
                        self.poll_host_graph();
                        self.advance_publications();
                    }
                }
                return Ok(true);
            }
            Phase::Publication => return self.driver_publish_result(job),
            _ => {}
        }
        let Some(completion) = self.driver_controller.as_mut().unwrap().poll() else {
            return Ok(true);
        };
        if completion.ticket != job.ticket {
            return Ok(true);
        }
        let reply = match completion.result {
            Ok(reply) => reply,
            Err(error) if job.phase == Phase::Saving => {
                self.driver_state.switch.status = AudioDriverSwitchStatus::Failed;
                self.driver_state.switch.message = format!(
                    "The new driver is active, but its startup preference was not saved: {error}"
                );
                self.driver_state.switch.persistence_retry_available = true;
                self.driver_finish(job, Some(error));
                return Ok(false);
            }
            Err(error) => return Err(anyhow!(error)),
        };
        match (job.phase, reply) {
            (Phase::Discovery, DriverReply::Discovered(catalog)) => {
                self.driver_state.catalog = catalog;
                self.driver_finish(job, None);
                return Ok(false);
            }
            (Phase::Preflight, DriverReply::Resolved(target)) => {
                if target.configured == job.source.configured
                    && target.sample_rate == job.source.sample_rate
                    && target.buffer_size == job.source.buffer_size
                {
                    return Err(anyhow!(
                        "the requested audio-driver configuration is already active"
                    ));
                }
                self.driver_confirmation(job, target);
            }
            (Phase::Conversion, DriverReply::Converted(image)) => {
                self.driver_prepare(job, &image, job.target.as_ref().unwrap().sample_rate)?;
                job.ticket = self.driver_work(DriverWork::Preflight(
                    job.target.as_ref().unwrap().configured.clone(),
                ))?;
                job.phase = Phase::Recheck;
            }
            (Phase::Recheck, DriverReply::Resolved(target)) => {
                if target.sample_rate != job.target.as_ref().unwrap().sample_rate
                    || target.buffer_size != job.target.as_ref().unwrap().buffer_size
                {
                    job.original = None;
                    job.data = None;
                    job.candidate = None;
                    self.driver_confirmation(job, target);
                } else {
                    self.driver_conflicts(true)
                        .map_err(|e| anyhow!(e.message))?;
                    self.stop_preview();
                    self.driver_start(job, target)?;
                    self.driver_state.switch.status = AudioDriverSwitchStatus::Switching;
                    self.driver_state.switch.message = "Starting the target audio driver".into();
                }
            }
            (
                Phase::Starting,
                DriverReply::Started {
                    backend,
                    graph,
                    profile,
                },
            ) => {
                self.backend = backend;
                self.host_graph_controller = graph;
                self.host_graph = Arc::default();
                self.publications.clear();
                self.graph_connections.clear();
                self.master_graph_auto_done = true;
                self.engine_closed = false;
                self.init_error = None;
                self.init_through = 0;
                self.bootstrap_track = None;
                self.bootstrap_loop = None;
                let expected = if job.rollback {
                    &job.source
                } else {
                    job.target.as_ref().unwrap()
                };
                job.phase = Phase::Restoration;
                if profile.sample_rate != expected.sample_rate {
                    if !job.rollback {
                        job.reconfirm = Some(profile);
                    }
                    return Err(anyhow!(
                        "resolved target sample rate changed after opening the device"
                    ));
                }
                self.driver_state.active = Some(profile);
                self.driver_state.switch.status = AudioDriverSwitchStatus::Restoring;
                self.driver_state.switch.message = "Restoring the captured session".into();
            }
            (Phase::Saving, DriverReply::Persisted) => {
                self.driver_state.switch.status = AudioDriverSwitchStatus::Completed;
                self.driver_state.switch.persistence_retry_available = false;
                self.driver_state.switch.message =
                    "Audio driver switched and saved for the next host start".into();
                self.driver_finish(job, None);
                return Ok(false);
            }
            _ => return Err(anyhow!("unexpected audio-driver worker result")),
        }
        self.bump();
        Ok(true)
    }
}
