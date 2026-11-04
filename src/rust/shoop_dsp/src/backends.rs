//! Stable scalar control IDs for prepared host processors. Equal-time parameters
//! are applied before MIDI; insertion order is retained within either stream.
use rt_buffered::{Backend, Batch};
use rt_runtime::{graph::PortCounts, Error};

pub use shoop_processor_protocol::{
    CC_BASE, CLEAR_CC, DRIVE_TYPE, MODULATION_TYPE, REVERB_TYPE, STAGE_BASE, SYNTH_PANIC,
    SYNTH_PRESET, WIDTH,
};

fn integer(value: f32, min: u32, max: u32) -> bool {
    value.is_finite() && value.fract() == 0. && value >= min as f32 && value <= max as f32
}
fn cc_source(value: f32) -> Option<(u8, u8)> {
    if value == -1. {
        None
    } else {
        Some(((value as u16 / 128) as u8, (value as u16 % 128) as u8))
    }
}
fn cc_value(source: Option<(u8, u8)>) -> f32 {
    source.map_or(-1., |(channel, cc)| {
        (u16::from(channel) * 128 + u16::from(cc)) as f32
    })
}

trait Instrument: Backend {
    fn render(&mut self, batch: &mut Batch<'_>, start: usize, end: usize) -> Result<(), Error>;
    fn change(&mut self, parameter: u32, value: f32) -> Result<(), Error>;
    fn midi(&mut self, data: &[u8]);
}
fn process<I: Instrument>(instrument: &mut I, batch: &mut Batch<'_>) -> Result<(), Error> {
    let ports = instrument.ports();
    if batch.audio.input_count() != ports.audio_inputs
        || batch.audio.output_count() != ports.audio_outputs
        || batch.midi_inputs.len() != 1
        || !batch.midi_outputs.is_empty()
    {
        return Err(Error::InvalidShape);
    }
    for event in batch.parameters {
        if event.offset as usize >= batch.frames {
            return Err(Error::InvalidEvent);
        }
        instrument.validate_parameter(event.parameter, event.value)?;
    }
    let mut midi = 0;
    let mut parameter = 0;
    let mut cursor = 0;
    while cursor < batch.frames {
        let next_midi = batch.midi_inputs[0]
            .get(midi)
            .map_or(batch.frames, |(at, _)| at as usize);
        let next_parameter = batch
            .parameters
            .get(parameter)
            .map_or(batch.frames, |event| event.offset as usize);
        let end = next_midi.min(next_parameter).min(batch.frames);
        if end < cursor {
            return Err(Error::InvalidEvent);
        }
        instrument.render(batch, cursor, end)?;
        cursor = end;
        while let Some(event) = batch.parameters.get(parameter) {
            if event.offset as usize != cursor {
                break;
            }
            instrument.change(event.parameter, event.value)?;
            parameter += 1;
        }
        while let Some((at, data)) = batch.midi_inputs[0].get(midi) {
            if at as usize != cursor {
                break;
            }
            instrument.midi(data);
            midi += 1;
        }
    }
    Ok(())
}

#[cfg(feature = "builtin-fx")]
mod fx {
    use super::*;
    use crate::builtin_fx::*;
    use crate::midi_storage::MidiStorageElem;

    const STAGES: [BuiltInFxStage; 6] = [
        BuiltInFxStage::Compressor,
        BuiltInFxStage::Drive,
        BuiltInFxStage::Eq,
        BuiltInFxStage::Chorus,
        BuiltInFxStage::Modulation,
        BuiltInFxStage::Reverb,
    ];
    const DRIVES: [DriveType; 4] = [
        DriveType::Saturation,
        DriveType::Overdrive,
        DriveType::Distortion,
        DriveType::Fuzz,
    ];
    const MODULATIONS: [ModulationType; 3] = [
        ModulationType::Tremolo,
        ModulationType::Flanger,
        ModulationType::Phaser,
    ];
    const REVERBS: [ReverbType; 3] = [ReverbType::Room, ReverbType::Hall, ReverbType::Plate];

