mod compressor;
mod drive;
mod eq;

use self::compressor::CompressorProcessor;
use self::drive::DriveProcessor;
use self::eq::EqProcessor;
use crate::midi_cc::MidiCcSources;
use fundsp::prelude32::{reverb_stereo, AudioUnit, BufferVec};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;

const STATE_FORMAT: &str = "shoop-builtin-fx";
const STATE_VERSION: &str = "2";
const LEGACY_STATE_VERSION: &str = "1";
pub const AUDIO_CHANNELS: usize = 2;
pub const ROOM_SIZE_METERS: f32 = 10.0;
pub const REVERB_TIME_SECONDS: f32 = 2.5;
pub const DAMPING: f32 = 0.5;
pub const REVERB_GAIN: f32 = 0.2;

pub const MIN_COMPRESSOR_THRESHOLD_DB: f32 = -48.0;
pub const MAX_COMPRESSOR_THRESHOLD_DB: f32 = 0.0;
pub const MIN_COMPRESSOR_RATIO: f32 = 1.0;
pub const MAX_COMPRESSOR_RATIO: f32 = 20.0;
pub const MIN_COMPRESSOR_ATTACK_MS: f32 = 0.5;
pub const MAX_COMPRESSOR_ATTACK_MS: f32 = 100.0;
pub const MIN_COMPRESSOR_RELEASE_MS: f32 = 20.0;
pub const MAX_COMPRESSOR_RELEASE_MS: f32 = 1_000.0;
pub const MIN_COMPRESSOR_MAKEUP_DB: f32 = 0.0;
pub const MAX_COMPRESSOR_MAKEUP_DB: f32 = 18.0;
pub const MIN_DRIVE_DB: f32 = 0.0;
pub const MAX_DRIVE_DB: f32 = 36.0;
pub const MIN_DRIVE_OUTPUT_DB: f32 = -18.0;
pub const MAX_DRIVE_OUTPUT_DB: f32 = 6.0;
pub const MIN_EQ_GAIN_DB: f32 = -12.0;
pub const MAX_EQ_GAIN_DB: f32 = 12.0;
pub const MIN_CHORUS_RATE_HZ: f32 = 0.05;
pub const MAX_CHORUS_RATE_HZ: f32 = 5.0;
pub const MIN_MODULATION_RATE_HZ: f32 = 0.05;
pub const MAX_MODULATION_RATE_HZ: f32 = 5.0;
pub const MIN_MODULATION_FEEDBACK: f32 = -0.95;
pub const MAX_MODULATION_FEEDBACK: f32 = 0.95;
const MIN_NORMALIZED: f32 = 0.0;
const MAX_NORMALIZED: f32 = 1.0;
const CONTROL_SMOOTHING_MS: f32 = 10.0;
const MODULATION_SMOOTHING_MS: f32 = 20.0;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum DriveType {
    #[default]
    Saturation,
    Overdrive,
    Distortion,
    Fuzz,
}

