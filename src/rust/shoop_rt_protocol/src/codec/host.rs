use super::{decode, encode, CodecError, Reader, Wire, Writer};
use crate::host::*;
use crate::{DriverKind, DriverStatus};
impl Wire for Text {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u16(self.as_str().len() as u16)?;
        w.bytes(self.as_str().as_bytes())
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let len = r.u16()? as usize;
        let value = core::str::from_utf8(r.bytes(len)?).map_err(|_| CodecError::Invalid)?;
        Self::new(value).ok_or(CodecError::Invalid)
    }
}
impl Wire for DiscoverKind {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u8(*self as u8)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Hosts,
            1 => Self::AudioInputs,
            2 => Self::AudioOutputs,
            3 => Self::MidiInputs,
            4 => Self::MidiOutputs,
            _ => return Err(CodecError::Invalid),
        })
    }
}
impl Wire for DeviceSelection {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Default => w.u8(0),
            Self::None => w.u8(1),
            Self::Named(name) => {
                w.u8(2)?;
                name.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Default,
            1 => Self::None,
            2 => Self::Named(Text::get(r)?),
            _ => return Err(CodecError::Invalid),
        })
    }
}
impl Wire for EngineSettings {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(self.max_frames)?;
        w.u32(self.strip_channels)?;
        w.u32(self.master_channels)?;
        w.u32(self.buses)?;
        w.u32(self.bus_channels)?;
        w.u32(self.smoothing_frames)?;
        w.u32(self.smoothing_limit_frames)?;
        w.u32(self.ramp_frames)?;
        w.u32(self.control_lead_frames)?;
        w.u32(self.resource_pages)?;
        w.u32(self.page_bytes)?;
        w.u32(self.midi_events)?;
        w.u32(self.midi_bytes)?;
        w.u32(self.pending_transitions)?;
        w.u32(self.reliable_credits)?;
        w.u32(self.bulk_credits)?;
        w.u32(self.telemetry_interval_frames)?;
        Ok(())
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            max_frames: r.u32()?,
            strip_channels: r.u32()?,
            master_channels: r.u32()?,
            buses: r.u32()?,
            bus_channels: r.u32()?,
            smoothing_frames: r.u32()?,
            smoothing_limit_frames: r.u32()?,
            ramp_frames: r.u32()?,
            control_lead_frames: r.u32()?,
            resource_pages: r.u32()?,
            page_bytes: r.u32()?,
            midi_events: r.u32()?,
            midi_bytes: r.u32()?,
            pending_transitions: r.u32()?,
            reliable_credits: r.u32()?,
            bulk_credits: r.u32()?,
            telemetry_interval_frames: r.u32()?,
        })
    }
}
impl Wire for DriverSettings {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.kind.put(w)?;
        self.host.put(w)?;
        self.input.put(w)?;
        self.output.put(w)?;
        w.u32(self.sample_rate)?;
        w.u32(self.buffer_frames)?;
        w.u16(self.audio_inputs)?;
        w.u16(self.audio_outputs)?;
        w.u16(self.midi_inputs)?;
        w.u16(self.midi_outputs)?;
        self.client_name.put(w)?;
        w.bool(self.dummy_input_ramp)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            kind: DriverKind::get(r)?,
            host: Text::get(r)?,
            input: DeviceSelection::get(r)?,
            output: DeviceSelection::get(r)?,
            sample_rate: r.u32()?,
            buffer_frames: r.u32()?,
            audio_inputs: r.u16()?,
            audio_outputs: r.u16()?,
            midi_inputs: r.u16()?,
            midi_outputs: r.u16()?,
            client_name: Text::get(r)?,
            dummy_input_ramp: r.bool()?,
        })
    }
}
impl Wire for HostRequest {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u8(0xf0)?;
        match self {
            Self::Hello { version } => {
                w.u8(0)?;
                w.u16(*version)
            }
            Self::Discover { kind, index, host } => {
                w.u8(1)?;
                kind.put(w)?;
                w.u16(*index)?;
                host.put(w)
            }
            Self::ConfigureEngine(settings) => {
                w.u8(2)?;
                settings.put(w)
            }
            Self::AddSegment { strips, loops } => {
                w.u8(3)?;
                w.u32(*strips)?;
                w.u32(*loops)
            }
            Self::ConfigureDriver(settings) => {
                w.u8(4)?;
                settings.put(w)
            }
            Self::AddMidiInput(name) => {
                w.u8(5)?;
                name.put(w)
            }
            Self::AddMidiOutput(name) => {
                w.u8(6)?;
                name.put(w)
            }
            Self::Start => w.u8(7),
            Self::Stop => w.u8(8),
            Self::Status => w.u8(9),
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        if r.u8()? != 0xf0 {
            return Err(CodecError::Invalid);
        }
        Ok(match r.u8()? {
            0 => Self::Hello { version: r.u16()? },
            1 => Self::Discover {
                kind: DiscoverKind::get(r)?,
                index: r.u16()?,
                host: Text::get(r)?,
            },
            2 => Self::ConfigureEngine(EngineSettings::get(r)?),
            3 => Self::AddSegment {
                strips: r.u32()?,
                loops: r.u32()?,
            },
            4 => Self::ConfigureDriver(DriverSettings::get(r)?),
            5 => Self::AddMidiInput(Text::get(r)?),
            6 => Self::AddMidiOutput(Text::get(r)?),
            7 => Self::Start,
            8 => Self::Stop,
            9 => Self::Status,
            _ => return Err(CodecError::Invalid),
        })
    }
}
impl Wire for HostReply {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u8(0xf1)?;
        match self {
            Self::Hello { version } => {
                w.u8(0)?;
                w.u16(*version)
            }
            Self::Ok => w.u8(1),
            Self::Device {
                kind,
                index,
                name,
                default,
                channels,
                sample_rate,
                min_buffer,
                max_buffer,
            } => {
                w.u8(2)?;
                kind.put(w)?;
                w.u16(*index)?;
                name.put(w)?;
                w.bool(*default)?;
                w.u16(*channels)?;
                w.u32(*sample_rate)?;
                w.u32(*min_buffer)?;
                w.u32(*max_buffer)
            }
            Self::End => w.u8(3),
            Self::InstanceName(name) => {
                w.u8(9)?;
                name.put(w)
            }
            Self::Ready(driver) => {
                w.u8(4)?;
                driver.put(w)
            }
            Self::Warning(text) => {
                w.u8(5)?;
                text.put(w)
            }
            Self::Error(text) => {
                w.u8(6)?;
                text.put(w)
            }
            Self::Stopped { midi_reset } => {
                w.u8(7)?;
                w.bool(*midi_reset)
            }
            Self::Status { frame, callbacks } => {
                w.u8(8)?;
                w.u64(*frame)?;
                w.u64(*callbacks)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        if r.u8()? != 0xf1 {
            return Err(CodecError::Invalid);
        }
        Ok(match r.u8()? {
            0 => Self::Hello { version: r.u16()? },
            1 => Self::Ok,
            2 => Self::Device {
                kind: DiscoverKind::get(r)?,
                index: r.u16()?,
                name: Text::get(r)?,
                default: r.bool()?,
                channels: r.u16()?,
                sample_rate: r.u32()?,
                min_buffer: r.u32()?,
                max_buffer: r.u32()?,
            },
            3 => Self::End,
            4 => Self::Ready(DriverStatus::get(r)?),
            9 => Self::InstanceName(Text::get(r)?),
            5 => Self::Warning(Text::get(r)?),
            6 => Self::Error(Text::get(r)?),
            7 => Self::Stopped {
                midi_reset: r.bool()?,
            },
            8 => Self::Status {
                frame: r.u64()?,
                callbacks: r.u64()?,
            },
            _ => return Err(CodecError::Invalid),
        })
    }
}
pub fn encode_host_request(message: &HostRequest, buf: &mut [u8]) -> Result<usize, CodecError> {
    encode(message, buf)
}
pub fn decode_host_request(record: &[u8]) -> Result<HostRequest, CodecError> {
    decode(record)
}
pub fn encode_host_reply(message: &HostReply, buf: &mut [u8]) -> Result<usize, CodecError> {
    encode(message, buf)
}
pub fn decode_host_reply(record: &[u8]) -> Result<HostReply, CodecError> {
    decode(record)
}
