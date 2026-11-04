use super::*;

pub(super) struct Preview {
    pub state: ClickTrackState,
    codec: CodecExecutor,
    worker: Option<CodecJob>,
    output: Option<ApplicationAudioPreview>,
    samples: Option<Arc<Vec<f32>>>,
    offset: usize,
    begin: bool,
    play: bool,
    stop: bool,
    pending: VecDeque<(OperationId, Option<ClientMessage>)>,
    epoch: Option<u64>,
    bus: Option<BusId>,
    rate: u32,
}
impl Preview {
    pub fn new() -> Self {
        Self {
            state: ClickTrackState::default(),
            codec: CodecExecutor::new(),
            worker: None,
            output: None,
            samples: None,
            offset: 0,
            begin: false,
            play: false,
            stop: false,
            pending: VecDeque::new(),
            epoch: None,
            bus: None,
            rate: 0,
        }
    }
    pub fn handles(intent: &AppIntent) -> bool {
        matches!(
            intent,
            AppIntent::PreviewClickTrack { .. }
                | AppIntent::StopClickTrackPreview
                | AppIntent::ClickTrackPreviewStarted { .. }
                | AppIntent::CompleteClickTrackPreview { .. }
        )
    }
    fn cancel(&mut self) {
        self.codec.reset();
        self.worker = None;
        self.output = None;
        self.samples = None;
        self.begin = false;
        self.play = false;
        self.stop |= self.bus.is_some();
        if matches!(
            self.state.preview_status,
            ClickTrackPreviewStatus::Queued | ClickTrackPreviewStatus::Playing
        ) {
            self.state.preview_status = ClickTrackPreviewStatus::Cancelled;
        }
    }
    pub fn complete(&mut self, intent: AppIntent) {
        match intent {
            AppIntent::ClickTrackPreviewStarted { request_id }
                if self.state.preview_request_id == request_id
                    && self.state.preview_status == ClickTrackPreviewStatus::Queued =>
            {
                self.state.preview_status = ClickTrackPreviewStatus::Playing
            }
            AppIntent::CompleteClickTrackPreview {
                request_id,
                success,
                message,
            } if self.state.preview_request_id == request_id
                && matches!(
                    self.state.preview_status,
                    ClickTrackPreviewStatus::Queued | ClickTrackPreviewStatus::Playing
                ) =>
            {
                self.state.preview_status = if success {
                    ClickTrackPreviewStatus::Completed
                } else {
                    ClickTrackPreviewStatus::Failed
                };
                self.state.preview_message = message;
            }
            _ => {}
        }
    }
    pub fn apply(&mut self, intent: AppIntent, state: &SessionSnapshot) -> Result<()> {
        match intent {
            AppIntent::StopClickTrackPreview => self.cancel(),
            AppIntent::PreviewClickTrack {
                loop_id,
                request,
                output,
            } => {
                ensure!(
                    request.kind == ClickTrackKind::Audio,
                    "MIDI clicks cannot be previewed as audio"
                );
                ensure!(
                    state
                        .tracks
                        .iter()
                        .flat_map(|t| &t.loops)
                        .any(|l| l.id == loop_id
                            && l.has_audio
                            && l.composite_kind == CompositeKind::None),
                    "preview requires an audio loop"
                );
                let bus = if output == ClickTrackPreviewOutput::MasterBus {
                    Some(
                        state
                            .preview_bus
                            .ok_or_else(|| anyhow!("Master bus is unavailable"))?,
                    )
                } else {
                    None
                };
                let timing = shoop_session::generate_click_track_timing(
                    shoop_session::ClickTrackTimingSpec {
                        bpm: request.bpm,
                        click_count: request.click_count,
                        odd_click_delay_percent: request.odd_click_delay_percent,
                    },
                    state.status.sample_rate,
                )?;
                ensure!(
                    timing.output_frames as usize <= MAX_PREVIEW_FRAMES,
                    "Preview is too long; shorten the click count"
                );
                self.cancel();
                let id = NEXT_FILE_TASK.fetch_add(1, Ordering::Relaxed);
                self.epoch = Some(state.persistence_epoch);
                self.bus = bus;
                self.rate = state.status.sample_rate;
                self.worker = Some(self.codec.start(Work::GenerateClick {
                    request,
                    rate: self.rate,
                })?);
                self.state.preview_request_id = id;
                self.state.preview_output = Some(output);
                self.state.preview_status = ClickTrackPreviewStatus::Queued;
                self.state.preview_message = "Preparing click preview".into();
            }
            other => self.complete(other),
        }
        Ok(())
    }
    pub fn take_output(&mut self) -> Option<ApplicationAudioPreview> {
        self.output.take()
    }
    fn advance(&mut self, client: &mut Client, delivery: &mut dyn Delivery) -> Result<()> {
        if client.snapshot().is_some_and(|state| {
            self.epoch
                .is_some_and(|epoch| epoch != state.persistence_epoch)
        }) {
            self.cancel();
            self.epoch = None;
        }
        if self.stop {
            self.pending.retain(|(operation, message)| {
                if message.is_some() {
                    client.unsent(*operation);
                    false
                } else {
                    true
                }
            });
        }
        for (_, message) in &mut self.pending {
            if let Some(value) = message.take() {
                match delivery.send(value.clone()) {
                    Ok(()) => {}
                    Err(Error::Full) => {
                        *message = Some(value);
                        return Ok(());
                    }
                    Err(e) => bail!("{e:?}"),
                }
            }
        }
        while let Some((operation, _)) = self.pending.front() {
            let Some(outcome) = client.results().get(operation) else {
                break;
            };
            if let Outcome::Refused { message, .. } = outcome {
                let message = message.clone();
                self.pending.clear();
                bail!("{message}");
            }
            self.pending.pop_front();
        }
        if self.pending.len() >= 16 && !self.stop {
            return Ok(());
        }
        let Some(state) = client.snapshot() else {
            return Ok(());
        };
        self.state.master_preview_available = state.preview.master_preview_available;
        if self
            .epoch
            .is_some_and(|epoch| epoch != state.persistence_epoch)
        {
            self.cancel();
            self.epoch = None;
        }
        if self.bus.is_some()
            && state.preview.preview_request_id == self.state.preview_request_id
            && !self.begin
            && self.samples.is_none()
            && self.worker.is_none()
            && !self.stop
            && matches!(
                self.state.preview_status,
                ClickTrackPreviewStatus::Queued | ClickTrackPreviewStatus::Playing
            )
        {
            self.state.preview_status = state.preview.preview_status;
            self.state.preview_message = state.preview.preview_message.clone();
        }
        if let Some(result) = self.worker.as_mut().and_then(CodecJob::poll) {
            self.worker = None;
            let result = result.map_err(|e| anyhow!(e))?;
            if let Value::Decoded(decoded) = result {
                ensure!(
                    decoded.audio_labels().is_some(),
                    "click generator returned no audio"
                );
                self.worker = Some(self.codec.start(Work::Prepare {
                    decoded,
                    mapping: vec![0],
                    update_length: true,
                })?);
                return Ok(());
            }
            let Value::Image(image) = result else {
                bail!("click generator returned no audio");
            };
            let image::Manifest::Update { sample_rate, .. } = &image.manifest else {
                bail!("click generator returned invalid audio");
            };
            let Some(image::Payload::Audio(samples)) = image.payloads.first() else {
                bail!("preview contains no audio");
            };
            ensure!(
                !samples.is_empty() && samples.len() <= MAX_PREVIEW_FRAMES,
                "preview exceeds its frame budget"
            );
            if self.bus.is_none() {
                self.output = Some(ApplicationAudioPreview {
                    output: ClickTrackPreviewOutput::System,
                    request_id: self.state.preview_request_id,
                    sample_rate: *sample_rate,
                    samples: Arc::from(samples.as_slice()),
                });
            } else {
                self.samples = Some(Arc::clone(samples));
                self.offset = 0;
                self.begin = true;
                self.play = true;
            }
        }
        let id = self.state.preview_request_id;
        let command = if self.stop {
            self.stop = false;
            Some(PreviewCommand::Stop)
        } else if self.begin {
            self.begin = false;
            Some(PreviewCommand::Begin {
                request_id: id,
                bus: self.bus.ok_or_else(|| anyhow!("preview bus missing"))?,
                sample_rate: self.rate,
                frames: self.samples.as_ref().unwrap().len() as u32,
            })
        } else if let Some(samples) = &self.samples {
            if self.offset < samples.len() {
                let end = (self.offset + PREVIEW_CHUNK_FRAMES).min(samples.len());
                let command = PreviewCommand::Chunk {
                    request_id: id,
                    offset: self.offset as u32,
                    samples: samples[self.offset..end].to_vec(),
                };
                self.offset = end;
                Some(command)
            } else if self.play {
                self.play = false;
                self.samples = None;
                Some(PreviewCommand::Play { request_id: id })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(command) = command {
            let message = client
                .command(Command::Preview(command), None)
                .map_err(|e| anyhow!("{e:?}"))?;
            let ClientMessage::Submit { operation, .. } = &message else {
                bail!("invalid preview submission");
            };
            self.pending.push_back((*operation, Some(message)));
        }
        Ok(())
    }
    pub fn tick(&mut self, client: &mut Client, delivery: &mut dyn Delivery) {
        if client.state() != State::Ready {
            self.cancel();
            self.pending.clear();
            return;
        }
        for _ in 0..16 {
            if let Err(error) = self.advance(client, delivery) {
                self.cancel();
                self.state.preview_status = ClickTrackPreviewStatus::Failed;
                self.state.preview_message = error.to_string();
                break;
            }
        }
    }
}