impl DriveType {
    const fn tag(self) -> &'static str {
        match self {
            Self::Saturation => "saturation",
            Self::Overdrive => "overdrive",
            Self::Distortion => "distortion",
            Self::Fuzz => "fuzz",
        }
    }

    fn decode(value: &str) -> Option<Self> {
        match value {
            "saturation" => Some(Self::Saturation),
            "overdrive" => Some(Self::Overdrive),
            "distortion" => Some(Self::Distortion),
            "fuzz" => Some(Self::Fuzz),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ModulationType {
    #[default]
    Tremolo,
    Flanger,
    Phaser,
}

impl ModulationType {
    const fn tag(self) -> &'static str {
        match self {
            Self::Tremolo => "tremolo",
            Self::Flanger => "flanger",
            Self::Phaser => "phaser",
        }
    }

    fn decode(value: &str) -> Option<Self> {
        match value {
            "tremolo" => Some(Self::Tremolo),
            "flanger" => Some(Self::Flanger),
            "phaser" => Some(Self::Phaser),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReverbType {
    #[default]
    Room,
    Hall,
    Plate,
}

impl ReverbType {
    const fn tag(self) -> &'static str {
        match self {
            Self::Room => "room",
            Self::Hall => "hall",
            Self::Plate => "plate",
        }
    }

    fn decode(value: &str) -> Option<Self> {
        match value {
            "room" => Some(Self::Room),
            "hall" => Some(Self::Hall),
            "plate" => Some(Self::Plate),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltInFxStage {
    Compressor,
    Drive,
    Eq,
    Chorus,
    Modulation,
    Reverb,
}

impl BuiltInFxStage {
    #[cfg(test)]
    const fn index(self) -> usize {
        match self {
            Self::Compressor => 0,
            Self::Drive => 1,
            Self::Eq => 2,
            Self::Chorus => 3,
            Self::Modulation => 4,
            Self::Reverb => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum BuiltInFxParameter {
    CompressorThreshold,
    CompressorRatio,
    CompressorAttack,
    CompressorRelease,
    CompressorMakeup,
    Drive,
    DriveTone,
    DriveMix,
    DriveOutput,
    EqLow,
    EqMid,
    EqHigh,
    ChorusRate,
    ChorusDepth,
    ChorusMix,
    ChorusWidth,
    ModulationRate,
    ModulationDepth,
    ModulationMix,
    ModulationFeedback,
    ModulationSpread,
    ReverbAmount,
    ReverbTone,
}

impl BuiltInFxParameter {
    pub const ALL: [Self; 23] = [
        Self::CompressorThreshold,
        Self::CompressorRatio,
        Self::CompressorAttack,
        Self::CompressorRelease,
        Self::CompressorMakeup,
        Self::Drive,
        Self::DriveTone,
        Self::DriveMix,
        Self::DriveOutput,
        Self::EqLow,
        Self::EqMid,
        Self::EqHigh,
        Self::ChorusRate,
        Self::ChorusDepth,
        Self::ChorusMix,
        Self::ChorusWidth,
        Self::ModulationRate,
        Self::ModulationDepth,
        Self::ModulationMix,
        Self::ModulationFeedback,
        Self::ModulationSpread,
        Self::ReverbAmount,
        Self::ReverbTone,
    ];

    const fn index(self) -> usize {
        self as usize
    }

    pub fn value_from_cc(self, value: u8) -> f32 {
        let normalized = f32::from(value.min(127)) / 127.0;
        match self {
            Self::CompressorThreshold => lerp(
                MIN_COMPRESSOR_THRESHOLD_DB,
                MAX_COMPRESSOR_THRESHOLD_DB,
                normalized,
            ),
            Self::CompressorRatio => {
                MIN_COMPRESSOR_RATIO
                    + (MAX_COMPRESSOR_RATIO - MIN_COMPRESSOR_RATIO) * normalized * normalized
            }
            Self::CompressorAttack => xerp(
                MIN_COMPRESSOR_ATTACK_MS,
                MAX_COMPRESSOR_ATTACK_MS,
                normalized,
            ),
            Self::CompressorRelease => xerp(
                MIN_COMPRESSOR_RELEASE_MS,
                MAX_COMPRESSOR_RELEASE_MS,
                normalized,
            ),
            Self::CompressorMakeup => lerp(
                MIN_COMPRESSOR_MAKEUP_DB,
                MAX_COMPRESSOR_MAKEUP_DB,
                normalized,
            ),
            Self::Drive => lerp(MIN_DRIVE_DB, MAX_DRIVE_DB, normalized),
            Self::DriveOutput => lerp(MIN_DRIVE_OUTPUT_DB, MAX_DRIVE_OUTPUT_DB, normalized),
            Self::EqLow | Self::EqMid | Self::EqHigh => {
                lerp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB, normalized)
            }
            Self::ChorusRate => xerp(MIN_CHORUS_RATE_HZ, MAX_CHORUS_RATE_HZ, normalized),
            Self::ModulationRate => {
                xerp(MIN_MODULATION_RATE_HZ, MAX_MODULATION_RATE_HZ, normalized)
            }
            Self::ModulationFeedback => {
                lerp(MIN_MODULATION_FEEDBACK, MAX_MODULATION_FEEDBACK, normalized)
            }
            Self::DriveTone
            | Self::DriveMix
            | Self::ChorusDepth
            | Self::ChorusMix
            | Self::ChorusWidth
            | Self::ModulationDepth
            | Self::ModulationMix
            | Self::ModulationSpread
            | Self::ReverbAmount
            | Self::ReverbTone => normalized,
        }
    }

    pub fn value(self, state: BuiltInFxState) -> f32 {
        match self {
            Self::CompressorThreshold => state.compressor_threshold_db,
            Self::CompressorRatio => state.compressor_ratio,
            Self::CompressorAttack => state.compressor_attack_ms,
            Self::CompressorRelease => state.compressor_release_ms,
            Self::CompressorMakeup => state.compressor_makeup_db,
            Self::Drive => state.drive_db,
            Self::DriveTone => state.drive_tone,
            Self::DriveMix => state.drive_mix,
            Self::DriveOutput => state.drive_output_db,
            Self::EqLow => state.eq_low_db,
            Self::EqMid => state.eq_mid_db,
            Self::EqHigh => state.eq_high_db,
            Self::ChorusRate => state.chorus_rate_hz,
            Self::ChorusDepth => state.chorus_depth,
            Self::ChorusMix => state.chorus_mix,
            Self::ChorusWidth => state.chorus_width,
            Self::ModulationRate => state.modulation_rate_hz,
            Self::ModulationDepth => state.modulation_depth,
            Self::ModulationMix => state.modulation_mix,
            Self::ModulationFeedback => state.modulation_feedback,
            Self::ModulationSpread => state.modulation_spread,
            Self::ReverbAmount => state.reverb_amount,
            Self::ReverbTone => state.reverb_tone,
        }
    }

    fn set(self, state: &mut BuiltInFxState, value: f32) {
        match self {
            Self::CompressorThreshold => state.compressor_threshold_db = value,
            Self::CompressorRatio => state.compressor_ratio = value,
            Self::CompressorAttack => state.compressor_attack_ms = value,
            Self::CompressorRelease => state.compressor_release_ms = value,
            Self::CompressorMakeup => state.compressor_makeup_db = value,
            Self::Drive => state.drive_db = value,
            Self::DriveTone => state.drive_tone = value,
            Self::DriveMix => state.drive_mix = value,
            Self::DriveOutput => state.drive_output_db = value,
            Self::EqLow => state.eq_low_db = value,
            Self::EqMid => state.eq_mid_db = value,
            Self::EqHigh => state.eq_high_db = value,
            Self::ChorusRate => state.chorus_rate_hz = value,
            Self::ChorusDepth => state.chorus_depth = value,
            Self::ChorusMix => state.chorus_mix = value,
            Self::ChorusWidth => state.chorus_width = value,
            Self::ModulationRate => state.modulation_rate_hz = value,
            Self::ModulationDepth => state.modulation_depth = value,
            Self::ModulationMix => state.modulation_mix = value,
            Self::ModulationFeedback => state.modulation_feedback = value,
            Self::ModulationSpread => state.modulation_spread = value,
            Self::ReverbAmount => state.reverb_amount = value,
            Self::ReverbTone => state.reverb_tone = value,
        }
    }

    fn range(self) -> (f32, f32) {
        match self {
            Self::CompressorThreshold => (MIN_COMPRESSOR_THRESHOLD_DB, MAX_COMPRESSOR_THRESHOLD_DB),
            Self::CompressorRatio => (MIN_COMPRESSOR_RATIO, MAX_COMPRESSOR_RATIO),
            Self::CompressorAttack => (MIN_COMPRESSOR_ATTACK_MS, MAX_COMPRESSOR_ATTACK_MS),
            Self::CompressorRelease => (MIN_COMPRESSOR_RELEASE_MS, MAX_COMPRESSOR_RELEASE_MS),
            Self::CompressorMakeup => (MIN_COMPRESSOR_MAKEUP_DB, MAX_COMPRESSOR_MAKEUP_DB),
            Self::Drive => (MIN_DRIVE_DB, MAX_DRIVE_DB),
            Self::DriveOutput => (MIN_DRIVE_OUTPUT_DB, MAX_DRIVE_OUTPUT_DB),
            Self::EqLow | Self::EqMid | Self::EqHigh => (MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB),
            Self::ChorusRate => (MIN_CHORUS_RATE_HZ, MAX_CHORUS_RATE_HZ),
            Self::ModulationRate => (MIN_MODULATION_RATE_HZ, MAX_MODULATION_RATE_HZ),
            Self::ModulationFeedback => (MIN_MODULATION_FEEDBACK, MAX_MODULATION_FEEDBACK),
            Self::DriveTone
            | Self::DriveMix
            | Self::ChorusDepth
            | Self::ChorusMix
            | Self::ChorusWidth
            | Self::ModulationDepth
            | Self::ModulationMix
            | Self::ModulationSpread
            | Self::ReverbAmount
            | Self::ReverbTone => (MIN_NORMALIZED, MAX_NORMALIZED),
        }
    }

    fn validate(self, value: f32) -> bool {
        let (minimum, maximum) = self.range();
        value.is_finite() && (minimum..=maximum).contains(&value)
    }
}

fn lerp(minimum: f32, maximum: f32, normalized: f32) -> f32 {
    minimum + (maximum - minimum) * normalized
}

fn xerp(minimum: f32, maximum: f32, normalized: f32) -> f32 {
    minimum * (maximum / minimum).powf(normalized)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuiltInFxState {
    pub compressor_enabled: bool,
    pub compressor_threshold_db: f32,
    pub compressor_ratio: f32,
    pub compressor_attack_ms: f32,
    pub compressor_release_ms: f32,
    pub compressor_makeup_db: f32,
    pub drive_enabled: bool,
    pub drive_type: DriveType,
    pub drive_db: f32,
    pub drive_tone: f32,
    pub drive_mix: f32,
    pub drive_output_db: f32,
    pub eq_enabled: bool,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub chorus_enabled: bool,
    pub chorus_rate_hz: f32,
    pub chorus_depth: f32,
    pub chorus_mix: f32,
    pub chorus_width: f32,
    pub modulation_enabled: bool,
    pub modulation_type: ModulationType,
    pub modulation_rate_hz: f32,
    pub modulation_depth: f32,
    pub modulation_mix: f32,
    pub modulation_feedback: f32,
    pub modulation_spread: f32,
    pub reverb_enabled: bool,
    pub reverb_type: ReverbType,
    pub reverb_amount: f32,
    pub reverb_tone: f32,
}

impl Default for BuiltInFxState {
    fn default() -> Self {
        Self {
            compressor_enabled: false,
            compressor_threshold_db: -18.0,
            compressor_ratio: 4.0,
            compressor_attack_ms: 10.0,
            compressor_release_ms: 150.0,
            compressor_makeup_db: 0.0,
            drive_enabled: false,
            drive_type: DriveType::Saturation,
            drive_db: 12.0,
            drive_tone: 0.5,
            drive_mix: 1.0,
            drive_output_db: 0.0,
            eq_enabled: false,
            eq_low_db: 0.0,
            eq_mid_db: 0.0,
            eq_high_db: 0.0,
            chorus_enabled: false,
            chorus_rate_hz: 0.3,
            chorus_depth: 0.5,
            chorus_mix: 0.3,
            chorus_width: 1.0,
            modulation_enabled: false,
            modulation_type: ModulationType::Tremolo,
            modulation_rate_hz: 0.5,
            modulation_depth: 0.5,
            modulation_mix: 0.5,
            modulation_feedback: 0.25,
            modulation_spread: 1.0,
            reverb_enabled: true,
            reverb_type: ReverbType::Room,
            reverb_amount: REVERB_GAIN,
            reverb_tone: 0.5,
        }
    }
}

impl BuiltInFxState {
    pub fn encode(self) -> String {
        let mut fields = Vec::with_capacity(34);
        fields.push(STATE_FORMAT.to_owned());
        fields.push(STATE_VERSION.to_owned());
        fields.push(encode_bool(self.compressor_enabled).to_owned());
        push_float(&mut fields, self.compressor_threshold_db);
        push_float(&mut fields, self.compressor_ratio);
        push_float(&mut fields, self.compressor_attack_ms);
        push_float(&mut fields, self.compressor_release_ms);
        push_float(&mut fields, self.compressor_makeup_db);
        fields.push(encode_bool(self.drive_enabled).to_owned());
        fields.push(self.drive_type.tag().to_owned());
        push_float(&mut fields, self.drive_db);
        push_float(&mut fields, self.drive_tone);
        push_float(&mut fields, self.drive_mix);
        push_float(&mut fields, self.drive_output_db);
        fields.push(encode_bool(self.eq_enabled).to_owned());
        push_float(&mut fields, self.eq_low_db);
        push_float(&mut fields, self.eq_mid_db);
        push_float(&mut fields, self.eq_high_db);
        fields.push(encode_bool(self.chorus_enabled).to_owned());
        push_float(&mut fields, self.chorus_rate_hz);
        push_float(&mut fields, self.chorus_depth);
        push_float(&mut fields, self.chorus_mix);
        push_float(&mut fields, self.chorus_width);
        fields.push(encode_bool(self.modulation_enabled).to_owned());
        fields.push(self.modulation_type.tag().to_owned());
        push_float(&mut fields, self.modulation_rate_hz);
        push_float(&mut fields, self.modulation_depth);
        push_float(&mut fields, self.modulation_mix);
        push_float(&mut fields, self.modulation_feedback);
        push_float(&mut fields, self.modulation_spread);
        fields.push(encode_bool(self.reverb_enabled).to_owned());
        fields.push(self.reverb_type.tag().to_owned());
        push_float(&mut fields, self.reverb_amount);
        push_float(&mut fields, self.reverb_tone);
        fields.join(":")
    }

    pub fn decode(encoded: &str) -> Result<Self, BuiltInFxStateError> {
        let mut fields = encoded.split(':');
        if fields.next() != Some(STATE_FORMAT) {
            return Err(BuiltInFxStateError::InvalidEnvelope);
        }
        match fields.next() {
            Some(LEGACY_STATE_VERSION) => Self::decode_legacy(fields, encoded),
            Some(STATE_VERSION) => Self::decode_current(fields, encoded),
            Some(version) => Err(BuiltInFxStateError::UnsupportedVersion(version.to_owned())),
            None => Err(BuiltInFxStateError::InvalidEnvelope),
        }
    }

    fn decode_legacy(
        mut fields: std::str::Split<'_, char>,
        encoded: &str,
    ) -> Result<Self, BuiltInFxStateError> {
        let reverb_enabled = decode_bool(fields.next(), "reverb_enabled")?;
        if fields.next().is_some()
            || encoded
                != format!(
                    "{STATE_FORMAT}:{LEGACY_STATE_VERSION}:{}",
                    encode_bool(reverb_enabled)
                )
        {
            return Err(BuiltInFxStateError::InvalidEnvelope);
        }
        Ok(Self {
            reverb_enabled,
            ..Self::default()
        })
    }

    fn decode_current(
        mut fields: std::str::Split<'_, char>,
        encoded: &str,
    ) -> Result<Self, BuiltInFxStateError> {
        let state = Self {
            compressor_enabled: decode_bool(fields.next(), "compressor_enabled")?,
            compressor_threshold_db: decode_float(fields.next(), "compressor_threshold")?,
            compressor_ratio: decode_float(fields.next(), "compressor_ratio")?,
            compressor_attack_ms: decode_float(fields.next(), "compressor_attack")?,
            compressor_release_ms: decode_float(fields.next(), "compressor_release")?,
            compressor_makeup_db: decode_float(fields.next(), "compressor_makeup")?,
            drive_enabled: decode_bool(fields.next(), "drive_enabled")?,
            drive_type: DriveType::decode(required(fields.next(), "drive_type")?)
                .ok_or(BuiltInFxStateError::InvalidField("drive_type"))?,
            drive_db: decode_float(fields.next(), "drive")?,
            drive_tone: decode_float(fields.next(), "drive_tone")?,
            drive_mix: decode_float(fields.next(), "drive_mix")?,
            drive_output_db: decode_float(fields.next(), "drive_output")?,
            eq_enabled: decode_bool(fields.next(), "eq_enabled")?,
            eq_low_db: decode_float(fields.next(), "eq_low")?,
            eq_mid_db: decode_float(fields.next(), "eq_mid")?,
            eq_high_db: decode_float(fields.next(), "eq_high")?,
            chorus_enabled: decode_bool(fields.next(), "chorus_enabled")?,
            chorus_rate_hz: decode_float(fields.next(), "chorus_rate")?,
            chorus_depth: decode_float(fields.next(), "chorus_depth")?,
            chorus_mix: decode_float(fields.next(), "chorus_mix")?,
            chorus_width: decode_float(fields.next(), "chorus_width")?,
            modulation_enabled: decode_bool(fields.next(), "modulation_enabled")?,
            modulation_type: ModulationType::decode(required(fields.next(), "modulation_type")?)
                .ok_or(BuiltInFxStateError::InvalidField("modulation_type"))?,
            modulation_rate_hz: decode_float(fields.next(), "modulation_rate")?,
            modulation_depth: decode_float(fields.next(), "modulation_depth")?,
            modulation_mix: decode_float(fields.next(), "modulation_mix")?,
            modulation_feedback: decode_float(fields.next(), "modulation_feedback")?,
            modulation_spread: decode_float(fields.next(), "modulation_spread")?,
            reverb_enabled: decode_bool(fields.next(), "reverb_enabled")?,
            reverb_type: ReverbType::decode(required(fields.next(), "reverb_type")?)
                .ok_or(BuiltInFxStateError::InvalidField("reverb_type"))?,
            reverb_amount: decode_float(fields.next(), "reverb_amount")?,
            reverb_tone: decode_float(fields.next(), "reverb_tone")?,
        };
        if fields.next().is_some() {
            return Err(BuiltInFxStateError::InvalidEnvelope);
        }
        state.validate()?;
        if state.encode() != encoded {
            return Err(BuiltInFxStateError::InvalidEnvelope);
        }
        Ok(state)
    }

    fn validate(self) -> Result<(), BuiltInFxStateError> {
        for parameter in BuiltInFxParameter::ALL {
            if !parameter.validate(parameter.value(self)) {
                return Err(BuiltInFxStateError::InvalidParameter(parameter));
            }
        }
        Ok(())
    }

    pub fn stage_enabled(self, stage: BuiltInFxStage) -> bool {
        match stage {
            BuiltInFxStage::Compressor => self.compressor_enabled,
            BuiltInFxStage::Drive => self.drive_enabled,
            BuiltInFxStage::Eq => self.eq_enabled,
            BuiltInFxStage::Chorus => self.chorus_enabled,
            BuiltInFxStage::Modulation => self.modulation_enabled,
            BuiltInFxStage::Reverb => self.reverb_enabled,
        }
    }

    fn set_stage_enabled(&mut self, stage: BuiltInFxStage, enabled: bool) {
        match stage {
            BuiltInFxStage::Compressor => self.compressor_enabled = enabled,
            BuiltInFxStage::Drive => self.drive_enabled = enabled,
            BuiltInFxStage::Eq => self.eq_enabled = enabled,
            BuiltInFxStage::Chorus => self.chorus_enabled = enabled,
            BuiltInFxStage::Modulation => self.modulation_enabled = enabled,
            BuiltInFxStage::Reverb => self.reverb_enabled = enabled,
        }
    }

    pub fn all_stages_disabled(self) -> bool {
        [
            BuiltInFxStage::Compressor,
            BuiltInFxStage::Drive,
            BuiltInFxStage::Eq,
            BuiltInFxStage::Chorus,
            BuiltInFxStage::Modulation,
            BuiltInFxStage::Reverb,
        ]
        .into_iter()
        .all(|stage| !self.stage_enabled(stage))
    }
}

fn encode_bool(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

fn push_float(fields: &mut Vec<String>, value: f32) {
    fields.push(format!("{:08x}", value.to_bits()));
}

fn required<'a>(
    value: Option<&'a str>,
    field: &'static str,
) -> Result<&'a str, BuiltInFxStateError> {
    value.ok_or(BuiltInFxStateError::InvalidField(field))
}

fn decode_bool(value: Option<&str>, field: &'static str) -> Result<bool, BuiltInFxStateError> {
    match value {
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        _ => Err(BuiltInFxStateError::InvalidField(field)),
    }
}

fn decode_float(value: Option<&str>, field: &'static str) -> Result<f32, BuiltInFxStateError> {
    let value = required(value, field)?;
    if value.len() != 8 {
        return Err(BuiltInFxStateError::InvalidField(field));
    }
    let bits =
        u32::from_str_radix(value, 16).map_err(|_| BuiltInFxStateError::InvalidField(field))?;
    Ok(f32::from_bits(bits))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltInFxMidiCcAssignment {
    pub parameter: BuiltInFxParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltInFxMidiCcAssignments {
    sources: MidiCcSources<23>,
}

impl BuiltInFxMidiCcAssignments {
    pub fn assign(&mut self, assignment: BuiltInFxMidiCcAssignment) -> bool {
        self.sources.assign(
            assignment.parameter.index(),
            assignment.channel,
            assignment.controller,
        )
    }

    pub fn remove(&mut self, parameter: BuiltInFxParameter) {
        self.sources.remove(parameter.index());
    }

    pub fn clear(&mut self) {
        self.sources.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = BuiltInFxMidiCcAssignment> + '_ {
        BuiltInFxParameter::ALL.into_iter().filter_map(|parameter| {
            self.sources
                .source(parameter.index())
                .map(|(channel, controller)| BuiltInFxMidiCcAssignment {
                    parameter,
                    channel,
                    controller,
                })
        })
    }

    fn matching_parameter(&self, channel: u8, controller: u8) -> Option<BuiltInFxParameter> {
        self.sources
            .matching_index(channel, controller)
            .and_then(|index| BuiltInFxParameter::ALL.get(index).copied())
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum BuiltInFxStateError {
    #[error("invalid Built-in FX state envelope")]
    InvalidEnvelope,
    #[error("unsupported Built-in FX state version {0}")]
    UnsupportedVersion(String),
    #[error("invalid Built-in FX field {0}")]
    InvalidField(&'static str),
    #[error("invalid Built-in FX parameter {0:?}")]
    InvalidParameter(BuiltInFxParameter),
    #[error("Built-in FX audio channel count must be positive")]
    InvalidAudioChannels,
}

#[derive(Clone, Copy, Debug)]
struct ParameterSmoother {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
}

impl ParameterSmoother {
    fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
        }
    }

    fn set_target(&mut self, target: f32, sample_rate: f32, milliseconds: f32) {
        if self.target.to_bits() == target.to_bits() {
            return;
        }
        self.target = target;
        self.remaining = ((sample_rate.max(1.0) * milliseconds * 0.001).round() as u32).max(1);
        self.step = (self.target - self.current) / self.remaining as f32;
    }

    fn next(&mut self) -> f32 {
        if self.remaining == 0 {
            return self.current;
        }
        self.remaining -= 1;
        if self.remaining == 0 {
            self.current = self.target;
        } else {
            self.current += self.step;
        }
        self.current
    }
}

fn parameter_smoothing_ms(parameter: BuiltInFxParameter) -> f32 {
    match parameter {
        BuiltInFxParameter::ChorusRate
        | BuiltInFxParameter::ChorusDepth
        | BuiltInFxParameter::ChorusWidth
        | BuiltInFxParameter::ModulationRate
        | BuiltInFxParameter::ModulationDepth
        | BuiltInFxParameter::ModulationSpread => MODULATION_SMOOTHING_MS,
        _ => CONTROL_SMOOTHING_MS,
    }
}

#[derive(Debug)]
struct BuiltInFxRuntimeState {
    values: [AtomicU32; 23],
    revision: AtomicU64,
}

impl BuiltInFxRuntimeState {
    fn new(state: BuiltInFxState) -> Self {
        Self {
            values: std::array::from_fn(|index| {
                AtomicU32::new(BuiltInFxParameter::ALL[index].value(state).to_bits())
            }),
            revision: AtomicU64::new(1),
        }
    }

    fn publish(&self, parameter: BuiltInFxParameter, value: f32) -> u64 {
        self.values[parameter.index()].store(value.to_bits(), Ordering::Relaxed);
        self.revision.fetch_add(1, Ordering::Release) + 1
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn value(&self, parameter: BuiltInFxParameter) -> f32 {
        f32::from_bits(self.values[parameter.index()].load(Ordering::Relaxed))
    }

    fn apply_to(&self, state: &mut BuiltInFxState) {
        for parameter in BuiltInFxParameter::ALL {
            parameter.set(state, self.value(parameter));
        }
    }
}

#[derive(Debug)]
pub struct BuiltInFxControlState {
    state: BuiltInFxState,
    midi_cc_assignments: BuiltInFxMidiCcAssignments,
    runtime_state: Arc<BuiltInFxRuntimeState>,
}

impl Clone for BuiltInFxControlState {
    fn clone(&self) -> Self {
        let state = self.state();
        Self {
            state,
            midi_cc_assignments: self.midi_cc_assignments,
            runtime_state: Arc::new(BuiltInFxRuntimeState::new(state)),
        }
    }
}

impl Default for BuiltInFxControlState {
    fn default() -> Self {
        let state = BuiltInFxState::default();
        Self {
            state,
            midi_cc_assignments: BuiltInFxMidiCcAssignments::default(),
            runtime_state: Arc::new(BuiltInFxRuntimeState::new(state)),
        }
    }
}

impl BuiltInFxControlState {
    pub fn from_state(state: BuiltInFxState) -> Result<Self, BuiltInFxStateError> {
        state.validate()?;
        Ok(Self {
            state,
            midi_cc_assignments: BuiltInFxMidiCcAssignments::default(),
            runtime_state: Arc::new(BuiltInFxRuntimeState::new(state)),
        })
    }

    pub fn from_encoded(encoded: &str) -> Result<Self, BuiltInFxStateError> {
        Self::from_state(BuiltInFxState::decode(encoded)?)
    }

    pub fn state(&self) -> BuiltInFxState {
        let mut state = self.state;
        self.runtime_state.apply_to(&mut state);
        state
    }

    pub fn encode(&self) -> String {
        self.state().encode()
    }

    pub fn set_stage_enabled(&mut self, stage: BuiltInFxStage, enabled: bool) {
        self.state.set_stage_enabled(stage, enabled);
    }

    pub fn set_reverb_enabled(&mut self, enabled: bool) {
        self.set_stage_enabled(BuiltInFxStage::Reverb, enabled);
    }

    pub fn set_parameter(
        &mut self,
        parameter: BuiltInFxParameter,
        value: f32,
    ) -> Result<(), BuiltInFxStateError> {
        if !parameter.validate(value) {
            return Err(BuiltInFxStateError::InvalidParameter(parameter));
        }
        parameter.set(&mut self.state, value);
        self.runtime_state.publish(parameter, value);
        Ok(())
    }

    pub fn set_drive_type(&mut self, drive_type: DriveType) {
        self.state.drive_type = drive_type;
    }

    pub fn set_modulation_type(&mut self, modulation_type: ModulationType) {
        self.state.modulation_type = modulation_type;
    }

    pub fn set_reverb_type(&mut self, reverb_type: ReverbType) {
        self.state.reverb_type = reverb_type;
    }

    pub fn assign_midi_cc(&mut self, assignment: BuiltInFxMidiCcAssignment) -> bool {
        self.midi_cc_assignments.assign(assignment)
    }

    pub fn remove_midi_cc(&mut self, parameter: BuiltInFxParameter) {
        self.midi_cc_assignments.remove(parameter);
    }

    pub fn clear_midi_cc_assignments(&mut self) {
        self.midi_cc_assignments.clear();
    }

    pub fn midi_cc_assignments(&self) -> BuiltInFxMidiCcAssignments {
        self.midi_cc_assignments
    }

    pub fn set_midi_cc_assignments(&mut self, assignments: BuiltInFxMidiCcAssignments) {
        self.midi_cc_assignments = assignments;
    }

    pub fn prepare_processor(&self, sample_rate: f32, max_frames: usize) -> BuiltInFxProcessor {
        self.prepare_processor_with_channels(sample_rate, max_frames, AUDIO_CHANNELS)
            .expect("default Built-in FX channel count is valid")
    }

    pub fn prepare_processor_with_channels(
        &self,
        sample_rate: f32,
        max_frames: usize,
        audio_channels: usize,
    ) -> Result<BuiltInFxProcessor, BuiltInFxStateError> {
        BuiltInFxProcessor::new_with_runtime(
            sample_rate,
            max_frames,
            audio_channels,
            self.state(),
            self.midi_cc_assignments,
            Arc::clone(&self.runtime_state),
        )
    }
}

pub struct BuiltInFxProcessor {
    compressor: CompressorProcessor,
    drive: DriveProcessor,
    eq: EqProcessor,
    stereo_reverb: Option<Box<dyn AudioUnit>>,
    mono_reverbs: Vec<Box<dyn AudioUnit>>,
    state: BuiltInFxState,
    midi_cc_assignments: BuiltInFxMidiCcAssignments,
    runtime_state: Arc<BuiltInFxRuntimeState>,
    synchronized_revision: u64,
    sample_rate: f32,
    smoothers: [ParameterSmoother; 23],
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
    stage_a: Vec<Vec<f32>>,
    stage_b: Vec<Vec<f32>>,
    fundsp_input: BufferVec,
    fundsp_output: BufferVec,
    #[cfg(test)]
    stage_process_calls: [u64; 6],
}

impl std::fmt::Debug for BuiltInFxProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuiltInFxProcessor")
            .field("state", &self.state())
            .field("audio_channels", &self.audio_channels())
            .field("max_frames", &self.max_frames())
            .field("stage_buffers", &(self.stage_a.len(), self.stage_b.len()))
            .finish_non_exhaustive()
    }
}

impl BuiltInFxProcessor {
    pub fn new(sample_rate: f32, max_frames: usize, state: BuiltInFxState) -> Self {
        Self::new_with_channels(sample_rate, max_frames, AUDIO_CHANNELS, state)
            .expect("default Built-in FX channel count is valid")
    }

    pub fn new_with_channels(
        sample_rate: f32,
        max_frames: usize,
        audio_channels: usize,
        state: BuiltInFxState,
    ) -> Result<Self, BuiltInFxStateError> {
        state.validate()?;
        Self::new_with_runtime(
            sample_rate,
            max_frames,
            audio_channels,
            state,
            BuiltInFxMidiCcAssignments::default(),
            Arc::new(BuiltInFxRuntimeState::new(state)),
        )
    }

    fn new_with_runtime(
        sample_rate: f32,
        max_frames: usize,
        audio_channels: usize,
        state: BuiltInFxState,
        midi_cc_assignments: BuiltInFxMidiCcAssignments,
        runtime_state: Arc<BuiltInFxRuntimeState>,
    ) -> Result<Self, BuiltInFxStateError> {
        if audio_channels == 0 {
            return Err(BuiltInFxStateError::InvalidAudioChannels);
        }
        let sample_rate = sample_rate.max(1.0);
        let make_reverb = || {
            let mut reverb: Box<dyn AudioUnit> = Box::new(reverb_stereo(
                ROOM_SIZE_METERS,
                REVERB_TIME_SECONDS,
                DAMPING,
            ));
            reverb.set_sample_rate(f64::from(sample_rate));
            reverb.reset();
            reverb.allocate();
            reverb
        };
        let (stereo_reverb, mono_reverbs) = if audio_channels == AUDIO_CHANNELS {
            (Some(make_reverb()), Vec::new())
        } else {
            (None, (0..audio_channels).map(|_| make_reverb()).collect())
        };
        let max_frames = max_frames.max(1);
        Ok(Self {
            compressor: CompressorProcessor::new(sample_rate, audio_channels),
            drive: DriveProcessor::new(sample_rate, audio_channels),
            eq: EqProcessor::new(sample_rate, audio_channels),
            stereo_reverb,
            mono_reverbs,
            state,
            midi_cc_assignments,
            runtime_state,
            synchronized_revision: 1,
            sample_rate,
            smoothers: std::array::from_fn(|index| {
                ParameterSmoother::new(BuiltInFxParameter::ALL[index].value(state))
            }),
            inputs: (0..audio_channels).map(|_| vec![0.0; max_frames]).collect(),
            outputs: (0..audio_channels).map(|_| vec![0.0; max_frames]).collect(),
            stage_a: (0..audio_channels).map(|_| vec![0.0; max_frames]).collect(),
            stage_b: (0..audio_channels).map(|_| vec![0.0; max_frames]).collect(),
            fundsp_input: BufferVec::new(AUDIO_CHANNELS),
            fundsp_output: BufferVec::new(AUDIO_CHANNELS),
            #[cfg(test)]
            stage_process_calls: [0; 6],
        })
    }

    pub fn state(&self) -> BuiltInFxState {
        let mut state = self.state;
        self.runtime_state.apply_to(&mut state);
        state
    }

    pub fn audio_channels(&self) -> usize {
        self.inputs.len()
    }

    pub fn max_frames(&self) -> usize {
        self.inputs[0].len()
    }

    pub fn input_mut(&mut self, channel: usize, frames: usize) -> Option<&mut [f32]> {
        let frames = frames.min(self.max_frames());
        self.inputs
            .get_mut(channel)
            .map(|input| &mut input[..frames])
    }

    pub fn output(&self, channel: usize, frames: usize) -> Option<&[f32]> {
        self.outputs
            .get(channel)
            .map(|output| &output[..frames.min(self.max_frames())])
    }

    pub fn set_stage_enabled(&mut self, stage: BuiltInFxStage, enabled: bool) {
        if self.state.stage_enabled(stage) == enabled {
            return;
        }
        if !enabled {
            match stage {
                BuiltInFxStage::Compressor => self.compressor.reset(),
                BuiltInFxStage::Drive => self.drive.reset(),
                BuiltInFxStage::Eq => self.eq.reset(),
                BuiltInFxStage::Reverb => self.reset_reverbs(),
                BuiltInFxStage::Chorus | BuiltInFxStage::Modulation => {}
            }
        }
        self.state.set_stage_enabled(stage, enabled);
    }

    pub fn set_reverb_enabled(&mut self, enabled: bool) {
        self.set_stage_enabled(BuiltInFxStage::Reverb, enabled);
    }

    pub fn set_drive_type(&mut self, drive_type: DriveType) {
        if self.state.drive_type != drive_type {
            self.drive.reset();
            self.state.drive_type = drive_type;
        }
    }

    pub fn set_modulation_type(&mut self, modulation_type: ModulationType) {
        self.state.modulation_type = modulation_type;
    }

    pub fn set_reverb_type(&mut self, reverb_type: ReverbType) {
        if self.state.reverb_type != reverb_type {
            self.reset_reverbs();
            self.state.reverb_type = reverb_type;
        }
    }

    pub fn set_parameter(
        &mut self,
        parameter: BuiltInFxParameter,
        value: f32,
    ) -> Result<(), BuiltInFxStateError> {
        if !parameter.validate(value) {
            return Err(BuiltInFxStateError::InvalidParameter(parameter));
        }
        parameter.set(&mut self.state, value);
        self.smoothers[parameter.index()].set_target(
            value,
            self.sample_rate,
            parameter_smoothing_ms(parameter),
        );
        self.synchronized_revision = self.runtime_state.publish(parameter, value);
        Ok(())
    }

    pub fn assign_midi_cc(&mut self, assignment: BuiltInFxMidiCcAssignment) -> bool {
        self.midi_cc_assignments.assign(assignment)
    }

    pub fn remove_midi_cc(&mut self, parameter: BuiltInFxParameter) {
        self.midi_cc_assignments.remove(parameter);
    }

    pub fn clear_midi_cc_assignments(&mut self) {
        self.midi_cc_assignments.clear();
    }

    pub fn process_midi_controls_only(&mut self, events: &[crate::midi_storage::MidiStorageElem]) {
        self.synchronize_runtime_values();
        for event in events {
            self.apply_midi_cc(event.data());
        }
    }

    pub fn reset(&mut self) {
        self.compressor.reset();
        self.drive.reset();
        self.eq.reset();
        self.reset_reverbs();
    }

    fn reset_reverbs(&mut self) {
        if let Some(reverb) = &mut self.stereo_reverb {
            reverb.reset();
        }
        for reverb in &mut self.mono_reverbs {
            reverb.reset();
        }
    }

    pub fn process(&mut self, frames: usize) {
        self.synchronize_runtime_values();
        let frames = frames.min(self.max_frames());
        if self.state.all_stages_disabled() {
            for channel in 0..self.audio_channels() {
                self.outputs[channel][..frames].copy_from_slice(&self.inputs[channel][..frames]);
            }
            return;
        }

        let _span =
            shoop_tracing::realtime_span_detail!("engine.rt.fx.builtin_fx_process", value = frames);
        for channel in 0..self.audio_channels() {
            self.stage_a[channel][..frames].copy_from_slice(&self.inputs[channel][..frames]);
        }
        let mut source_is_a = true;

        if self.state.compressor_enabled {
            self.compressor.process(
                frames,
                &self.stage_a,
                &mut self.stage_b,
                &mut self.smoothers,
            );
            source_is_a = false;
            #[cfg(test)]
            {
                self.stage_process_calls[BuiltInFxStage::Compressor.index()] += 1;
            }
        }
        if self.state.drive_enabled {
            if source_is_a {
                self.drive.process(
                    self.state.drive_type,
                    frames,
                    &self.stage_a,
                    &mut self.stage_b,
                    &mut self.smoothers,
                );
            } else {
                self.drive.process(
                    self.state.drive_type,
                    frames,
                    &self.stage_b,
                    &mut self.stage_a,
                    &mut self.smoothers,
                );
            }
            source_is_a = !source_is_a;
            #[cfg(test)]
            {
                self.stage_process_calls[BuiltInFxStage::Drive.index()] += 1;
            }
        }
        if self.state.eq_enabled {
            if source_is_a {
                self.eq.process(
                    frames,
                    &self.stage_a,
                    &mut self.stage_b,
                    &mut self.smoothers,
                );
            } else {
                self.eq.process(
                    frames,
                    &self.stage_b,
                    &mut self.stage_a,
                    &mut self.smoothers,
                );
            }
            source_is_a = !source_is_a;
            #[cfg(test)]
            {
                self.stage_process_calls[BuiltInFxStage::Eq.index()] += 1;
            }
        }

        if self.state.reverb_enabled {
            if !source_is_a {
                for channel in 0..self.audio_channels() {
                    self.stage_a[channel][..frames]
                        .copy_from_slice(&self.stage_b[channel][..frames]);
                }
            }
            self.process_reverb(frames);
        } else {
            let source = if source_is_a {
                &self.stage_a
            } else {
                &self.stage_b
            };
            for channel in 0..self.audio_channels() {
                self.outputs[channel][..frames].copy_from_slice(&source[channel][..frames]);
            }
        }
    }

    fn process_reverb(&mut self, frames: usize) {
        if let Some(reverb) = &mut self.stereo_reverb {
            let mut start = 0;
            while start < frames {
                let chunk = (frames - start).min(fundsp::MAX_BUFFER_SIZE);
                for channel in 0..AUDIO_CHANNELS {
                    self.fundsp_input.channel_f32_mut(channel)[..chunk]
                        .copy_from_slice(&self.stage_a[channel][start..start + chunk]);
                }
                reverb.process(
                    chunk,
                    &self.fundsp_input.buffer_ref(),
                    &mut self.fundsp_output.buffer_mut(),
                );
                for channel in 0..AUDIO_CHANNELS {
                    self.stage_b[channel][start..start + chunk]
                        .copy_from_slice(&self.fundsp_output.channel_f32_mut(channel)[..chunk]);
                }
                start += chunk;
                #[cfg(test)]
                {
                    self.stage_process_calls[BuiltInFxStage::Reverb.index()] += 1;
                }
            }
        } else {
            for channel in 0..self.audio_channels() {
                let reverb = &mut self.mono_reverbs[channel];
                let mut start = 0;
                while start < frames {
                    let chunk = (frames - start).min(fundsp::MAX_BUFFER_SIZE);
                    for fundsp_channel in 0..AUDIO_CHANNELS {
                        self.fundsp_input.channel_f32_mut(fundsp_channel)[..chunk]
                            .copy_from_slice(&self.stage_a[channel][start..start + chunk]);
                    }
                    reverb.process(
                        chunk,
                        &self.fundsp_input.buffer_ref(),
                        &mut self.fundsp_output.buffer_mut(),
                    );
                    for index in 0..chunk {
                        let left = self.fundsp_output.channel_f32_mut(0)[index];
                        let right = self.fundsp_output.channel_f32_mut(1)[index];
                        self.stage_b[channel][start + index] = (left + right) * 0.5;
                    }
                    start += chunk;
                    #[cfg(test)]
                    {
                        self.stage_process_calls[BuiltInFxStage::Reverb.index()] += 1;
                    }
                }
            }
        }
        for frame in 0..frames {
            let amount = self.smoothers[BuiltInFxParameter::ReverbAmount.index()].next();
            for channel in 0..self.audio_channels() {
                self.outputs[channel][frame] =
                    self.stage_a[channel][frame] + amount * self.stage_b[channel][frame];
            }
        }
    }

    fn synchronize_runtime_values(&mut self) {
        let revision = self.runtime_state.revision();
        if revision == self.synchronized_revision {
            return;
        }
        self.runtime_state.apply_to(&mut self.state);
        for parameter in BuiltInFxParameter::ALL {
            self.smoothers[parameter.index()].set_target(
                parameter.value(self.state),
                self.sample_rate,
                parameter_smoothing_ms(parameter),
            );
        }
        self.synchronized_revision = revision;
    }

    fn apply_midi_cc(&mut self, data: &[u8]) {
        if data.len() != 3 || data[0] & 0xf0 != 0xb0 || data[1] > 127 || data[2] > 127 {
            return;
        }
        let Some(parameter) = self
            .midi_cc_assignments
            .matching_parameter(data[0] & 0x0f, data[1])
        else {
            return;
        };
        let value = parameter.value_from_cc(data[2]);
        parameter.set(&mut self.state, value);
        self.smoothers[parameter.index()].set_target(
            value,
            self.sample_rate,
            parameter_smoothing_ms(parameter),
        );
        self.synchronized_revision = self.runtime_state.publish(parameter, value);
    }

    #[cfg(test)]
    pub(crate) fn stage_process_calls(&self, stage: BuiltInFxStage) -> u64 {
        self.stage_process_calls[stage.index()]
    }

    #[cfg(test)]
    pub(crate) fn reverb_process_calls(&self) -> u64 {
        self.stage_process_calls(BuiltInFxStage::Reverb)
    }
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<BuiltInFxProcessor>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi_storage::MidiStorageElem;

    fn processor(enabled: bool, max_frames: usize) -> BuiltInFxProcessor {
        BuiltInFxProcessor::new(
            48_000.0,
            max_frames,
            BuiltInFxState {
                reverb_enabled: enabled,
                ..BuiltInFxState::default()
            },
        )
    }

    fn set_impulse(processor: &mut BuiltInFxProcessor, frames: usize) {
        for channel in 0..processor.audio_channels() {
            let input = processor.input_mut(channel, frames).unwrap();
            input.fill(0.0);
            input[0] = 1.0;
        }
    }

    fn set_silence(processor: &mut BuiltInFxProcessor, frames: usize) {
        for channel in 0..processor.audio_channels() {
            processor.input_mut(channel, frames).unwrap().fill(0.0);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn state_codec_is_canonical_strict_and_migrates_version_one() {
        let default = BuiltInFxState::default();
        let encoded = default.encode();
        assert!(encoded.starts_with("shoop-builtin-fx:2:"));
        assert_eq!(BuiltInFxState::decode(&encoded).unwrap(), default);
        assert_eq!(
            BuiltInFxState::decode("shoop-builtin-fx:1:0").unwrap(),
            BuiltInFxState {
                reverb_enabled: false,
                ..default
            }
        );
        for invalid in [
            "",
            "shoop-builtin-fx",
            "shoop-builtin-fx:1",
            "shoop-builtin-fx:1:false",
            "shoop-builtin-fx:1:01",
            "shoop-builtin-fx:1:1:extra",
            "other:1:1",
            "shoop-builtin-fx:2:0",
        ] {
            assert!(BuiltInFxState::decode(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            BuiltInFxState::decode("shoop-builtin-fx:3:1"),
            Err(BuiltInFxStateError::UnsupportedVersion("3".to_owned()))
        );

        let mut out_of_range = default;
        out_of_range.reverb_amount = 2.0;
        assert_eq!(
            BuiltInFxState::decode(&out_of_range.encode()),
            Err(BuiltInFxStateError::InvalidParameter(
                BuiltInFxParameter::ReverbAmount
            ))
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn parameter_cc_curves_cover_valid_endpoints_and_assignments_are_unique() {
        for parameter in BuiltInFxParameter::ALL {
            let (minimum, maximum) = parameter.range();
            assert!((parameter.value_from_cc(0) - minimum).abs() < 1.0e-5);
            assert!((parameter.value_from_cc(127) - maximum).abs() < 1.0e-4);
            assert!(parameter.validate(parameter.value_from_cc(64)));
        }

        let mut assignments = BuiltInFxMidiCcAssignments::default();
        assert!(assignments.assign(BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::Drive,
            channel: 1,
            controller: 7,
        }));
        assert!(assignments.assign(BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::ReverbAmount,
            channel: 1,
            controller: 7,
        }));
        assert_eq!(assignments.iter().count(), 1);
        assert_eq!(
            assignments.matching_parameter(1, 7),
            Some(BuiltInFxParameter::ReverbAmount)
        );
        assert!(assignments.assign(BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::ReverbAmount,
            channel: 2,
            controller: 9,
        }));
        assert_eq!(assignments.iter().count(), 1);
        assert_eq!(assignments.matching_parameter(1, 7), None);
        assert_eq!(
            assignments.matching_parameter(2, 9),
            Some(BuiltInFxParameter::ReverbAmount)
        );
        assert!(!assignments.assign(BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::Drive,
            channel: 16,
            controller: 7,
        }));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn parameter_smoothing_reaches_its_target_without_overshoot() {
        let mut smoother = ParameterSmoother::new(0.0);
        smoother.set_target(1.0, 1_000.0, 10.0);
        for step in 1..=10 {
            let value = smoother.next();
            assert!((value - step as f32 * 0.1).abs() < 1.0e-6);
        }
        assert_eq!(smoother.next(), 1.0);
        smoother.set_target(-1.0, 1_000.0, 20.0);
        for _ in 0..20 {
            smoother.next();
        }
        assert_eq!(smoother.next(), -1.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn control_and_processor_share_validated_midi_driven_values() {
        let mut control = BuiltInFxControlState::default();
        assert!(control.assign_midi_cc(BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::Drive,
            channel: 2,
            controller: 11,
        }));
        let mut processor = control.prepare_processor(48_000.0, 128);
        let event = MidiStorageElem::new(0, &[0xb2, 11, 127]).unwrap();
        processor.process_midi_controls_only(&[event]);
        assert_eq!(processor.state().drive_db, MAX_DRIVE_DB);
        assert_eq!(control.state().drive_db, MAX_DRIVE_DB);
        assert!(control.encode().starts_with("shoop-builtin-fx:2:"));

        let note = MidiStorageElem::new(0, &[0x92, 60, 100]).unwrap();
        processor.process_midi_controls_only(&[note]);
        assert_eq!(control.state().drive_db, MAX_DRIVE_DB);
        assert_eq!(
            control.set_parameter(BuiltInFxParameter::Drive, f32::NAN),
            Err(BuiltInFxStateError::InvalidParameter(
                BuiltInFxParameter::Drive
            ))
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabled_reverb_is_exact_passthrough_for_variable_channels_and_skips_fundsp() {
        let frames = 257;
        for channels in [1, 2, 3, 6] {
            let mut processor = BuiltInFxProcessor::new_with_channels(
                48_000.0,
                frames,
                channels,
                BuiltInFxState {
                    reverb_enabled: false,
                    ..BuiltInFxState::default()
                },
            )
            .unwrap();
            for channel in 0..channels {
                let input = processor.input_mut(channel, frames).unwrap();
                for (index, sample) in input.iter_mut().enumerate() {
                    *sample = (index as f32 + channel as f32) / frames as f32;
                }
            }
            processor.process(frames);
            for channel in 0..channels {
                assert_eq!(
                    processor.output(channel, frames).unwrap(),
                    processor.inputs[channel]
                );
            }
            assert_eq!(processor.reverb_process_calls(), 0);
        }
        assert_eq!(
            BuiltInFxProcessor::new_with_channels(48_000.0, frames, 0, BuiltInFxState::default())
                .unwrap_err(),
            BuiltInFxStateError::InvalidAudioChannels
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn compressor_reduces_peaks_links_stereo_and_keeps_n_channels_independent() {
        let frames = 4_096;
        let state = BuiltInFxState {
            compressor_enabled: true,
            compressor_threshold_db: -24.0,
            compressor_ratio: 20.0,
            compressor_attack_ms: 0.5,
            compressor_release_ms: 20.0,
            reverb_enabled: false,
            ..BuiltInFxState::default()
        };
        let mut stereo = BuiltInFxProcessor::new_with_channels(48_000.0, frames, 2, state).unwrap();
        stereo.input_mut(0, frames).unwrap().fill(1.0);
        stereo.input_mut(1, frames).unwrap().fill(0.25);
        stereo.process(frames);
        let left = stereo.output(0, frames).unwrap()[frames - 1];
        let right = stereo.output(1, frames).unwrap()[frames - 1];
        assert!(left < 0.15, "compressed left {left}");
        assert!((right / left - 0.25).abs() < 1.0e-4);
        assert_eq!(stereo.stage_process_calls(BuiltInFxStage::Compressor), 1);
        assert_eq!(stereo.stage_process_calls(BuiltInFxStage::Drive), 0);

        let mut multichannel =
            BuiltInFxProcessor::new_with_channels(48_000.0, frames, 3, state).unwrap();
        multichannel.input_mut(0, frames).unwrap().fill(1.0);
        multichannel.input_mut(1, frames).unwrap().fill(0.01);
        multichannel.input_mut(2, frames).unwrap().fill(0.0);
        multichannel.process(frames);
        assert!(multichannel.output(0, frames).unwrap()[frames - 1] < 0.15);
        assert!((multichannel.output(1, frames).unwrap()[frames - 1] - 0.01).abs() < 1.0e-4);
        assert!(multichannel
            .output(2, frames)
            .unwrap()
            .iter()
            .all(|sample| *sample == 0.0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn compressor_attack_and_release_follow_signal_transitions() {
        let frames = 4_096;
        let state = BuiltInFxState {
            compressor_enabled: true,
            compressor_threshold_db: -24.0,
            compressor_ratio: 20.0,
            compressor_attack_ms: 10.0,
            compressor_release_ms: 20.0,
            reverb_enabled: false,
            ..BuiltInFxState::default()
        };
        let mut processor =
            BuiltInFxProcessor::new_with_channels(48_000.0, frames, 1, state).unwrap();
        let input = processor.input_mut(0, frames).unwrap();
        input[..2_048].fill(1.0);
        input[2_048..].fill(0.01);
        processor.process(frames);
        let output = processor.output(0, frames).unwrap();
        assert!(output[0] > output[1_500] * 2.0);
        assert!(output[4_000] > output[2_048] * 2.0);
        assert!(output[4_000] <= 0.011);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn drive_types_are_distinct_bounded_and_skip_unselected_processing() {
        let frames = 2_048;
        let mut rendered = Vec::new();
        for drive_type in [
            DriveType::Saturation,
            DriveType::Overdrive,
            DriveType::Distortion,
            DriveType::Fuzz,
        ] {
            let state = BuiltInFxState {
                drive_enabled: true,
                drive_type,
                drive_db: 18.0,
                drive_tone: 1.0,
                drive_mix: 1.0,
                reverb_enabled: false,
                ..BuiltInFxState::default()
            };
            let mut processor =
                BuiltInFxProcessor::new_with_channels(48_000.0, frames, 1, state).unwrap();
            for (index, sample) in processor
                .input_mut(0, frames)
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                *sample = (std::f32::consts::TAU * 440.0 * index as f32 / 48_000.0).sin() * 0.4;
            }
            processor.process(frames);
            let output = processor.output(0, frames).unwrap();
            assert!(output.iter().all(|sample| sample.is_finite()));
            assert!(output.iter().all(|sample| sample.abs() <= 8.1));
            assert_eq!(processor.stage_process_calls(BuiltInFxStage::Drive), 1);
            for candidate in [
                DriveType::Saturation,
                DriveType::Overdrive,
                DriveType::Distortion,
                DriveType::Fuzz,
            ] {
                assert_eq!(
                    processor.drive.type_process_calls(candidate),
                    u64::from(candidate == drive_type)
                );
            }
            rendered.push(output.to_vec());
        }
        for left in 0..rendered.len() {
            for right in left + 1..rendered.len() {
                let difference: f32 = rendered[left]
                    .iter()
                    .zip(&rendered[right])
                    .map(|(left, right)| (left - right).abs())
                    .sum();
                assert!(difference > 1.0, "types {left} and {right}: {difference}");
            }
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn equalizer_bands_boost_and_cut_their_fixed_regions() {
        fn render(frequency: f32, parameter: BuiltInFxParameter, gain_db: f32) -> f32 {
            let frames = 8_192;
            let mut state = BuiltInFxState {
                eq_enabled: true,
                reverb_enabled: false,
                ..BuiltInFxState::default()
            };
            parameter.set(&mut state, gain_db);
            let mut processor =
                BuiltInFxProcessor::new_with_channels(48_000.0, frames, 1, state).unwrap();
            for (index, sample) in processor
                .input_mut(0, frames)
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                *sample = (std::f32::consts::TAU * frequency * index as f32 / 48_000.0).sin() * 0.1;
            }
            processor.process(frames);
            let output = &processor.output(0, frames).unwrap()[frames / 2..];
            assert_eq!(processor.stage_process_calls(BuiltInFxStage::Eq), 1);
            (output.iter().map(|sample| sample * sample).sum::<f32>() / output.len() as f32).sqrt()
        }

        for (frequency, parameter) in [
            (120.0, BuiltInFxParameter::EqLow),
            (1_000.0, BuiltInFxParameter::EqMid),
            (8_000.0, BuiltInFxParameter::EqHigh),
        ] {
            let neutral = render(frequency, parameter, 0.0);
            let boosted = render(frequency, parameter, 12.0);
            let cut = render(frequency, parameter, -12.0);
            assert!(boosted > neutral * 1.5, "{frequency}: {boosted} {neutral}");
            assert!(cut < neutral * 0.8, "{frequency}: {cut} {neutral}");
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dynamics_drive_and_eq_preserve_n_channel_isolation_and_bypass_individually() {
        let frames = 257;
        let state = BuiltInFxState {
            compressor_enabled: true,
            drive_enabled: true,
            eq_enabled: true,
            reverb_enabled: false,
            ..BuiltInFxState::default()
        };
        let mut processor =
            BuiltInFxProcessor::new_with_channels(48_000.0, frames, 6, state).unwrap();
        for channel in 0..6 {
            processor.input_mut(channel, frames).unwrap().fill(0.0);
        }
        processor.input_mut(4, frames).unwrap()[0] = 0.5;
        processor.process(frames);
        for channel in [0, 1, 2, 3, 5] {
            assert!(processor
                .output(channel, frames)
                .unwrap()
                .iter()
                .all(|sample| *sample == 0.0));
        }
        for stage in [
            BuiltInFxStage::Compressor,
            BuiltInFxStage::Drive,
            BuiltInFxStage::Eq,
        ] {
            assert_eq!(processor.stage_process_calls(stage), 1);
            processor.set_stage_enabled(stage, false);
        }
        for channel in 0..6 {
            let input = processor.input_mut(channel, frames).unwrap();
            for (index, sample) in input.iter_mut().enumerate() {
                *sample = channel as f32 + index as f32 / frames as f32;
            }
        }
        processor.process(frames);
        for channel in 0..6 {
            assert_eq!(
                processor.output(channel, frames).unwrap(),
                processor.inputs[channel]
            );
        }
        for stage in [
            BuiltInFxStage::Compressor,
            BuiltInFxStage::Drive,
            BuiltInFxStage::Eq,
        ] {
            assert_eq!(processor.stage_process_calls(stage), 1);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dynamics_drive_and_eq_support_runtime_rates_and_block_sizes() {
        let state = BuiltInFxState {
            compressor_enabled: true,
            drive_enabled: true,
            eq_enabled: true,
            reverb_enabled: false,
            ..BuiltInFxState::default()
        };
        for (sample_rate, frames) in [
            (44_100.0, 1),
            (48_000.0, 128),
            (96_000.0, 257),
            (48_000.0, 2_048),
        ] {
            let mut processor =
                BuiltInFxProcessor::new_with_channels(sample_rate, frames, 3, state).unwrap();
            for channel in 0..3 {
                for (index, sample) in processor
                    .input_mut(channel, frames)
                    .unwrap()
                    .iter_mut()
                    .enumerate()
                {
                    *sample = ((index + channel) as f32 * 0.01).sin() * 0.25;
                }
            }
            processor.process(frames);
            for channel in 0..3 {
                assert!(processor
                    .output(channel, frames)
                    .unwrap()
                    .iter()
                    .all(|sample| sample.is_finite()));
            }
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn enabled_reverb_processes_mono_stereo_and_n_channel_tails() {
        let frames = 257;
        for channels in [1, 2, 3, 6] {
            let mut processor = BuiltInFxProcessor::new_with_channels(
                48_000.0,
                frames,
                channels,
                BuiltInFxState::default(),
            )
            .unwrap();
            set_impulse(&mut processor, frames);
            processor.process(frames);
            assert!(processor.output(0, frames).unwrap()[0] > 0.9);

            let mut heard_tail = false;
            for _ in 0..400 {
                set_silence(&mut processor, frames);
                processor.process(frames);
                heard_tail |= processor
                    .output(0, frames)
                    .unwrap()
                    .iter()
                    .any(|sample| sample.abs() > 1.0e-6);
            }
            assert!(heard_tail, "channels={channels}");
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabling_discards_the_existing_tail() {
        let frames = 128;
        let mut processor = processor(true, frames);
        set_impulse(&mut processor, frames);
        processor.process(frames);
        processor.set_reverb_enabled(false);
        set_silence(&mut processor, frames);
        processor.process(frames);
        processor.set_reverb_enabled(true);
        for _ in 0..400 {
            set_silence(&mut processor, frames);
            processor.process(frames);
            assert!(processor
                .output(0, frames)
                .unwrap()
                .iter()
                .all(|sample| *sample == 0.0));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn processor_uses_runtime_sample_rate_and_maximum_block_size() {
        for (sample_rate, frames) in [(44_100.0, 1), (48_000.0, 128), (96_000.0, 2_048)] {
            let mut processor =
                BuiltInFxProcessor::new(sample_rate, frames, BuiltInFxState::default());
            assert_eq!(processor.max_frames(), frames);
            set_impulse(&mut processor, frames);
            processor.process(frames);
            assert!(processor.output(0, frames).unwrap()[0] > 0.9);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn steady_state_processing_transitions_and_midi_do_not_allocate() {
        let frames = 257;
        let mut control = BuiltInFxControlState::default();
        control.assign_midi_cc(BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::Drive,
            channel: 0,
            controller: 1,
        });
        let mut processor = control.prepare_processor(48_000.0, frames);
        set_impulse(&mut processor, frames);
        let event = MidiStorageElem::new(0, &[0xb0, 1, 64]).unwrap();
        assert_no_alloc::assert_no_alloc(|| processor.process(frames));
        assert_no_alloc::assert_no_alloc(|| processor.set_reverb_enabled(false));
        assert_no_alloc::assert_no_alloc(|| processor.process(frames));
        assert_no_alloc::assert_no_alloc(|| processor.process_midi_controls_only(&[event]));
        assert_no_alloc::assert_no_alloc(|| processor.set_reverb_enabled(true));
        assert_no_alloc::assert_no_alloc(|| processor.reset());

        let state = BuiltInFxState {
            compressor_enabled: true,
            drive_enabled: true,
            eq_enabled: true,
            reverb_enabled: false,
            ..BuiltInFxState::default()
        };
        let mut rack = BuiltInFxProcessor::new_with_channels(48_000.0, frames, 6, state).unwrap();
        set_impulse(&mut rack, frames);
        assert_no_alloc::assert_no_alloc(|| rack.process(frames));
        assert_no_alloc::assert_no_alloc(|| {
            rack.set_stage_enabled(BuiltInFxStage::Compressor, false)
        });
        assert_no_alloc::assert_no_alloc(|| rack.set_stage_enabled(BuiltInFxStage::Drive, false));
        assert_no_alloc::assert_no_alloc(|| rack.set_stage_enabled(BuiltInFxStage::Eq, false));
        assert_no_alloc::assert_no_alloc(|| rack.process(frames));
        assert_no_alloc::assert_no_alloc(|| rack.reset());
    }
}
