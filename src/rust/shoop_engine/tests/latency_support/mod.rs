use shoop_engine::midi_storage::MidiStorageElem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterministicTimingConfig {
    pub loop_length: u32,
    pub callback_size: u32,
    pub input_delay: u32,
    pub processor_delay: u32,
    pub cue_output_delay: u32,
    pub backend_hop_delay: u32,
    pub manual_trim: i32,
    pub performance_reference_offset: u32,
}

impl DeterministicTimingConfig {
    pub fn direct_raw_frame(&self, logical_frame: u64) -> u64 {
        logical_frame + u64::from(self.performance_reference_offset) + u64::from(self.input_delay)
    }

    pub fn wet_raw_frame(&self, logical_frame: u64) -> u64 {
        self.direct_raw_frame(logical_frame)
            + u64::from(self.processor_delay)
            + u64::from(self.backend_hop_delay)
    }

    pub fn direct_capture_advance(&self, cue_enabled: bool) -> Option<u32> {
        let cue = cue_enabled.then_some(self.cue_output_delay).unwrap_or(0);
        checked_signed_total(&[self.input_delay, cue], self.manual_trim)
    }

    pub fn wet_capture_advance(&self, cue_enabled: bool) -> Option<u32> {
        let cue = cue_enabled.then_some(self.cue_output_delay).unwrap_or(0);
        checked_signed_total(
            &[
                self.input_delay,
                self.processor_delay,
                self.backend_hop_delay,
                cue,
            ],
            self.manual_trim,
        )
    }

    pub fn render_advance(&self) -> Option<u32> {
        checked_signed_total(
            &[self.processor_delay, self.backend_hop_delay],
            self.manual_trim,
        )
    }
}