    pub struct FxBackend {
        processor: BuiltInFxProcessor,
        channels: usize,
    }
    impl FxBackend {
        pub fn prepare(
            sample_rate: f32,
            frames: usize,
            channels: usize,
        ) -> Result<Self, BuiltInFxStateError> {
            if !(8000. ..=384000.).contains(&sample_rate)
                || frames == 0
                || frames > rt_buffered::MAX_BATCH_FRAMES
            {
                return Err(BuiltInFxStateError::InvalidField("processor configuration"));
            }
            if channels == 0 || channels > 8 {
                return Err(BuiltInFxStateError::InvalidAudioChannels);
            }
            Ok(Self {
                processor: BuiltInFxProcessor::new_with_channels(
                    sample_rate,
                    frames,
                    channels,
                    BuiltInFxState::default(),
                )?,
                channels,
            })
        }
        pub fn state(&self) -> BuiltInFxState {
            self.processor.state()
        }
        pub fn assignments(&self) -> BuiltInFxMidiCcAssignments {
            self.processor.midi_cc_assignments()
        }
    }
    impl Backend for FxBackend {
        fn ports(&self) -> PortCounts {
            PortCounts {
                audio_inputs: self.channels,
                audio_outputs: self.channels,
                midi_inputs: 1,
                midi_outputs: 0,
            }
        }
        fn validate_parameter(&self, id: u32, value: f32) -> Result<(), Error> {
            let valid = if let Some(parameter) = BuiltInFxParameter::ALL.get(id as usize) {
                parameter.validate(value)
            } else if (STAGE_BASE..STAGE_BASE + 6).contains(&id) {
                integer(value, 0, 1)
            } else if id == DRIVE_TYPE {
                integer(value, 0, 3)
            } else if id == MODULATION_TYPE || id == REVERB_TYPE {
                integer(value, 0, 2)
            } else if (CC_BASE..CC_BASE + 23).contains(&id) {
                value == -1. || integer(value, 0, 2047)
            } else if id == CLEAR_CC {
                value == 0.
            } else if id == WIDTH {
                integer(value, 1, self.channels as u32)
            } else {
                false
            };
            if valid {
                Ok(())
            } else {
                Err(Error::InvalidParameter)
            }
        }
        fn parameter(&self, id: u32) -> Option<f32> {
            let state = self.processor.state();
            if let Some(parameter) = BuiltInFxParameter::ALL.get(id as usize) {
                Some(parameter.value(state))
            } else if let Some(stage) = id
                .checked_sub(STAGE_BASE)
                .and_then(|i| STAGES.get(i as usize))
            {
                Some(u8::from(state.stage_enabled(*stage)) as f32)
            } else if id == DRIVE_TYPE {
                DRIVES
                    .iter()
                    .position(|v| *v == state.drive_type)
                    .map(|v| v as f32)
            } else if id == MODULATION_TYPE {
                MODULATIONS
                    .iter()
                    .position(|v| *v == state.modulation_type)
                    .map(|v| v as f32)
            } else if id == REVERB_TYPE {
                REVERBS
                    .iter()
                    .position(|v| *v == state.reverb_type)
                    .map(|v| v as f32)
            } else if let Some(parameter) = id
                .checked_sub(CC_BASE)
                .and_then(|i| BuiltInFxParameter::ALL.get(i as usize))
            {
                Some(cc_value(
                    self.assignments()
                        .iter()
                        .find(|a| a.parameter == *parameter)
                        .map(|a| (a.channel, a.controller)),
                ))
            } else if id == WIDTH {
                Some(self.processor.audio_channels() as f32)
            } else {
                None
            }
        }
        fn control(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
            self.validate_parameter(parameter, value)?;
            self.change(parameter, value)
        }
        fn suspend(&mut self) -> Result<(), Error> {
            self.processor.reset();
            Ok(())
        }
        fn reset_step(&mut self, step: usize) -> Result<bool, Error> {
            Ok(self.processor.reset_for_activation_step(step))
        }
        fn reset(&mut self) -> Result<(), Error> {
            self.processor
                .reset_for_activation(BuiltInFxState::default())
                .map_err(|_| Error::InvalidParameter)
        }
        fn process(&mut self, batch: &mut Batch<'_>) -> Result<(), Error> {
            if batch.frames > self.processor.max_frames() {
                return Err(Error::Capacity);
            }
            process(self, batch)
        }
    }
    impl Instrument for FxBackend {
        fn render(&mut self, batch: &mut Batch<'_>, start: usize, end: usize) -> Result<(), Error> {
            let frames = end - start;
            if frames == 0 {
                return Ok(());
            }
            let active = self.processor.audio_channels();
            for channel in 0..active {
                self.processor
                    .input_mut(channel, frames)
                    .ok_or(Error::InvalidShape)?
                    .copy_from_slice(
                        &batch.audio.input(channel).ok_or(Error::InvalidShape)?[start..end],
                    );
            }
            self.processor.process(frames);
            for channel in 0..self.channels {
                let output =
                    &mut batch.audio.output(channel).ok_or(Error::InvalidShape)?[start..end];
                if channel < active {
                    output.copy_from_slice(
                        self.processor
                            .output(channel, frames)
                            .ok_or(Error::InvalidShape)?,
                    );
                } else {
                    output.fill(0.);
                }
            }
            Ok(())
        }
        fn change(&mut self, id: u32, value: f32) -> Result<(), Error> {
            if let Some(parameter) = BuiltInFxParameter::ALL.get(id as usize) {
                self.processor
                    .set_parameter(*parameter, value)
                    .map_err(|_| Error::InvalidParameter)?;
            } else if let Some(stage) = id
                .checked_sub(STAGE_BASE)
                .and_then(|i| STAGES.get(i as usize))
            {
                self.processor.set_stage_enabled(*stage, value == 1.);
            } else if id == DRIVE_TYPE {
                self.processor.set_drive_type(DRIVES[value as usize]);
            } else if id == MODULATION_TYPE {
                self.processor
                    .set_modulation_type(MODULATIONS[value as usize]);
            } else if id == REVERB_TYPE {
                self.processor.set_reverb_type(REVERBS[value as usize]);
            } else if let Some(parameter) = id
                .checked_sub(CC_BASE)
                .and_then(|i| BuiltInFxParameter::ALL.get(i as usize))
            {
                if let Some((channel, controller)) = cc_source(value) {
                    self.processor.assign_midi_cc(BuiltInFxMidiCcAssignment {
                        parameter: *parameter,
                        channel,
                        controller,
                    });
                } else {
                    self.processor.remove_midi_cc(*parameter);
                }
            } else if id == CLEAR_CC {
                self.processor.clear_midi_cc_assignments();
            } else if id == WIDTH {
                self.processor
                    .set_audio_channels(value as usize)
                    .map_err(|_| Error::InvalidShape)?;
            } else {
                return Err(Error::InvalidParameter);
            }
            Ok(())
        }
        fn midi(&mut self, data: &[u8]) {
            if let Some(event) = MidiStorageElem::new(0, data) {
                self.processor.process_midi_controls_only(&[event]);
            }
        }
    }
}
#[cfg(feature = "builtin-fx")]
pub use fx::FxBackend;

#[cfg(feature = "oxisynth")]
mod synth {
    use super::*;
    use crate::{midi_storage::MidiStorageElem, oxisynth::*};

