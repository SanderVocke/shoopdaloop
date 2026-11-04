use super::*;
use alloc::string::ToString;
use shoop_app_protocol::{PreviewCommand, MAX_PREVIEW_FRAMES};
pub(super) struct Stage {
    id: u64,
    bus: BusId,
    rate: u32,
    frames: usize,
    samples: Arc<Vec<f32>>,
    playing: bool,
}
impl Service {
    pub(super) fn stop_preview(&mut self) {
        self.preview_stage = None;
        let _ = self.backend.stop_audio_preview();
        if matches!(
            self.preview.preview_status,
            ClickTrackPreviewStatus::Queued | ClickTrackPreviewStatus::Playing
        ) {
            self.preview.preview_status = ClickTrackPreviewStatus::Cancelled;
        }
    }
    pub(super) fn preview_command(
        &mut self,
        command: PreviewCommand,
    ) -> Result<Effect, CommandError> {
        match command {
            PreviewCommand::Stop => self.stop_preview(),
            PreviewCommand::Begin {
                request_id,
                bus,
                sample_rate,
                frames,
            } => {
                if !self.backend.supports_audio_preview()
                    || self.preview_bus != Some(bus)
                    || !self.buses.contains_key(&bus)
                    || sample_rate != self.observed.status.sample_rate
                    || frames == 0
                    || frames as usize > MAX_PREVIEW_FRAMES
                {
                    return Err(CommandError::invalid(
                        "preview destination, sample rate or size is unavailable",
                    ));
                }
                // Operation IDs deduplicate commands; a new begin supersedes the previous stage.
                self.stop_preview();
                let mut samples = Vec::new();
                samples
                    .try_reserve_exact(frames as usize)
                    .map_err(|_| CommandError::invalid("preview budget unavailable"))?;
                self.preview_stage = Some(Stage {
                    id: request_id,
                    bus,
                    rate: sample_rate,
                    frames: frames as usize,
                    samples: Arc::new(samples),
                    playing: false,
                });
                self.preview.preview_request_id = request_id;
                self.preview.preview_output = Some(ClickTrackPreviewOutput::MasterBus);
                self.preview.preview_status = ClickTrackPreviewStatus::Queued;
                self.preview.preview_message = "Preparing Master preview".into();
            }
            PreviewCommand::Chunk {
                request_id,
                offset,
                samples,
            } => {
                let stage = self
                    .preview_stage
                    .as_mut()
                    .filter(|s| s.id == request_id && !s.playing)
                    .ok_or_else(|| CommandError::invalid("preview was superseded or stopped"))?;
                if offset as usize != stage.samples.len()
                    || stage.samples.len() + samples.len() > stage.frames
                {
                    return Err(CommandError::invalid("invalid preview chunk offset"));
                }
                Arc::get_mut(&mut stage.samples)
                    .ok_or_else(|| CommandError::invalid("preview is already admitted"))?
                    .extend(samples);
            }
            PreviewCommand::Play { request_id } => {
                let stage = self
                    .preview_stage
                    .as_mut()
                    .filter(|s| s.id == request_id && !s.playing)
                    .ok_or_else(|| CommandError::invalid("preview was superseded or stopped"))?;
                if stage.samples.len() != stage.frames {
                    return Err(CommandError::invalid("preview upload is incomplete"));
                }
                stage.playing = true;
            }
        }
        Ok(Effect::None)
    }
    pub(super) fn advance_preview(&mut self) {
        let Some(stage) = &self.preview_stage else {
            return;
        };
        if !stage.playing {
            return;
        }
        let Some(bus) = self.buses.get(&stage.bus).map(|b| b.backend) else {
            self.stop_preview();
            return;
        };
        match self.backend.start_bus_audio_preview_async(
            bus,
            stage.id,
            stage.rate,
            Arc::clone(&stage.samples),
        ) {
            Ok(BackendAsyncResult::Pending(_)) => {
                if self.backend.audio_preview_playing() {
                    self.preview.preview_status = ClickTrackPreviewStatus::Playing;
                    self.preview.preview_message = "Previewing through Master bus".into();
                }
            }
            Ok(BackendAsyncResult::Ready(())) => {
                self.preview_stage = None;
                self.preview.preview_status = ClickTrackPreviewStatus::Completed;
                self.preview.preview_message = "Click preview completed".into();
            }
            Err(error) => {
                self.preview_stage = None;
                self.preview.preview_status = ClickTrackPreviewStatus::Failed;
                self.preview.preview_message = error.to_string();
            }
        }
    }
}