fn checked_signed_total(values: &[u32], trim: i32) -> Option<u32> {
    let total = values
        .iter()
        .try_fold(0_i64, |total, value| total.checked_add(i64::from(*value)))?
        .checked_add(i64::from(trim))?;
    u32::try_from(total).ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentifiedAudioEvent {
    pub logical_frame: u64,
    pub id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentifiedMidiEvent {
    pub frame: u64,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct DeterministicDelayedSource {
    audio: Vec<(u32, u64)>,
    midi: Vec<IdentifiedMidiEvent>,
}

impl DeterministicDelayedSource {
    pub fn new(
        config: DeterministicTimingConfig,
        audio: &[IdentifiedAudioEvent],
        midi: &[IdentifiedMidiEvent],
    ) -> Self {
        Self {
            audio: audio
                .iter()
                .map(|event| (event.id, config.direct_raw_frame(event.logical_frame)))
                .collect(),
            midi: midi
                .iter()
                .map(|event| IdentifiedMidiEvent {
                    frame: config.direct_raw_frame(event.frame),
                    data: event.data.clone(),
                })
                .collect(),
        }
    }

    pub fn process(&self, block_start: u64, frames: u32) -> (Vec<f32>, Vec<MidiStorageElem>) {
        let block_end = block_start + u64::from(frames);
        let mut audio_output = vec![0.0; frames as usize];
        for (id, frame) in &self.audio {
            if *frame >= block_start && *frame < block_end {
                audio_output[(*frame - block_start) as usize] = identified_audio_sample(*id);
            }
        }
        let midi_output = self
            .midi
            .iter()
            .filter(|event| event.frame >= block_start && event.frame < block_end)
            .map(|event| {
                MidiStorageElem::new((event.frame - block_start) as u32, &event.data)
                    .expect("fixture MIDI event")
            })
            .collect();
        (audio_output, midi_output)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimingObservations {
    pub audio_logical: Vec<(u32, u64)>,
    pub audio_raw: Vec<(u32, u64)>,
    pub audio_dispatch: Vec<(u32, u64)>,
    pub audio_output: Vec<(u32, u64)>,
    pub midi_logical: Vec<(Vec<u8>, u64)>,
    pub midi_raw: Vec<(Vec<u8>, u64)>,
    pub midi_dispatch: Vec<(Vec<u8>, u64)>,
    pub midi_output: Vec<(Vec<u8>, u64)>,
}

#[derive(Debug)]
pub struct DeterministicDelayedProcessor {
    delay: u64,
    pending_audio: Vec<(u32, u64)>,
    pending_midi: Vec<IdentifiedMidiEvent>,
    observations: TimingObservations,
}

impl DeterministicDelayedProcessor {
    pub fn new(processor_delay: u32, backend_hop_delay: u32) -> Self {
        Self {
            delay: u64::from(processor_delay) + u64::from(backend_hop_delay),
            pending_audio: Vec::new(),
            pending_midi: Vec::new(),
            observations: TimingObservations::default(),
        }
    }

    pub fn process(
        &mut self,
        block_start: u64,
        audio: &[f32],
        midi: &[MidiStorageElem],
    ) -> (Vec<f32>, Vec<MidiStorageElem>) {
        for (offset, sample) in audio.iter().copied().enumerate() {
            if sample == 0.0 {
                continue;
            }
            let id = sample.to_bits();
            let dispatch = block_start + offset as u64;
            self.observations.audio_dispatch.push((id, dispatch));
            self.pending_audio.push((id, dispatch + self.delay));
        }
        for event in midi {
            let dispatch = block_start + u64::from(event.time);
            self.observations
                .midi_dispatch
                .push((event.data().to_vec(), dispatch));
            self.pending_midi.push(IdentifiedMidiEvent {
                frame: dispatch + self.delay,
                data: event.data().to_vec(),
            });
        }

        let block_end = block_start + audio.len() as u64;
        let mut audio_output = vec![0.0; audio.len()];
        self.pending_audio.retain(|(id, frame)| {
            if *frame >= block_start && *frame < block_end {
                audio_output[(*frame - block_start) as usize] = f32::from_bits(*id);
                self.observations.audio_output.push((*id, *frame));
                false
            } else {
                true
            }
        });
        let mut midi_output = Vec::new();
        self.pending_midi.retain(|event| {
            if event.frame >= block_start && event.frame < block_end {
                midi_output.push(
                    MidiStorageElem::new((event.frame - block_start) as u32, &event.data)
                        .expect("fixture MIDI event"),
                );
                self.observations
                    .midi_output
                    .push((event.data.clone(), event.frame));
                false
            } else {
                true
            }
        });
        (audio_output, midi_output)
    }

    pub fn observations(&self) -> &TimingObservations {
        &self.observations
    }
}

#[derive(Debug)]
pub struct DeterministicActionHarness {
    config: DeterministicTimingConfig,
    source: DeterministicDelayedSource,
    processor: DeterministicDelayedProcessor,
    source_observations: TimingObservations,
}

impl DeterministicActionHarness {
    pub fn new(
        config: DeterministicTimingConfig,
        audio: &[IdentifiedAudioEvent],
        midi: &[IdentifiedMidiEvent],
    ) -> Self {
        let mut source_observations = TimingObservations::default();
        for event in audio {
            source_observations
                .audio_logical
                .push((event.id, event.logical_frame));
            source_observations
                .audio_raw
                .push((event.id, config.direct_raw_frame(event.logical_frame)));
        }
        for event in midi {
            source_observations
                .midi_logical
                .push((event.data.clone(), event.frame));
            source_observations
                .midi_raw
                .push((event.data.clone(), config.direct_raw_frame(event.frame)));
        }
        Self {
            config,
            source: DeterministicDelayedSource::new(config, audio, midi),
            processor: DeterministicDelayedProcessor::new(
                config.processor_delay,
                config.backend_hop_delay,
            ),
            source_observations,
        }
    }

    pub fn pump(&mut self, total_frames: u64) {
        pump_callbacks(total_frames, self.config.callback_size, |start, frames| {
            let (audio, midi) = self.source.process(start, frames);
            self.processor.process(start, &audio, &midi);
        });
    }

    pub fn observations(&self) -> TimingObservations {
        let mut observations = self.source_observations.clone();
        let processor = self.processor.observations();
        observations
            .audio_dispatch
            .clone_from(&processor.audio_dispatch);
        observations
            .audio_output
            .clone_from(&processor.audio_output);
        observations
            .midi_dispatch
            .clone_from(&processor.midi_dispatch);
        observations.midi_output.clone_from(&processor.midi_output);
        observations
    }
}

pub fn identified_audio_sample(id: u32) -> f32 {
    assert!(id != 0);
    f32::from_bits(id)
}

pub fn pump_callbacks(total_frames: u64, callback_size: u32, mut pump: impl FnMut(u64, u32)) {
    assert!(callback_size > 0);
    let mut start = 0_u64;
    while start < total_frames {
        let frames = u64::from(callback_size).min(total_frames - start) as u32;
        pump(start, frames);
        start += u64::from(frames);
    }
}