    pub struct SynthBackend {
        processor: OxiSynthProcessor,
    }
    impl SynthBackend {
        pub fn prepare(sample_rate: f32, frames: usize) -> anyhow::Result<Self> {
            anyhow::ensure!(
                (8000. ..=96000.).contains(&sample_rate),
                "OxiSynth requires a sample rate from 8000 to 96000 Hz"
            );
            anyhow::ensure!(
                frames > 0 && frames <= rt_buffered::MAX_BATCH_FRAMES,
                "invalid synth batch capacity"
            );
            Ok(Self {
                processor: OxiSynthProcessor::new(sample_rate, frames, OxiSynthState::default())?,
            })
        }
        pub fn state(&self) -> OxiSynthState {
            self.processor.state()
        }
        pub fn assignments(&self) -> OxiSynthMidiCcAssignments {
            self.processor.midi_cc_assignments()
        }
    }
    impl Backend for SynthBackend {
        fn ports(&self) -> PortCounts {
            PortCounts {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_inputs: 1,
                midi_outputs: 0,
            }
        }
        fn validate_parameter(&self, id: u32, value: f32) -> Result<(), Error> {
            let valid = if id < 2 {
                value.is_finite() && (MIN_SEND..=MAX_SEND).contains(&value)
            } else if id == SYNTH_PRESET {
                integer(value, 0, 65535 * 128 + 127)
                    && preset_descriptor(OxiSynthPresetId {
                        bank: (value as u32 / 128) as u16,
                        program: (value as u32 % 128) as u8,
                    })
                    .is_some()
            } else if id == SYNTH_PANIC || id == CLEAR_CC {
                value == 0.
            } else if (CC_BASE..CC_BASE + 2).contains(&id) {
                value == -1. || integer(value, 0, 2047)
            } else {
                false
            };
            if valid {
                Ok(())
            } else {
                Err(Error::InvalidParameter)
            }
        }
        fn parameter(&self, id: u32) -> Option<f32> {
            let state = self.state();
            match id {
                0 => Some(state.reverb_send),
                1 => Some(state.chorus_send),
                SYNTH_PRESET => Some(
                    (u32::from(state.preset.bank) * 128 + u32::from(state.preset.program)) as f32,
                ),
                _ => id
                    .checked_sub(CC_BASE)
                    .and_then(|i| OxiSynthParameter::ALL.get(i as usize))
                    .map(|parameter| {
                        cc_value(
                            self.assignments()
                                .iter()
                                .find(|a| a.parameter == *parameter)
                                .map(|a| (a.channel, a.controller)),
                        )
                    }),
            }
        }
        fn process(&mut self, batch: &mut Batch<'_>) -> Result<(), Error> {
            if batch.frames > self.processor.max_frames() {
                return Err(Error::Capacity);
            }
            process(self, batch)
        }
        fn control(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
            self.validate_parameter(parameter, value)?;
            self.change(parameter, value)
        }
        fn suspend(&mut self) -> Result<(), Error> {
            self.processor.reset_for_activation();
            Ok(())
        }
        fn reset(&mut self) -> Result<(), Error> {
            self.processor.clear_midi_cc_assignments();
            self.processor
                .select_preset(DEFAULT_PRESET)
                .map_err(|_| Error::InvalidParameter)?;
            self.processor
                .set_send(OxiSynthParameter::ReverbSend, 0.)
                .map_err(|_| Error::InvalidParameter)?;
            self.processor
                .set_send(OxiSynthParameter::ChorusSend, 0.)
                .map_err(|_| Error::InvalidParameter)?;
            self.processor.reset_for_activation();
            Ok(())
        }
    }
    impl Instrument for SynthBackend {
        fn render(&mut self, batch: &mut Batch<'_>, start: usize, end: usize) -> Result<(), Error> {
            let frames = end - start;
            if frames == 0 {
                return Ok(());
            }
            self.processor.process(frames, &[]);
            for channel in 0..2 {
                batch.audio.output(channel).ok_or(Error::InvalidShape)?[start..end]
                    .copy_from_slice(
                        self.processor
                            .output(channel, frames)
                            .ok_or(Error::InvalidShape)?,
                    );
            }
            Ok(())
        }
        fn change(&mut self, id: u32, value: f32) -> Result<(), Error> {
            match id {
                0 | 1 => self
                    .processor
                    .set_send(OxiSynthParameter::ALL[id as usize], value)
                    .map_err(|_| Error::InvalidParameter)?,
                SYNTH_PRESET => self
                    .processor
                    .select_preset(OxiSynthPresetId {
                        bank: (value as u32 / 128) as u16,
                        program: (value as u32 % 128) as u8,
                    })
                    .map_err(|_| Error::InvalidParameter)?,
                SYNTH_PANIC => self.processor.panic(),
                CLEAR_CC => self.processor.clear_midi_cc_assignments(),
                _ => {
                    let parameter = OxiSynthParameter::ALL
                        .get(id.wrapping_sub(CC_BASE) as usize)
                        .ok_or(Error::InvalidParameter)?;
                    if let Some((channel, controller)) = cc_source(value) {
                        self.processor.assign_midi_cc(OxiSynthMidiCcAssignment {
                            parameter: *parameter,
                            channel,
                            controller,
                        });
                    } else {
                        self.processor.remove_midi_cc(*parameter);
                    }
                }
            }
            Ok(())
        }
        fn midi(&mut self, data: &[u8]) {
            if let Some(event) = MidiStorageElem::new(0, data) {
                self.processor.process(0, &[event]);
            }
        }
    }
}
#[cfg(feature = "oxisynth")]
pub use synth::SynthBackend;
