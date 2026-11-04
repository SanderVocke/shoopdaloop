//! Client-owned files, codecs and dialogs over the authoritative persistence protocol.
use anyhow::{anyhow, bail, ensure, Result};
use shoop_app_api::*;
use shoop_app_client::{runtime::*, Client, Error, State};
use shoop_app_protocol::{persistence as transfer, persistence_image as image, *};
use shoop_persistence_codec::codec::{Decoded, Value, Work};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static NEXT_FILE_TASK: AtomicU64 = AtomicU64::new(1 << 32);

mod jobs;
mod preview;
use jobs::{CodecExecutor, CodecJob};
struct Read {
    receiver: transfer::Receiver,
    object: Option<transfer::ObjectDescription>,
    received: u64,
    requested: u64,
}
enum Phase {
    Idle,
    Input,
    Working,
    Capture,
    CaptureSent,
    Reading(Read),
    Decoded(Decoded),
    Captured(Arc<image::Image>),
    Staging,
    Ready,
    Committed(OperationId),
    Output,
}
struct Task {
    view: IoTaskState,
    key: transfer::Key,
    target: transfer::Target,
    rate: u32,
    audio_channels: usize,
    update_length: bool,
    wav: bool,
    standard: bool,
    output_warning: String,
}
pub(super) struct Provider {
    preview: preview::Preview,
    task: Option<Task>,
    phase: Phase,
    worker: Option<CodecJob>,
    codec: CodecExecutor,
    messages: VecDeque<ClientMessage>,
    requests: VecDeque<ApplicationFileRequest>,
    output: Option<ApplicationFileOutput>,
    pending_cancel: Option<transfer::Key>,
}
impl Provider {
    pub fn new() -> Self {
        Self {
            preview: preview::Preview::new(),
            task: None,
            phase: Phase::Idle,
            worker: None,
            codec: CodecExecutor::new(),
            messages: VecDeque::new(),
            requests: VecDeque::new(),
            output: None,
            pending_cancel: None,
        }
    }
    fn start(
        &mut self,
        state: &SessionSnapshot,
        kind: IoTaskKind,
        loop_id: Option<LoopId>,
    ) -> Result<()> {
        ensure!(
            matches!(self.phase, Phase::Idle) && self.worker.is_none() && self.output.is_none(),
            "another file workflow is still active"
        );
        self.codec.reset();
        let (target, audio_channels) = if let Some(loop_id) = loop_id {
            let (track, entry) = state
                .tracks
                .iter()
                .find_map(|t| t.loops.iter().find(|l| l.id == loop_id).map(|l| (t, l)))
                .ok_or_else(|| anyhow!("the target loop no longer exists"))?;
            ensure!(
                track.topology == TrackTopology::Direct,
                "media files require a direct track (P03)"
            );
            let ports = state.connections.application_ports.iter().filter(|p| matches!(p.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track.id)).collect::<Vec<_>>();
            let audio = ports
                .iter()
                .filter(|p| p.role == PortRole::AudioInput)
                .count();
            let midi = ports.iter().any(|p| p.role == PortRole::MidiInput);
            ensure!(
                !matches!(
                    kind,
                    IoTaskKind::ImportLoopAudio | IoTaskKind::ExportLoopAudio
                ) || audio != 0,
                "the target loop has no audio channels"
            );
            ensure!(
                !matches!(
                    kind,
                    IoTaskKind::ImportLoopMidi | IoTaskKind::ExportLoopMidi
                ) || midi,
                "the target loop has no MIDI channel"
            );
            (
                transfer::Target::Loop {
                    loop_id,
                    content_revision: entry.content_revision,
                },
                audio,
            )
        } else {
            (
                transfer::Target::Session {
                    revision: state.revision,
                },
                0,
            )
        };
        let mut id = NEXT_FILE_TASK.load(Ordering::Relaxed);
        loop {
            let next = id
                .checked_add(1)
                .ok_or_else(|| anyhow!("file task identity exhausted"))?;
            match NEXT_FILE_TASK.compare_exchange_weak(
                id,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => id = current,
            }
        }
        self.task = Some(Task {
            view: IoTaskState {
                id: TaskId::from_raw(id),
                kind,
                status: IoTaskStatus::Running,
                progress: 0.,
                message: "Preparing file workflow".into(),
                sample_rate_warning: None,
                audio_channel_mapping: None,
                audio_channel_selection: None,
            },
            key: transfer::Key {
                transfer: TransferId(id),
                epoch: state.persistence_epoch,
            },
            target,
            rate: state.status.sample_rate,
            audio_channels,
            update_length: true,
            wav: false,
            standard: false,
            output_warning: String::new(),
        });
        Ok(())
    }
    fn work(&mut self, work: Work) -> Result<()> {
        let kind = match &work {
            Work::Assemble(_) => "assemble",
            Work::GenerateClick { .. } => "generate_click",
            Work::Decode { .. } => "decode",
            Work::Resample { .. } => "resample",
            Work::Prepare { .. } => "prepare",
            Work::Encode { .. } => "encode",
        };
        tracing::debug!(kind, "frontend.persistence.codec_start");
        ensure!(
            self.worker.is_none(),
            "file codec worker is still reclaiming an earlier task"
        );
        self.worker = Some(self.codec.start(work)?);
        self.phase = Phase::Working;
        Ok(())
    }
    fn finish(&mut self, status: IoTaskStatus, message: impl Into<String>) {
        self.codec.reset();
        #[cfg(target_arch = "wasm32")]
        {
            self.worker = None;
        }
        if let Some(task) = &mut self.task {
            tracing::debug!(
                task = task.view.id.raw(),
                ?status,
                "frontend.persistence.finished"
            );
            task.view.status = status;
            task.view.message = message.into();
            task.view.progress = 1.;
            task.view.sample_rate_warning = None;
            task.view.audio_channel_mapping = None;
            task.view.audio_channel_selection = None;
            if !matches!(
                self.phase,
                Phase::Idle
                    | Phase::Input
                    | Phase::Working
                    | Phase::Decoded(_)
                    | Phase::Captured(_)
                    | Phase::Output
            ) {
                self.pending_cancel = Some(task.key);
            }
        }
        self.phase = Phase::Idle;
        self.requests.clear();
        self.output = None;
    }
    fn current(&self, state: &SessionSnapshot) -> Result<()> {
        let task = self.task.as_ref().ok_or_else(|| anyhow!("no file task"))?;
        ensure!(
            state.persistence_epoch == task.key.epoch && state.status.sample_rate == task.rate,
            "session or sample rate changed while the file task was pending"
        );
        if let transfer::Target::Loop {
            loop_id,
            content_revision,
        } = task.target
        {
            ensure!(
                state
                    .tracks
                    .iter()
                    .flat_map(|t| &t.loops)
                    .any(|l| l.id == loop_id && l.content_revision == content_revision),
                "loop content changed while the file task was pending"
            );
        }
        Ok(())
    }
    fn decoded(&mut self, decoded: Decoded) -> Result<()> {
        let task = self.task.as_mut().unwrap();
        if decoded.rate() != task.rate {
            task.view.status = IoTaskStatus::AwaitingSampleRateConfirmation;
            task.view.message =
                "Confirm conversion to the current audio device's sample rate".into();
            task.view.sample_rate_warning = Some(SampleRateWarning {
                source_rate: decoded.rate(),
                target_rate: task.rate,
                affected_media: "Session audio, MIDI and frame timing".into(),
            });
            self.phase = Phase::Decoded(decoded);
        } else if task.view.kind == IoTaskKind::GenerateClickTrack {
            let mapping = vec![0; task.audio_channels];
            self.work(Work::Prepare {
                decoded,
                mapping,
                update_length: true,
            })?;
        } else if let Some(labels) = decoded.audio_labels() {
            ensure!(!labels.is_empty(), "the audio file has no channels");
            task.view.status = IoTaskStatus::AwaitingChannelMapping;
            task.view.sample_rate_warning = None;
            task.view.audio_channel_mapping = Some(AudioChannelMappingState {
                source_channels: labels.clone(),
                destination_channels: (0..task.audio_channels)
                    .map(|i| format!("Audio {}", i + 1))
                    .collect(),
                default_mapping: (0..task.audio_channels)
                    .map(|i| (i % labels.len()) as u32)
                    .collect(),
            });
            task.view.message = "Map source audio to the loop channels".into();
            self.phase = Phase::Decoded(decoded);
        } else {
            let update_length = task.update_length;
            task.view.sample_rate_warning = None;
            task.view.status = IoTaskStatus::Running;
            self.work(Work::Prepare {
                decoded,
                mapping: Vec::new(),
                update_length,
            })?;
        }
        Ok(())
    }
    fn captured(&mut self, image: Arc<image::Image>) -> Result<()> {
        self.pending_cancel = Some(self.task.as_ref().unwrap().key);
        let task = self.task.as_mut().unwrap();
        if task.view.kind == IoTaskKind::ExportLoopAudio {
            let image::Manifest::Loop { content, .. } = &image.manifest else {
                bail!("expected loop capture")
            };
            ensure!(!content.audio.is_empty(), "loop has no audio channels");
            task.view.status = IoTaskStatus::AwaitingChannelSelection;
            task.view.audio_channel_selection = Some(AudioChannelSelectionState {
                available_channels: (0..content.audio.len())
                    .map(|i| format!("Audio {}", i + 1))
                    .collect(),
                default_selection: (0..content.audio.len() as u32).collect(),
            });
            task.view.message = "Choose audio channels to export".into();
            self.phase = Phase::Captured(image);
            Ok(())
        } else {
            self.encode(image, Vec::new())
        }
    }
    fn encode(&mut self, image: Arc<image::Image>, channels: Vec<u32>) -> Result<()> {
        let t = self.task.as_mut().unwrap();
        t.view.status = IoTaskStatus::Running;
        t.view.audio_channel_selection = None;
        t.view.message = "Encoding captured media".into();
        let work = Work::Encode {
            image,
            kind: t.view.kind,
            wav: t.wav,
            standard: t.standard,
            channels,
        };
        self.work(work)
    }
    fn apply_inner(
        &mut self,
        intent: AppIntent,
        state: &SessionSnapshot,
        scripts: bool,
    ) -> Result<()> {
        let importing_audio = matches!(intent, AppIntent::ImportLoopAudioBytes { .. });
        match intent {
            AppIntent::RequestSaveSession | AppIntent::RequestLoadSessionPicker | AppIntent::RequestLoadSessionUrl | AppIntent::RequestLoadSessionSource { .. } if scripts => bail!("Session-owned script persistence is still unavailable (P16); remove or convert session scripts before saving or loading"),
            AppIntent::RequestSaveSession => { self.start(state, IoTaskKind::SaveSession, None)?; self.phase = Phase::Capture; }
            AppIntent::RequestLoadSessionSource { source } => {
                self.start(state, IoTaskKind::LoadSession, None)?;
                self.requests.push_back(ApplicationFileRequest { task_id: self.task.as_ref().unwrap().view.id, kind: ApplicationFileRequestKind::SessionSource, source: Some(source) });
                self.phase = Phase::Input;
            }
            AppIntent::RequestLoadSessionPicker | AppIntent::RequestLoadSessionUrl => {
                let kind = if matches!(intent, AppIntent::RequestLoadSessionUrl) { ApplicationFileRequestKind::SessionUrl } else { ApplicationFileRequestKind::Session };
                self.start(state, IoTaskKind::LoadSession, None)?;
                self.requests.push_back(ApplicationFileRequest { task_id: self.task.as_ref().unwrap().view.id, kind, source: None }); self.phase = Phase::Input;
            }
            AppIntent::RequestLoopAudioImportPicker { loop_id } | AppIntent::RequestLoopMidiImportPicker { loop_id } => {
                let audio = matches!(intent, AppIntent::RequestLoopAudioImportPicker { .. });
                self.start(state, if audio { IoTaskKind::ImportLoopAudio } else { IoTaskKind::ImportLoopMidi }, Some(loop_id))?;
                self.requests.push_back(ApplicationFileRequest { task_id: self.task.as_ref().unwrap().view.id, kind: if audio { ApplicationFileRequestKind::Audio } else { ApplicationFileRequestKind::Midi }, source: None }); self.phase = Phase::Input;
            }
            AppIntent::RequestLoopAudioExport { loop_id, format } => { self.start(state, IoTaskKind::ExportLoopAudio, Some(loop_id))?; self.task.as_mut().unwrap().wav = format == LoopAudioExportFormat::FloatWav; self.phase = Phase::Capture; }
            AppIntent::RequestLoopMidiExport { loop_id, standard } => { self.start(state, IoTaskKind::ExportLoopMidi, Some(loop_id))?; self.task.as_mut().unwrap().standard = standard; self.phase = Phase::Capture; }
            AppIntent::LoadSessionBytes { bytes, .. } => {
                ensure!(!scripts, "session-owned scripts must be removed or converted before loading (P16)");
                self.start(state, IoTaskKind::LoadSession, None)?; self.work(Work::Decode { kind: IoTaskKind::LoadSession, bytes, rate: state.status.sample_rate })?;
            }
            AppIntent::ImportLoopAudioBytes { loop_id, bytes, update_loop_length, .. } | AppIntent::ImportLoopMidiBytes { loop_id, bytes, update_loop_length, .. } => {
                let kind = if importing_audio { IoTaskKind::ImportLoopAudio } else { IoTaskKind::ImportLoopMidi };
                self.start(state, kind, Some(loop_id))?; self.task.as_mut().unwrap().update_length = update_loop_length;
                self.work(Work::Decode { kind, bytes, rate: state.status.sample_rate })?;
            }
            AppIntent::GenerateClickTrack { loop_id, request } => {
                ensure!(!state.tracks.iter().flat_map(|t| &t.loops).any(|l| matches!(l.mode, LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet)), "click generation is unavailable while recording or replacing");
                let entry = state.tracks.iter().flat_map(|t| &t.loops).find(|l| l.id == loop_id).ok_or_else(|| anyhow!("the target loop no longer exists"))?;
                ensure!(entry.composite_kind == CompositeKind::None, "click tracks require a primitive loop");
                let kind = match request.kind { ClickTrackKind::Audio => IoTaskKind::ImportLoopAudio, ClickTrackKind::Midi => IoTaskKind::ImportLoopMidi };
                self.start(state, kind, Some(loop_id))?;
                self.task.as_mut().unwrap().view.kind = IoTaskKind::GenerateClickTrack;
                self.work(Work::GenerateClick { request, rate: state.status.sample_rate })?;
            }
            AppIntent::CompleteIoInput { task_id, bytes, .. } if self.matches(task_id) && matches!(self.phase, Phase::Input) => {
                self.current(state)?; let t = self.task.as_ref().unwrap(); self.work(Work::Decode { kind: t.view.kind, bytes, rate: t.rate })?;
            }
            AppIntent::ConfirmSampleRateConversion { task_id, accept } if self.matches(task_id) && self.task.as_ref().unwrap().view.status == IoTaskStatus::AwaitingSampleRateConfirmation => {
                if !accept { self.finish(IoTaskStatus::Cancelled, "Rate conversion declined; session unchanged"); }
                else { self.current(state)?; let Phase::Decoded(decoded) = std::mem::replace(&mut self.phase, Phase::Idle) else { bail!("no decoded media") }; self.task.as_mut().unwrap().view.status = IoTaskStatus::Running; self.work(Work::Resample { decoded, rate: state.status.sample_rate })?; }
            }
            AppIntent::ConfirmAudioChannelMapping { task_id, source_for_destination } if self.matches(task_id) && self.task.as_ref().unwrap().view.status == IoTaskStatus::AwaitingChannelMapping => {
                self.current(state)?;
                ensure!(source_for_destination.len() == self.task.as_ref().unwrap().audio_channels, "map exactly one source for each destination");
                let Phase::Decoded(decoded) = std::mem::replace(&mut self.phase, Phase::Idle) else { bail!("no decoded audio") };
                self.task.as_mut().unwrap().view.status = IoTaskStatus::Running; self.task.as_mut().unwrap().view.audio_channel_mapping = None;
                self.work(Work::Prepare { decoded, mapping: source_for_destination, update_length: self.task.as_ref().unwrap().update_length })?;
            }
            AppIntent::ConfirmAudioChannelSelection { task_id, channels } if self.matches(task_id) && matches!(self.phase, Phase::Captured(_)) => {
                let Phase::Captured(image) = std::mem::replace(&mut self.phase, Phase::Idle) else { unreachable!() }; self.encode(image, channels)?;
            }
            AppIntent::CancelIoTask { task_id } if self.matches(task_id) => {
                if matches!(self.phase, Phase::Committed(_) | Phase::Output) { self.task.as_mut().unwrap().view.message = "Already handed off; awaiting the actual operation or file-output result".into(); }
                else { self.finish(IoTaskStatus::Cancelled, "Cancelled before commit; session unchanged"); }
            }
            intent @ (AppIntent::FailIoTask { .. } | AppIntent::CompleteIoOutput { .. }) => self.complete_platform(intent),
            AppIntent::FailIoWorkflow { kind, message } if self.task.as_ref().is_some_and(|t| t.view.kind == kind) && matches!(self.phase, Phase::Input) => self.finish(IoTaskStatus::Failed, message),
            _ => {}
        }
        Ok(())
    }
    fn matches(&self, id: TaskId) -> bool {
        self.task.as_ref().is_some_and(|t| t.view.id == id)
    }
}
impl PersistenceIo for Provider {
    fn click_track(&self) -> ClickTrackState {
        ClickTrackState {
            sounds: shoop_session::click_sound_ids()
                .map(|id| ClickSoundDescriptor {
                    id: id.into(),
                    name: id.into(),
                })
                .collect::<Vec<_>>()
                .into(),
            max_click_count: shoop_session::MAX_CLICK_TRACK_CLICKS,
            max_output_frames: shoop_session::MAX_CLICK_TRACK_FRAMES,
            ..self.preview.state.clone()
        }
    }
    fn take_preview(&mut self) -> Option<ApplicationAudioPreview> {
        self.preview.take_output()
    }
    fn handles(&self, intent: &AppIntent) -> bool {
        if preview::Preview::handles(intent) {
            return true;
        }
        if matches!(intent, AppIntent::FailIoTask { task_id, .. } if task_id.raw() < (1 << 32)) {
            return false;
        }
        matches!(
            intent,
            AppIntent::RequestSaveSession
                | AppIntent::RequestLoadSessionPicker
                | AppIntent::RequestLoadSessionUrl
                | AppIntent::RequestLoadSessionSource { .. }
                | AppIntent::LoadSessionBytes { .. }
                | AppIntent::CompleteIoInput { .. }
                | AppIntent::CompleteIoOutput { .. }
                | AppIntent::RequestLoopAudioExport { .. }
                | AppIntent::RequestLoopMidiExport { .. }
                | AppIntent::RequestLoopAudioImportPicker { .. }
                | AppIntent::RequestLoopMidiImportPicker { .. }
                | AppIntent::ImportLoopAudioBytes { .. }
                | AppIntent::ImportLoopMidiBytes { .. }
                | AppIntent::GenerateClickTrack { .. }
                | AppIntent::ConfirmSampleRateConversion { .. }
                | AppIntent::ConfirmAudioChannelMapping { .. }
                | AppIntent::ConfirmAudioChannelSelection { .. }
                | AppIntent::CancelIoTask { .. }
                | AppIntent::FailIoTask { .. }
                | AppIntent::FailIoWorkflow { .. }
        )
    }
    fn complete_platform(&mut self, intent: AppIntent) {
        if preview::Preview::handles(&intent) {
            self.preview.complete(intent);
            return;
        }
        match intent {
            AppIntent::FailIoTask { task_id, message }
                if self.matches(task_id) && !matches!(self.phase, Phase::Committed(_)) =>
            {
                self.finish(IoTaskStatus::Failed, message)
            }
            AppIntent::CompleteIoOutput { task_id, outcome }
                if self.matches(task_id) && matches!(self.phase, Phase::Output) =>
            {
                let (status, message) = match outcome { FileOutputOutcome::Saved => (IoTaskStatus::Completed, "File saved"), FileOutputOutcome::DownloadHandedOff => (IoTaskStatus::Completed, "Download handed to the browser; disk completion is controlled by the browser"), FileOutputOutcome::Cancelled => (IoTaskStatus::Cancelled, "File output cancelled") };
                let warning = &self.task.as_ref().unwrap().output_warning;
                let message = if status == IoTaskStatus::Completed && !warning.is_empty() {
                    format!("{message}. {warning}")
                } else {
                    message.to_owned()
                };
                self.finish(status, message);
            }
            _ => {}
        }
    }
    fn apply(&mut self, intent: AppIntent, state: &SessionSnapshot, scripts: bool) -> Result<()> {
        if preview::Preview::handles(&intent) {
            return self.preview.apply(intent, state);
        }
        let _span =
            tracing::debug_span!("frontend.persistence.intent", intent = intent.kind()).entered();
        let starting = matches!(
            intent,
            AppIntent::RequestSaveSession
                | AppIntent::RequestLoadSessionPicker
                | AppIntent::RequestLoadSessionUrl
                | AppIntent::RequestLoadSessionSource { .. }
                | AppIntent::LoadSessionBytes { .. }
                | AppIntent::RequestLoopAudioExport { .. }
                | AppIntent::RequestLoopMidiExport { .. }
                | AppIntent::RequestLoopAudioImportPicker { .. }
                | AppIntent::RequestLoopMidiImportPicker { .. }
                | AppIntent::ImportLoopAudioBytes { .. }
                | AppIntent::ImportLoopMidiBytes { .. }
                | AppIntent::GenerateClickTrack { .. }
        );
        if starting && (!matches!(self.phase, Phase::Idle) || self.worker.is_some()) {
            bail!("another file workflow is still active");
        }
        match (&intent, &self.phase) {
            (
                AppIntent::ConfirmAudioChannelMapping {
                    task_id,
                    source_for_destination,
                },
                Phase::Decoded(decoded),
            ) if self.matches(*task_id)
                && self.task.as_ref().unwrap().view.status
                    == IoTaskStatus::AwaitingChannelMapping =>
            {
                let count = decoded.audio_labels().map_or(0, |labels| labels.len());
                ensure!(
                    source_for_destination.len() == self.task.as_ref().unwrap().audio_channels
                        && source_for_destination
                            .iter()
                            .all(|source| (*source as usize) < count),
                    "invalid audio channel mapping"
                );
            }
            (
                AppIntent::ConfirmAudioChannelSelection { task_id, channels },
                Phase::Captured(image),
            ) if self.matches(*task_id) => {
                let image::Manifest::Loop { content, .. } = &image.manifest else {
                    bail!("expected loop capture")
                };
                ensure!(
                    !channels.is_empty()
                        && channels
                            .iter()
                            .copied()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            == channels.len()
                        && channels
                            .iter()
                            .all(|channel| (*channel as usize) < content.audio.len()),
                    "invalid audio channel selection"
                );
            }
            _ => {}
        }
        if let Err(error) = self.apply_inner(intent, state, scripts) {
            if !matches!(
                self.phase,
                Phase::Idle | Phase::Committed(_) | Phase::Output
            ) {
                self.finish(IoTaskStatus::Failed, error.to_string());
            }
            return Err(error);
        }
        Ok(())
    }
    fn tick(&mut self, client: &mut Client, delivery: &mut dyn Delivery, session_scripts: bool) {
        self.preview.tick(client, delivery);
        let _span = tracing::trace_span!(
            "frontend.persistence.pump",
            task = self.task.as_ref().map_or(0, |t| t.view.id.raw())
        )
        .entered();
        if session_scripts
            && self.task.as_ref().is_some_and(|task| {
                matches!(
                    task.view.kind,
                    IoTaskKind::SaveSession | IoTaskKind::LoadSession
                )
            })
            && !matches!(
                self.phase,
                Phase::Idle | Phase::Committed(_) | Phase::Output
            )
        {
            self.finish(
                IoTaskStatus::Failed,
                "Session-owned script persistence is still unavailable (P16)",
            );
        }
        if let Err(error) = self.pump(client, delivery) {
            self.finish(IoTaskStatus::Failed, error.to_string());
        }
    }
    fn dismiss_completed(&mut self) {
        if matches!(self.phase, Phase::Idle) {
            self.task = None;
        }
    }
    fn task(&self) -> Option<IoTaskState> {
        self.task.as_ref().map(|t| t.view.clone())
    }
    fn take_output(&mut self) -> Option<ApplicationFileOutput> {
        self.output.take()
    }
    fn take_request(&mut self) -> Option<ApplicationFileRequest> {
        self.requests.pop_front()
    }
}
mod pump;
#[cfg(test)]
mod tests;
