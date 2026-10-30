//! The configuration record that prepares the module before its first quantum.
//!
//! All numbers are little-endian `u32`, in this order: the record version, the
//! sample rate, the largest quantum, strip channels, Master channels, smoothing,
//! the smoothing limit, the ramp, the control lead, resource pages, page bytes,
//! MIDI events and bytes per port, pending transitions, reliable and bulk credits,
//! the telemetry interval, the record inbox and outbox bytes, the MIDI-in and
//! MIDI-out lane bytes and the records delivered per quantum. Then come counted
//! lists, each a `u32` count followed by its items: pool segments as strip and
//! loop counts; initial Master connections as Master channel and host audio
//! output index; and the names of the host audio inputs, audio outputs, MIDI
//! inputs and MIDI outputs, each a `u8` length and UTF-8 bytes.

use crate::lane::MAX_EVENT_BYTES;
use shoop_rt_engine::{EngineDescription, HostPorts, Segment, MAX_HOST_PORTS};
use shoop_rt_protocol::{Name, MAX_RECORD_BYTES};

pub const CONFIG_VERSION: u32 = 1;
/// Largest configuration record.
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
pub const MAX_SEGMENTS: usize = 256;
/// Largest record or MIDI lane buffer.
pub const MAX_LANE_BYTES: usize = 1 << 24;

pub const DEFAULT_RECORDS_IN_BYTES: usize = 16 * 1024;
pub const DEFAULT_RECORDS_OUT_BYTES: usize = 16 * 1024;
pub const DEFAULT_MIDI_LANE_BYTES: usize = 4 * 1024;
pub const DEFAULT_RECORDS_PER_QUANTUM: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    Version,
    Truncated,
    Trailing,
    /// A value does not fit its field or is outside the worklet's bounds.
    Range,
    /// A host port name is longer than the protocol allows or not UTF-8.
    Name,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkletConfig {
    pub engine: EngineDescription,
    /// Client record bytes the module holds between quanta; at least one largest
    /// record.
    pub records_in_bytes: usize,
    /// Engine record bytes posted per quantum; at least one largest record.
    pub records_out_bytes: usize,
    /// MIDI-in lane bytes the module holds, including events due in later quanta.
    pub midi_in_bytes: usize,
    /// MIDI-out lane bytes posted per quantum.
    pub midi_out_bytes: usize,
    /// Client records delivered to the engine per quantum.
    pub records_per_quantum: usize,
}

impl WorkletConfig {
    /// A configuration with the default lane sizes.
    pub fn new(engine: EngineDescription) -> Self {
        Self {
            engine,
            records_in_bytes: DEFAULT_RECORDS_IN_BYTES,
            records_out_bytes: DEFAULT_RECORDS_OUT_BYTES,
            midi_in_bytes: DEFAULT_MIDI_LANE_BYTES,
            midi_out_bytes: DEFAULT_MIDI_LANE_BYTES,
            records_per_quantum: DEFAULT_RECORDS_PER_QUANTUM,
        }
    }

