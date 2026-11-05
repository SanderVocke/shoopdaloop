//! Canonical built-in state envelopes. Preset availability is checked by the host.
use crate::{Kind, STATE_SCALARS};
use alloc::{format, string::String};
use core::fmt::Write;

const FX_ORDER: [usize; 32] = [
    23, 0, 1, 2, 3, 4, 24, 29, 5, 6, 7, 8, 25, 9, 10, 11, 26, 12, 13, 14, 15, 27, 30, 16, 17, 18,
    19, 20, 28, 31, 21, 22,
];
const DRIVE: [&str; 4] = ["saturation", "overdrive", "distortion", "fuzz"];
const MODULATION: [&str; 3] = ["tremolo", "flanger", "phaser"];
const REVERB: [&str; 3] = ["room", "hall", "plate"];
const DEFAULT_FX: [f32; 23] = [
    -18., 4., 10., 150., 0., 12., 0.5, 1., 0., 0., 0., 0., 0.3, 0.5, 0.3, 1., 0.5, 0.5, 0.5, 0.25,
    1., 0.2, 0.5,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidState;
impl core::fmt::Display for InvalidState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("invalid or unsupported built-in processor state")
    }
}
impl core::error::Error for InvalidState {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    kind: Kind,
    pub values: [f32; STATE_SCALARS],
}
impl State {
    pub fn new(kind: Kind) -> Result<Self, InvalidState> {
        let mut values = [0.; STATE_SCALARS];
        match kind {
            Kind::BuiltInFx => {
                values[..23].copy_from_slice(&DEFAULT_FX);
                values[28] = 1.;
                values[32..55].fill(-1.);
                values[55] = 1.;
            }
            Kind::OxiSynth => values[3..5].fill(-1.),
            Kind::External => return Err(InvalidState),
        }
        Ok(Self { kind, values })
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn validate(&self) -> Result<(), InvalidState> {
        let valid = match self.kind {
            Kind::BuiltInFx => {
                self.values[..23].iter().enumerate().all(|(index, &value)| {
                    let (min, max) = match index {
                        0 => (-48., 0.),
                        1 => (1., 20.),
                        2 => (0.5, 100.),
                        3 => (20., 1000.),
                        4 => (0., 18.),
                        5 => (0., 36.),
                        8 => (-18., 6.),
                        9..=11 => (-12., 12.),
                        12 | 16 => (0.05, 5.),
                        19 => (-0.95, 0.95),
                        _ => (0., 1.),
                    };
                    value.is_finite() && (min..=max).contains(&value)
                }) && self.values[23..29].iter().all(|&v| integer(v, 0, 1))
                    && integer(self.values[29], 0, 3)
                    && self.values[30..32].iter().all(|&v| integer(v, 0, 2))
                    && self.values[32..55]
                        .iter()
                        .all(|&v| v == -1. || integer(v, 0, 2047))
                    && integer(self.values[55], 1, 8)
            }
            Kind::OxiSynth => {
                self.values[..2]
                    .iter()
                    .all(|&v| v.is_finite() && (0. ..=1.).contains(&v))
                    && integer(self.values[2], 0, 65535 * 128 + 127)
                    && self.values[3..5]
                        .iter()
                        .all(|&v| v == -1. || integer(v, 0, 2047))
            }
            Kind::External => false,
        };
        if valid {
            Ok(())
        } else {
            Err(InvalidState)
        }
    }
    pub fn encode(&self) -> String {
        match self.kind {
            Kind::BuiltInFx => {
                let mut out = String::from("shoop-builtin-fx:2");
                for index in FX_ORDER {
                    let value = self.values[index];
                    out.push(':');
                    match index {
                        23..=28 => out.push(if value == 0. { '0' } else { '1' }),
                        29..=31 => {
                            out.push_str(tags(index).get(value as usize).copied().unwrap_or(""))
                        }
                        _ => write!(&mut out, "{:08x}", value.to_bits()).expect("string write"),
                    }
                }
                out
            }
            Kind::OxiSynth => {
                let preset = self.values[2] as u32;
                format!(
                    "shoop-oxisynth:2:timgm6mb:{}:{}:{:08x}:{:08x}",
                    preset / 128,
                    preset % 128,
                    self.values[0].to_bits(),
                    self.values[1].to_bits()
                )
            }
            Kind::External => String::new(),
        }
    }
    pub fn decode(kind: Kind, encoded: &str) -> Result<Self, InvalidState> {
        if encoded.len() > 1024 {
            return Err(InvalidState);
        }
        let mut state = Self::new(kind)?;
        let mut fields = encoded.split(':');
        match kind {
            Kind::BuiltInFx => {
                if fields.next() != Some("shoop-builtin-fx") {
                    return Err(InvalidState);
                }
                match fields.next() {
                    Some("1") => {
                        state.values[28] = boolean(fields.next())?;
                        if fields.next().is_some() {
                            return Err(InvalidState);
                        }
                        return Ok(state);
                    }
                    Some("2") => {}
                    _ => return Err(InvalidState),
                }
                for index in FX_ORDER {
                    state.values[index] = match index {
                        23..=28 => boolean(fields.next())?,
                        29..=31 => {
                            let value = fields.next().ok_or(InvalidState)?;
                            tags(index)
                                .iter()
                                .position(|&tag| tag == value)
                                .ok_or(InvalidState)? as f32
                        }
                        _ => float(fields.next())?,
                    };
                }
            }
            Kind::OxiSynth => {
                if fields.next() != Some("shoop-oxisynth")
                    || fields.next() != Some("2")
                    || fields.next() != Some("timgm6mb")
                {
                    return Err(InvalidState);
                }
                let bank = fields
                    .next()
                    .ok_or(InvalidState)?
                    .parse::<u16>()
                    .map_err(|_| InvalidState)?;
                let program = fields
                    .next()
                    .ok_or(InvalidState)?
                    .parse::<u8>()
                    .map_err(|_| InvalidState)?;
                if program > 127 {
                    return Err(InvalidState);
                }
                state.values[2] = (u32::from(bank) * 128 + u32::from(program)) as f32;
                state.values[0] = float(fields.next())?;
                state.values[1] = float(fields.next())?;
            }
            Kind::External => return Err(InvalidState),
        }
        if fields.next().is_some() {
            return Err(InvalidState);
        }
        state.validate()?;
        if state.encode() != encoded {
            return Err(InvalidState);
        }
        Ok(state)
    }
}
fn tags(index: usize) -> &'static [&'static str] {
    match index {
        29 => &DRIVE,
        30 => &MODULATION,
        _ => &REVERB,
    }
}
fn boolean(field: Option<&str>) -> Result<f32, InvalidState> {
    match field {
        Some("0") => Ok(0.),
        Some("1") => Ok(1.),
        _ => Err(InvalidState),
    }
}
fn float(field: Option<&str>) -> Result<f32, InvalidState> {
    let field = field.ok_or(InvalidState)?;
    if field.len() != 8 {
        return Err(InvalidState);
    }
    Ok(f32::from_bits(
        u32::from_str_radix(field, 16).map_err(|_| InvalidState)?,
    ))
}
fn integer(value: f32, min: u32, max: u32) -> bool {
    value.is_finite() && value >= min as f32 && value <= max as f32 && value as u32 as f32 == value
}
