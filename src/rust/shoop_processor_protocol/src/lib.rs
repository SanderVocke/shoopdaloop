//! Portable host startup policy and bounded processor state, without DSP libraries.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "alloc")]
extern crate alloc;
pub mod midi;
#[cfg(feature = "alloc")]
pub mod state;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

pub const STAGE_BASE: u32 = 0x100;
pub const DRIVE_TYPE: u32 = 0x200;
pub const MODULATION_TYPE: u32 = 0x201;
pub const REVERB_TYPE: u32 = 0x202;
pub const CC_BASE: u32 = 0x1000;
pub const CLEAR_CC: u32 = 0x3000;
pub const WIDTH: u32 = 0x4000;
pub const RESET: u32 = 0x4001;
pub const SYNTH_PRESET: u32 = 2;
pub const SYNTH_PANIC: u32 = 3;
pub const MAX_INSTANCES: u16 = 64;
pub const STATE_SCALARS: usize = 64;
pub const MAX_OPAQUE_STATE_BYTES: usize = 16 * 1024 * 1024;
/// A Base64 state envelope, including a possible serialized terminator and metadata.
pub const MAX_OPAQUE_STATE_ENVELOPE_BYTES: usize =
    (MAX_OPAQUE_STATE_BYTES + 1).div_ceil(3) * 4 + 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    Active(bool),
    Parameter {
        parameter: u32,
        value: f32,
    },
    Restore {
        values: [f32; STATE_SCALARS],
        replace_mappings: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Phase {
    Free,
    Owned,
    Running,
    Retiring,
    Faulted,
}

/// Lifecycle of the native service that owns a prepared hosted processor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostedLifecycle {
    Stopped,
    Starting,
    Running,
    Degraded,
    Crashed,
    Restarting,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostedFailure {
    Startup,
    Protocol,
    UnexpectedExit,
    Unresponsive,
    Disconnected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostedStatus {
    pub generation: u64,
    pub lifecycle: HostedLifecycle,
    pub active: bool,
    pub failure: Option<HostedFailure>,
    pub editor_visible: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    /// Exclusive source-frame end of the batch represented by these values.
    pub source_frame: u64,
    pub phase: Phase,
    pub global_midi: GlobalMidiDiagnostics,
    pub worker: Option<WorkerDiagnostics>,
    pub hosted: Option<HostedStatus>,
    pub values: [f32; STATE_SCALARS],
}

/// Bounded render-side observations for the current worker stream.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkerDiagnostics {
    pub pending: u32,
    pub peak_pending: u32,
    pub missed: u64,
    pub stale: u64,
    pub rejected: u64,
    pub failed: u64,
    pub invalid: u64,
    /// Frames between the input batch end and the render-side result.
    pub last_completion_frames: u64,
    pub peak_completion_frames: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GlobalMidiDiagnostics {
    pub latest_cc: Option<[u8; 3]>,
    pub rejected: u64,
    pub overwritten: u64,
    pub deferred: u64,
    pub drained: u64,
    pub pending: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Kind {
    External = 0,
    BuiltInFx = 1,
    OxiSynth = 2,
    CarlaRack = 3,
    CarlaPatchbay = 4,
    CarlaPatchbay16x = 5,
}
pub const BUILTIN_FX_MAX_AUDIO_CHANNELS: usize = 8;

impl Kind {
    pub const fn is_carla(self) -> bool {
        matches!(
            self,
            Self::CarlaRack | Self::CarlaPatchbay | Self::CarlaPatchbay16x
        )
    }
    pub const fn supports_bus(self) -> bool {
        matches!(self, Self::BuiltInFx) || self.is_carla()
    }
    pub const fn fixed_audio_channels(self) -> Option<usize> {
        match self {
            Self::OxiSynth | Self::CarlaRack | Self::CarlaPatchbay => Some(2),
            Self::CarlaPatchbay16x => Some(16),
            Self::BuiltInFx | Self::External => None,
        }
    }
    pub fn prepared_audio_channels(self, strip_capacity: usize) -> usize {
        match self {
            Self::BuiltInFx => strip_capacity.min(BUILTIN_FX_MAX_AUDIO_CHANNELS),
            _ => self.fixed_audio_channels().unwrap_or(strip_capacity),
        }
    }
    pub const fn scalar_count(self) -> usize {
        match self {
            Self::External | Self::CarlaRack | Self::CarlaPatchbay | Self::CarlaPatchbay16x => 0,
            Self::BuiltInFx => 56,
            Self::OxiSynth => 5,
        }
    }
    pub const fn scalar_id(self, index: usize) -> Option<u32> {
        match self {
            Self::BuiltInFx => match index {
                0..=22 => Some(index as u32),
                23..=28 => Some(STAGE_BASE + (index - 23) as u32),
                29 => Some(DRIVE_TYPE),
                30 => Some(MODULATION_TYPE),
                31 => Some(REVERB_TYPE),
                32..=54 => Some(CC_BASE + (index - 32) as u32),
                55 => Some(WIDTH),
                _ => None,
            },
            Self::OxiSynth => match index {
                0..=2 => Some(index as u32),
                3..=4 => Some(CC_BASE + (index - 3) as u32),
                _ => None,
            },
            Self::External | Self::CarlaRack | Self::CarlaPatchbay | Self::CarlaPatchbay16x => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[repr(u8)]
pub enum Policy {
    #[default]
    Auto = 0,
    On = 1,
    Off = 2,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[repr(u8)]
pub enum CarlaHosting {
    #[default]
    Disabled = 0,
    InProcess = 1,
    Subprocess = 2,
}

// Native Carla v2 accepts up to 1,024 short packets per block. The engine may
// provision smaller quotas based on its own MIDI pools.
pub const CARLA_MIDI_EVENTS: usize = 1024;
pub const CARLA_MIDI_BYTES: usize = CARLA_MIDI_EVENTS * 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct CarlaSettings {
    pub hosting: CarlaHosting,
    pub rack_instances: u16,
    pub patchbay_instances: u16,
    pub patchbay16x_instances: u16,
}

impl CarlaSettings {
    pub const fn configured_instances(self) -> u32 {
        match self.hosting {
            CarlaHosting::Disabled => 0,
            _ => {
                self.rack_instances as u32
                    + self.patchbay_instances as u32
                    + self.patchbay16x_instances as u32
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct Settings {
    pub builtin_fx: Policy,
    pub oxisynth: Policy,
    pub fx_instances: u16,
    pub synth_instances: u16,
    pub memory_mb: u32,
    pub carla: CarlaSettings,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            builtin_fx: Policy::Auto,
            oxisynth: Policy::Auto,
            fx_instances: 2,
            synth_instances: 2,
            memory_mb: 256,
            carla: CarlaSettings::default(),
        }
    }
}
impl Settings {
    pub const fn disabled() -> Self {
        Self {
            builtin_fx: Policy::Off,
            oxisynth: Policy::Off,
            fx_instances: 0,
            synth_instances: 0,
            memory_mb: 0,
            carla: CarlaSettings {
                hosting: CarlaHosting::Disabled,
                rack_instances: 0,
                patchbay_instances: 0,
                patchbay16x_instances: 0,
            },
        }
    }
    pub fn validate(self) -> Result<(), Error> {
        if u32::from(self.fx_instances)
            + u32::from(self.synth_instances)
            + self.carla.configured_instances()
            > u32::from(MAX_INSTANCES)
            || self.memory_mb > 4096
        {
            return Err(Error::Capacity);
        }
        if (self.builtin_fx == Policy::On && self.fx_instances == 0)
            || (self.oxisynth == Policy::On && self.synth_instances == 0)
        {
            return Err(Error::Capacity);
        }
        if self.memory_mb == 0
            && ((self.builtin_fx != Policy::Off && self.fx_instances > 0)
                || (self.oxisynth != Policy::Off && self.synth_instances > 0)
                || self.carla.configured_instances() != 0)
        {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    pub fn resolve(self, support: Support, sample_rate: u32) -> Result<Effective, Error> {
        self.resolve_for_channels(support, sample_rate, 8)
    }
    pub fn resolve_for_channels(
        self,
        support: Support,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Effective, Error> {
        self.validate()?;
        let fx = resolve(
            Kind::BuiltInFx,
            self.builtin_fx,
            self.fx_instances,
            support.builtin_fx,
            (8000..=384000).contains(&sample_rate),
        )?;
        let synth = resolve(
            Kind::OxiSynth,
            self.oxisynth,
            self.synth_instances,
            support.oxisynth,
            (8000..=96000).contains(&sample_rate),
        )?;
        let synth = if synth.is_none() && channels < 2 {
            if self.oxisynth == Policy::On {
                return Err(Error::Unavailable(
                    Kind::OxiSynth,
                    Unavailable::ChannelCapacity,
                ));
            }
            Some(Unavailable::ChannelCapacity)
        } else {
            synth
        };
        Ok(Effective {
            fx_instances: if fx.is_none() { self.fx_instances } else { 0 },
            synth_instances: if synth.is_none() {
                self.synth_instances
            } else {
                0
            },
            fx_unavailable: fx,
            synth_unavailable: synth,
            memory_bytes: u64::from(self.memory_mb) * (1 << 20),
            carla: EffectiveCarla::unavailable(match self.carla.hosting {
                CarlaHosting::Disabled => Unavailable::Disabled,
                _ => Unavailable::NotCompiled,
            }),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Support {
    pub builtin_fx: bool,
    pub oxisynth: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Unavailable {
    Disabled = 0,
    NotCompiled = 1,
    SampleRate = 2,
    NoCapacity = 3,
    ChannelCapacity = 4,
    Runtime = 5,
    Memory = 6,
}
impl Unavailable {
    pub const fn message(self) -> &'static str {
        match self {
            Self::Disabled => "disabled at host startup",
            Self::NotCompiled => "not compiled into this host",
            Self::SampleRate => "unsupported host sample rate",
            Self::NoCapacity => "no processor instances configured",
            Self::ChannelCapacity => "processor requires two prepared audio channels per side",
            Self::Runtime => "Carla Native runtime could not be started",
            Self::Memory => "processor buffers exceed the host memory budget",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Effective {
    pub fx_instances: u16,
    pub synth_instances: u16,
    pub fx_unavailable: Option<Unavailable>,
    pub synth_unavailable: Option<Unavailable>,
    pub memory_bytes: u64,
    pub carla: EffectiveCarla,
}
impl Effective {
    pub const fn slots(self) -> usize {
        2 * (self.fx_instances as usize
            + self.synth_instances as usize
            + self.carla.instances() as usize)
    }

    pub const fn instances(self, kind: Kind) -> u16 {
        match kind {
            Kind::BuiltInFx => self.fx_instances,
            Kind::OxiSynth => self.synth_instances,
            Kind::CarlaRack => self.carla.rack_instances,
            Kind::CarlaPatchbay => self.carla.patchbay_instances,
            Kind::CarlaPatchbay16x => self.carla.patchbay16x_instances,
            Kind::External => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveCarla {
    pub rack_instances: u16,
    pub patchbay_instances: u16,
    pub patchbay16x_instances: u16,
    pub unavailable: Option<Unavailable>,
}
impl EffectiveCarla {
    pub const fn instances(self) -> u32 {
        self.rack_instances as u32
            + self.patchbay_instances as u32
            + self.patchbay16x_instances as u32
    }

    pub const fn unavailable(reason: Unavailable) -> Self {
        Self {
            rack_instances: 0,
            patchbay_instances: 0,
            patchbay16x_instances: 0,
            unavailable: Some(reason),
        }
    }
}
impl Default for EffectiveCarla {
    fn default() -> Self {
        Self::unavailable(Unavailable::Disabled)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Capacity,
    Unavailable(Kind, Unavailable),
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Capacity => f.write_str("invalid processor instance or memory capacity"),
            Self::Unavailable(kind, reason) => write!(f, "{kind:?}: {}", reason.message()),
        }
    }
}
impl core::error::Error for Error {}
fn resolve(
    kind: Kind,
    policy: Policy,
    count: u16,
    compiled: bool,
    rate: bool,
) -> Result<Option<Unavailable>, Error> {
    let reason = if policy == Policy::Off {
        Some(Unavailable::Disabled)
    } else if !compiled {
        Some(Unavailable::NotCompiled)
    } else if !rate {
        Some(Unavailable::SampleRate)
    } else if count == 0 {
        Some(Unavailable::NoCapacity)
    } else {
        None
    };
    if let Some(reason) = reason.filter(|_| policy == Policy::On) {
        return Err(Error::Unavailable(kind, reason));
    }
    Ok(reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn startup_matrix_respects_host_build_policy_rate_and_capacity() {
        for builtin_fx in [false, true] {
            for oxisynth in [false, true] {
                for fx in [Policy::Auto, Policy::On, Policy::Off] {
                    for synth in [Policy::Auto, Policy::On, Policy::Off] {
                        let settings = Settings {
                            builtin_fx: fx,
                            oxisynth: synth,
                            ..Settings::default()
                        };
                        let result = settings.resolve(
                            Support {
                                builtin_fx,
                                oxisynth,
                            },
                            48000,
                        );
                        if (fx == Policy::On && !builtin_fx) || (synth == Policy::On && !oxisynth) {
                            assert!(result.is_err());
                        } else {
                            let state = result.unwrap();
                            assert_eq!(
                                state.fx_instances,
                                if builtin_fx && fx != Policy::Off {
                                    2
                                } else {
                                    0
                                }
                            );
                            assert_eq!(
                                state.synth_instances,
                                if oxisynth && synth != Policy::Off {
                                    2
                                } else {
                                    0
                                }
                            );
                        }
                    }
                }
            }
        }
        let support = Support {
            builtin_fx: true,
            oxisynth: true,
        };
        let high = Settings::default().resolve(support, 192000).unwrap();
        assert_eq!(high.fx_instances, 2);
        assert_eq!(high.synth_unavailable, Some(Unavailable::SampleRate));
        assert!(Settings {
            oxisynth: Policy::On,
            ..Settings::default()
        }
        .resolve(support, 192000)
        .is_err());
        assert_eq!(
            Settings::disabled()
                .resolve(Support::default(), 48000)
                .unwrap()
                .slots(),
            0
        );
        assert!(Settings {
            fx_instances: 64,
            synth_instances: 1,
            ..Settings::default()
        }
        .validate()
        .is_err());
    }
    #[cfg(feature = "serde")]
    #[shoop_test]
    fn startup_json_has_defaults_and_refuses_misspelled_keys() {
        let settings: Settings = serde_json::from_str(r#"{"oxisynth":"off"}"#).unwrap();
        assert_eq!(settings.oxisynth, Policy::Off);
        assert_eq!(settings.fx_instances, 2);
        assert!(serde_json::from_str::<Settings>(r#"{"synths":4}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Settings>(&serde_json::to_string(&settings).unwrap()).unwrap(),
            settings
        );
    }
}