    /// The worklet's own bounds; the engine validates its description when it is
    /// prepared.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let record_lanes = MAX_RECORD_BYTES..=MAX_LANE_BYTES;
        let midi_lanes = MAX_EVENT_BYTES..=MAX_LANE_BYTES;
        if !record_lanes.contains(&self.records_in_bytes)
            || !record_lanes.contains(&self.records_out_bytes)
            || !midi_lanes.contains(&self.midi_in_bytes)
            || !midi_lanes.contains(&self.midi_out_bytes)
            || self.records_per_quantum == 0
        {
            return Err(ConfigError::Range);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, ConfigError> {
        fn put(out: &mut Vec<u8>, value: usize) -> Result<(), ConfigError> {
            let value = u32::try_from(value).map_err(|_| ConfigError::Range)?;
            out.extend_from_slice(&value.to_le_bytes());
            Ok(())
        }
        let engine = &self.engine;
        let mut out = Vec::new();
        for value in [
            CONFIG_VERSION as usize,
            engine.sample_rate as usize,
            engine.max_frames,
            engine.strip_channels.into(),
            engine.master_channels.into(),
            engine.smoothing_frames as usize,
            engine.smoothing_limit_frames as usize,
            engine.ramp_frames as usize,
            engine.control_lead_frames as usize,
            engine.resource_pages,
            engine.page_bytes,
            engine.midi_events,
            engine.midi_bytes,
            engine.pending_transitions,
            engine.reliable_credits.into(),
            engine.bulk_credits.into(),
            engine.telemetry_interval_frames as usize,
            self.records_in_bytes,
            self.records_out_bytes,
            self.midi_in_bytes,
            self.midi_out_bytes,
            self.records_per_quantum,
            engine.segments.len(),
        ] {
            put(&mut out, value)?;
        }
        for segment in &engine.segments {
            put(&mut out, segment.strips as usize)?;
            put(&mut out, segment.loops as usize)?;
        }
        put(&mut out, engine.master_connections.len())?;
        for &(channel, host) in &engine.master_connections {
            put(&mut out, channel.into())?;
            put(&mut out, host.into())?;
        }
        let host = &engine.host;
        for names in [
            &host.audio_inputs,
            &host.audio_outputs,
            &host.midi_inputs,
            &host.midi_outputs,
        ] {
            put(&mut out, names.len())?;
            for name in names {
                let bytes = name.as_str().as_bytes();
                out.push(bytes.len() as u8);
                out.extend_from_slice(bytes);
            }
        }
        if out.len() > MAX_CONFIG_BYTES {
            return Err(ConfigError::Range);
        }
        Ok(out)
    }

    pub fn decode(record: &[u8]) -> Result<Self, ConfigError> {
        let mut r = Reader(record);
        if r.u32()? != CONFIG_VERSION {
            return Err(ConfigError::Version);
        }
        let sample_rate = r.u32()?;
        let max_frames = r.usize()?;
        let strip_channels = r.narrow()?;
        let master_channels = r.narrow()?;
        let smoothing_frames = r.u32()?;
        let smoothing_limit_frames = r.u32()?;
        let ramp_frames = r.u32()?;
        let control_lead_frames = r.u32()?;
        let resource_pages = r.usize()?;
        let page_bytes = r.usize()?;
        let midi_events = r.usize()?;
        let midi_bytes = r.usize()?;
        let pending_transitions = r.usize()?;
        let reliable_credits = r.narrow()?;
        let bulk_credits = r.narrow()?;
        let telemetry_interval_frames = r.u32()?;
        let records_in_bytes = r.usize()?;
        let records_out_bytes = r.usize()?;
        let midi_in_bytes = r.usize()?;
        let midi_out_bytes = r.usize()?;
        let records_per_quantum = r.usize()?;
        let segments = (0..r.count(MAX_SEGMENTS)?)
            .map(|_| {
                Ok(Segment {
                    strips: r.u32()?,
                    loops: r.u32()?,
                })
            })
            .collect::<Result<_, ConfigError>>()?;
        let master_connections = (0..r.count(MAX_HOST_PORTS)?)
            .map(|_| Ok((r.narrow()?, r.narrow()?)))
            .collect::<Result<_, ConfigError>>()?;
        let mut names = || -> Result<Vec<Name>, ConfigError> {
            (0..r.count(MAX_HOST_PORTS)?).map(|_| r.name()).collect()
        };
        let host = HostPorts {
            audio_inputs: names()?,
            audio_outputs: names()?,
            midi_inputs: names()?,
            midi_outputs: names()?,
        };
        if !r.0.is_empty() {
            return Err(ConfigError::Trailing);
        }
        Ok(Self {
            engine: EngineDescription {
                sample_rate,
                max_frames,
                host,
                segments,
                strip_channels,
                master_channels,
                master_connections,
                smoothing_frames,
                smoothing_limit_frames,
                ramp_frames,
                control_lead_frames,
                resource_pages,
                page_bytes,
                midi_events,
                midi_bytes,
                pending_transitions,
                reliable_credits,
                bulk_credits,
                telemetry_interval_frames,
            },
            records_in_bytes,
            records_out_bytes,
            midi_in_bytes,
            midi_out_bytes,
            records_per_quantum,
        })
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes(&mut self, len: usize) -> Result<&[u8], ConfigError> {
        if self.0.len() < len {
            return Err(ConfigError::Truncated);
        }
        let (bytes, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(bytes)
    }
    fn u32(&mut self) -> Result<u32, ConfigError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("four bytes"),
        ))
    }
    fn usize(&mut self) -> Result<usize, ConfigError> {
        usize::try_from(self.u32()?).map_err(|_| ConfigError::Range)
    }
    /// A `u32` field of a narrower value.
    fn narrow<T: TryFrom<u32>>(&mut self) -> Result<T, ConfigError> {
        T::try_from(self.u32()?).map_err(|_| ConfigError::Range)
    }
    fn count(&mut self, max: usize) -> Result<usize, ConfigError> {
        let count = self.usize()?;
        if count > max {
            return Err(ConfigError::Range);
        }
        Ok(count)
    }
    fn name(&mut self) -> Result<Name, ConfigError> {
        let len = usize::from(self.bytes(1)?[0]);
        let text = std::str::from_utf8(self.bytes(len)?).map_err(|_| ConfigError::Name)?;
        Name::new(text).ok_or(ConfigError::Name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn config() -> WorkletConfig {
        let mut engine = EngineDescription {
            segments: vec![
                Segment {
                    strips: 2,
                    loops: 5,
                },
                Segment {
                    strips: 1,
                    loops: 3,
                },
            ],
            master_connections: vec![(0, 1), (1, 0)],
            control_lead_frames: 3,
            ..EngineDescription::default()
        };
        engine.host.midi_inputs = vec![Name::new("Keystep ✓").unwrap()];
        engine.host.midi_outputs.clear();
        WorkletConfig {
            records_per_quantum: 3,
            ..WorkletConfig::new(engine)
        }
    }

    #[shoop_test]
    fn round_trips_every_field() {
        let config = config();
        let record = config.encode().unwrap();
        assert_eq!(record[..4], CONFIG_VERSION.to_le_bytes());
        assert_eq!(WorkletConfig::decode(&record), Ok(config));
    }

    #[shoop_test]
    fn rejects_malformed_records() {
        let record = config().encode().unwrap();
        assert_eq!(
            WorkletConfig::decode(&record[..record.len() - 1]),
            Err(ConfigError::Truncated)
        );
        let mut trailing = record.clone();
        trailing.push(0);
        assert_eq!(WorkletConfig::decode(&trailing), Err(ConfigError::Trailing));
        let mut version = record.clone();
        version[0] = 9;
        assert_eq!(WorkletConfig::decode(&version), Err(ConfigError::Version));
        let mut channels = record.clone();
        channels[12..16].copy_from_slice(&300u32.to_le_bytes());
        assert_eq!(WorkletConfig::decode(&channels), Err(ConfigError::Range));
        let name = record.len() - "Keystep ✓".len() - 4;
        let mut invalid = record;
        invalid[name] = 0xff;
        assert_eq!(WorkletConfig::decode(&invalid), Err(ConfigError::Name));
    }

    #[shoop_test]
    fn bounds_the_worklet_lanes() {
        assert_eq!(config().validate(), Ok(()));
        for broken in [
            WorkletConfig {
                records_in_bytes: MAX_RECORD_BYTES - 1,
                ..config()
            },
            WorkletConfig {
                midi_out_bytes: MAX_EVENT_BYTES - 1,
                ..config()
            },
            WorkletConfig {
                records_per_quantum: 0,
                ..config()
            },
        ] {
            assert_eq!(broken.validate(), Err(ConfigError::Range));
        }
    }
}
