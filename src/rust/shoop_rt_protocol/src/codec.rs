//! Bounded binary records.
//!
//! A record is a little-endian `u16` total length followed by one message. Encoding
//! writes into a caller-provided buffer of at most [`MAX_RECORD_BYTES`]; decoding
//! reads a borrowed slice. Neither allocates. Decoding checks every tag, length and
//! name and rejects trailing bytes.

use crate::ids::{BusId, HostPortId, LoopSlotId, SlotId, StripId};
use crate::message::{ClientMessage, DisconnectReason, EngineMessage};
use crate::schema::*;
use crate::{
    BULK_CHUNK_BYTES, BULK_CHUNK_SAMPLES, MAX_MIDI_BYTES, MAX_NAME_BYTES, MAX_RECORD_BYTES,
    MAX_STRIP_CHANNELS,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    /// The output buffer is too small.
    Overflow,
    /// The record ended early.
    Truncated,
    /// A tag, length or value is invalid.
    Invalid,
    /// Bytes remain after the message.
    Trailing,
}

struct Writer<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl Writer<'_> {
    fn bytes(&mut self, data: &[u8]) -> Result<(), CodecError> {
        let end = self.pos + data.len();
        if end > self.buf.len() {
            return Err(CodecError::Overflow);
        }
        self.buf[self.pos..end].copy_from_slice(data);
        self.pos = end;
        Ok(())
    }
    fn u8(&mut self, value: u8) -> Result<(), CodecError> {
        self.bytes(&[value])
    }
    fn u16(&mut self, value: u16) -> Result<(), CodecError> {
        self.bytes(&value.to_le_bytes())
    }
    fn u32(&mut self, value: u32) -> Result<(), CodecError> {
        self.bytes(&value.to_le_bytes())
    }
    fn u64(&mut self, value: u64) -> Result<(), CodecError> {
        self.bytes(&value.to_le_bytes())
    }
    fn f32(&mut self, value: f32) -> Result<(), CodecError> {
        self.bytes(&value.to_le_bytes())
    }
    fn bool(&mut self, value: bool) -> Result<(), CodecError> {
        self.u8(value as u8)
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, len: usize) -> Result<&'a [u8], CodecError> {
        let end = self.pos + len;
        if end > self.buf.len() {
            return Err(CodecError::Truncated);
        }
        let data = &self.buf[self.pos..end];
        self.pos = end;
        Ok(data)
    }
    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, CodecError> {
        Ok(u16::from_le_bytes(
            self.bytes(2)?.try_into().expect("two bytes"),
        ))
    }
    fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("four bytes"),
        ))
    }
    fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_le_bytes(
            self.bytes(8)?.try_into().expect("eight bytes"),
        ))
    }
    fn f32(&mut self) -> Result<f32, CodecError> {
        Ok(f32::from_le_bytes(
            self.bytes(4)?.try_into().expect("four bytes"),
        ))
    }
    fn bool(&mut self) -> Result<bool, CodecError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CodecError::Invalid),
        }
    }
}

trait Wire: Sized {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError>;
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError>;
}

macro_rules! tags {
    ($ty:ty { $($tag:literal => $variant:path),* $(,)? }) => {
        impl Wire for $ty {
            fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
                w.u8(match self { $($variant => $tag,)* })
            }
            fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
                match r.u8()? { $($tag => Ok($variant),)* _ => Err(CodecError::Invalid) }
            }
        }
    };
}

tags!(LoopMode { 0 => LoopMode::Stopped, 1 => LoopMode::Playing, 2 => LoopMode::Recording });
tags!(PortKind { 0 => PortKind::AudioIn, 1 => PortKind::AudioOut, 2 => PortKind::MidiIn, 3 => PortKind::MidiOut });
tags!(Refusal {
    0 => Refusal::Unsupported, 1 => Refusal::Capacity, 2 => Refusal::InvalidId, 3 => Refusal::InvalidValue,
    4 => Refusal::Busy, 5 => Refusal::Overloaded, 6 => Refusal::NotConnected, 7 => Refusal::Internal,
});
tags!(DriverKind { 0 => DriverKind::Dummy, 1 => DriverKind::Cpal, 2 => DriverKind::WebAudio });
tags!(DriverRunState {
    0 => DriverRunState::Starting, 1 => DriverRunState::Running, 2 => DriverRunState::Stopped, 3 => DriverRunState::Failed,
});
tags!(HostPortKind { 0 => HostPortKind::Audio, 1 => HostPortKind::Midi });
tags!(Direction { 0 => Direction::Input, 1 => Direction::Output });
tags!(BusId { 0 => BusId::Master });
tags!(DisconnectReason { 0 => DisconnectReason::Shutdown, 1 => DisconnectReason::Fault, 2 => DisconnectReason::ProtocolError });

impl Wire for SlotId {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(self.index)?;
        w.u32(self.generation)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self::new(r.u32()?, r.u32()?))
    }
}

impl Wire for StripId {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.0.put(w)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self(SlotId::get(r)?))
    }
}

impl Wire for LoopSlotId {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.0.put(w)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self(SlotId::get(r)?))
    }
}

impl Wire for HostPortId {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(self.0)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self(r.u32()?))
    }
}

impl<T: Wire> Wire for Option<T> {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            None => w.u8(0),
            Some(value) => {
                w.u8(1)?;
                value.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        match r.u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::get(r)?)),
            _ => Err(CodecError::Invalid),
        }
    }
}

impl Wire for u32 {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(*self)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        r.u32()
    }
}

impl Wire for Name {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        let bytes = self.as_str().as_bytes();
        w.u8(bytes.len() as u8)?;
        w.bytes(bytes)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let len = r.u8()? as usize;
        if len > MAX_NAME_BYTES {
            return Err(CodecError::Invalid);
        }
        let text = std::str::from_utf8(r.bytes(len)?).map_err(|_| CodecError::Invalid)?;
        Name::new(text).ok_or(CodecError::Invalid)
    }
}

impl Wire for MidiMessage {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u8(self.as_bytes().len() as u8)?;
        w.bytes(self.as_bytes())
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let len = r.u8()? as usize;
        if len == 0 || len > MAX_MIDI_BYTES {
            return Err(CodecError::Invalid);
        }
        MidiMessage::new(r.bytes(len)?).ok_or(CodecError::Invalid)
    }
}

impl Wire for Peaks {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u8(self.as_slice().len() as u8)?;
        self.as_slice().iter().try_for_each(|value| w.f32(*value))
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let len = r.u8()? as usize;
        if len > MAX_STRIP_CHANNELS {
            return Err(CodecError::Invalid);
        }
        let mut values = [0.; MAX_STRIP_CHANNELS];
        for value in &mut values[..len] {
            *value = r.f32()?;
        }
        Peaks::new(&values[..len]).ok_or(CodecError::Invalid)
    }
}

impl Wire for StripControl {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match *self {
            Self::InputGain(v) => {
                w.u8(0)?;
                w.f32(v)
            }
            Self::InputBalance(v) => {
                w.u8(1)?;
                w.f32(v)
            }
            Self::Monitoring(v) => {
                w.u8(2)?;
                w.bool(v)
            }
            Self::OutputGain(v) => {
                w.u8(3)?;
                w.f32(v)
            }
            Self::OutputBalance(v) => {
                w.u8(4)?;
                w.f32(v)
            }
            Self::OutputMute(v) => {
                w.u8(5)?;
                w.bool(v)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::InputGain(r.f32()?),
            1 => Self::InputBalance(r.f32()?),
            2 => Self::Monitoring(r.bool()?),
            3 => Self::OutputGain(r.f32()?),
            4 => Self::OutputBalance(r.f32()?),
            5 => Self::OutputMute(r.bool()?),
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for BusControl {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match *self {
            Self::Gain(v) => {
                w.u8(0)?;
                w.f32(v)
            }
            Self::Balance(v) => {
                w.u8(1)?;
                w.f32(v)
            }
            Self::Mute(v) => {
                w.u8(2)?;
                w.bool(v)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Gain(r.f32()?),
            1 => Self::Balance(r.f32()?),
            2 => Self::Mute(r.bool()?),
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for StripPort {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.strip.put(w)?;
        self.kind.put(w)?;
        w.u8(self.index)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            strip: StripId::get(r)?,
            kind: PortKind::get(r)?,
            index: r.u8()?,
        })
    }
}

impl Wire for StripShape {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u8(self.audio_inputs)?;
        w.u8(self.audio_outputs)?;
        w.bool(self.midi_input)?;
        w.bool(self.midi_output)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            audio_inputs: r.u8()?,
            audio_outputs: r.u8()?,
            midi_input: r.bool()?,
            midi_output: r.bool()?,
        })
    }
}

impl Wire for Request {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match *self {
            Self::CreateStrip { strip, shape } => {
                w.u8(0)?;
                strip.put(w)?;
                shape.put(w)
            }
            Self::RemoveStrip { strip } => {
                w.u8(1)?;
                strip.put(w)
            }
            Self::StripControl { strip, control } => {
                w.u8(2)?;
                strip.put(w)?;
                control.put(w)
            }
            Self::CreateLoop { loop_id, strip } => {
                w.u8(3)?;
                loop_id.put(w)?;
                strip.put(w)
            }
            Self::RemoveLoop { loop_id } => {
                w.u8(4)?;
                loop_id.put(w)
            }
            Self::LoopSync { loop_id, source } => {
                w.u8(5)?;
                loop_id.put(w)?;
                source.put(w)
            }
            Self::LoopTransition {
                loop_id,
                mode,
                delay,
            } => {
                w.u8(6)?;
                loop_id.put(w)?;
                mode.put(w)?;
                delay.put(w)
            }
            Self::LoopClear { loop_id } => {
                w.u8(7)?;
                loop_id.put(w)
            }
            Self::LoopGain { loop_id, gain } => {
                w.u8(8)?;
                loop_id.put(w)?;
                w.f32(gain)
            }
            Self::LoopBalance { loop_id, balance } => {
                w.u8(9)?;
                loop_id.put(w)?;
                w.f32(balance)
            }
            Self::Connect {
                port,
                host,
                connected,
            } => {
                w.u8(10)?;
                port.put(w)?;
                host.put(w)?;
                w.bool(connected)
            }
            Self::Route {
                strip,
                bus,
                connected,
            } => {
                w.u8(11)?;
                strip.put(w)?;
                bus.put(w)?;
                w.bool(connected)
            }
            Self::BusControl { bus, control } => {
                w.u8(12)?;
                bus.put(w)?;
                control.put(w)
            }
            Self::InjectMidi { strip, message } => {
                w.u8(13)?;
                strip.put(w)?;
                message.put(w)
            }
            Self::SetSmoothing { frames } => {
                w.u8(14)?;
                w.u32(frames)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::CreateStrip {
                strip: StripId::get(r)?,
                shape: StripShape::get(r)?,
            },
            1 => Self::RemoveStrip {
                strip: StripId::get(r)?,
            },
            2 => Self::StripControl {
                strip: StripId::get(r)?,
                control: StripControl::get(r)?,
            },
            3 => Self::CreateLoop {
                loop_id: LoopSlotId::get(r)?,
                strip: StripId::get(r)?,
            },
            4 => Self::RemoveLoop {
                loop_id: LoopSlotId::get(r)?,
            },
            5 => Self::LoopSync {
                loop_id: LoopSlotId::get(r)?,
                source: Option::get(r)?,
            },
            6 => Self::LoopTransition {
                loop_id: LoopSlotId::get(r)?,
                mode: LoopMode::get(r)?,
                delay: Option::get(r)?,
            },
            7 => Self::LoopClear {
                loop_id: LoopSlotId::get(r)?,
            },
            8 => Self::LoopGain {
                loop_id: LoopSlotId::get(r)?,
                gain: r.f32()?,
            },
            9 => Self::LoopBalance {
                loop_id: LoopSlotId::get(r)?,
                balance: r.f32()?,
            },
            10 => Self::Connect {
                port: StripPort::get(r)?,
                host: HostPortId::get(r)?,
                connected: r.bool()?,
            },
            11 => Self::Route {
                strip: StripId::get(r)?,
                bus: BusId::get(r)?,
                connected: r.bool()?,
            },
            12 => Self::BusControl {
                bus: BusId::get(r)?,
                control: BusControl::get(r)?,
            },
            13 => Self::InjectMidi {
                strip: StripId::get(r)?,
                message: MidiMessage::get(r)?,
            },
            14 => Self::SetSmoothing { frames: r.u32()? },
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for Outcome {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Done => w.u8(0),
            Self::Refused(refusal) => {
                w.u8(1)?;
                refusal.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Done,
            1 => Self::Refused(Refusal::get(r)?),
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for DriverStatus {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.kind.put(w)?;
        self.state.put(w)?;
        w.u32(self.sample_rate)?;
        w.u32(self.buffer_frames)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            kind: DriverKind::get(r)?,
            state: DriverRunState::get(r)?,
            sample_rate: r.u32()?,
            buffer_frames: r.u32()?,
        })
    }
}

impl Wire for HostPort {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.put(w)?;
        self.kind.put(w)?;
        self.direction.put(w)?;
        self.name.put(w)?;
        w.bool(self.preferred)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            id: HostPortId::get(r)?,
            kind: HostPortKind::get(r)?,
            direction: Direction::get(r)?,
            name: Name::get(r)?,
            preferred: r.bool()?,
        })
    }
}

impl Wire for Limits {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(self.strips)?;
        w.u32(self.loops)?;
        w.u8(self.strip_audio_channels)?;
        w.u16(self.host_audio_inputs)?;
        w.u16(self.host_audio_outputs)?;
        w.u16(self.host_midi_inputs)?;
        w.u16(self.host_midi_outputs)?;
        w.u32(self.max_frames)?;
        w.u16(self.reliable_credits)?;
        w.u16(self.bulk_credits)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            strips: r.u32()?,
            loops: r.u32()?,
            strip_audio_channels: r.u8()?,
            host_audio_inputs: r.u16()?,
            host_audio_outputs: r.u16()?,
            host_midi_inputs: r.u16()?,
            host_midi_outputs: r.u16()?,
            max_frames: r.u32()?,
            reliable_credits: r.u16()?,
            bulk_credits: r.u16()?,
        })
    }
}

impl Wire for Telemetry {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Engine {
                frame,
                xruns,
                load,
                driver,
            } => {
                w.u8(0)?;
                w.u64(*frame)?;
                w.u32(*xruns)?;
                w.f32(*load)?;
                driver.put(w)
            }
            Self::Strip {
                strip,
                input_peaks,
                output_peaks,
                midi_in,
                midi_out,
            } => {
                w.u8(1)?;
                strip.put(w)?;
                input_peaks.put(w)?;
                output_peaks.put(w)?;
                w.bool(*midi_in)?;
                w.bool(*midi_out)
            }
            Self::Loop {
                loop_id,
                mode,
                next_mode,
                next_delay,
                length,
                position,
                peaks,
            } => {
                w.u8(2)?;
                loop_id.put(w)?;
                mode.put(w)?;
                next_mode.put(w)?;
                next_delay.put(w)?;
                w.u64(*length)?;
                w.u64(*position)?;
                peaks.put(w)
            }
            Self::Bus { bus, peaks } => {
                w.u8(3)?;
                bus.put(w)?;
                peaks.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Engine {
                frame: r.u64()?,
                xruns: r.u32()?,
                load: r.f32()?,
                driver: DriverStatus::get(r)?,
            },
            1 => Self::Strip {
                strip: StripId::get(r)?,
                input_peaks: Peaks::get(r)?,
                output_peaks: Peaks::get(r)?,
                midi_in: r.bool()?,
                midi_out: r.bool()?,
            },
            2 => Self::Loop {
                loop_id: LoopSlotId::get(r)?,
                mode: LoopMode::get(r)?,
                next_mode: Option::get(r)?,
                next_delay: Option::get(r)?,
                length: r.u64()?,
                position: r.u64()?,
                peaks: Peaks::get(r)?,
            },
            3 => Self::Bus {
                bus: BusId::get(r)?,
                peaks: Peaks::get(r)?,
            },
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for StateRecord {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Begin { epoch } => {
                w.u8(0)?;
                w.u64(*epoch)
            }
            Self::HostPort(port) => {
                w.u8(1)?;
                port.put(w)
            }
            Self::Strip { strip, shape } => {
                w.u8(2)?;
                strip.put(w)?;
                shape.put(w)
            }
            Self::StripControl { strip, control } => {
                w.u8(3)?;
                strip.put(w)?;
                control.put(w)
            }
            Self::Loop {
                loop_id,
                strip,
                sync,
                gain,
                balance,
            } => {
                w.u8(4)?;
                loop_id.put(w)?;
                strip.put(w)?;
                sync.put(w)?;
                w.f32(*gain)?;
                w.f32(*balance)
            }
            Self::Connection { port, host } => {
                w.u8(5)?;
                port.put(w)?;
                host.put(w)
            }
            Self::Route { strip, bus } => {
                w.u8(6)?;
                strip.put(w)?;
                bus.put(w)
            }
            Self::Bus { bus, control } => {
                w.u8(7)?;
                bus.put(w)?;
                control.put(w)
            }
            Self::End { epoch } => {
                w.u8(8)?;
                w.u64(*epoch)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Begin { epoch: r.u64()? },
            1 => Self::HostPort(HostPort::get(r)?),
            2 => Self::Strip {
                strip: StripId::get(r)?,
                shape: StripShape::get(r)?,
            },
            3 => Self::StripControl {
                strip: StripId::get(r)?,
                control: StripControl::get(r)?,
            },
            4 => Self::Loop {
                loop_id: LoopSlotId::get(r)?,
                strip: StripId::get(r)?,
                sync: Option::get(r)?,
                gain: r.f32()?,
                balance: r.f32()?,
            },
            5 => Self::Connection {
                port: StripPort::get(r)?,
                host: HostPortId::get(r)?,
            },
            6 => Self::Route {
                strip: StripId::get(r)?,
                bus: BusId::get(r)?,
            },
            7 => Self::Bus {
                bus: BusId::get(r)?,
                control: BusControl::get(r)?,
            },
            8 => Self::End { epoch: r.u64()? },
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for BulkSelector {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Audio { channel } => {
                w.u8(0)?;
                w.u8(*channel)
            }
            Self::Midi => w.u8(1),
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Audio { channel: r.u8()? },
            1 => Self::Midi,
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for BulkRead {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(self.id)?;
        self.loop_id.put(w)?;
        self.selector.put(w)?;
        w.u64(self.offset)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            id: r.u32()?,
            loop_id: LoopSlotId::get(r)?,
            selector: BulkSelector::get(r)?,
            offset: r.u64()?,
        })
    }
}

impl Wire for BulkChunk {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(self.id)?;
        w.u64(self.offset)?;
        w.u64(self.total)?;
        match &self.payload {
            BulkPayload::Audio { len, samples } => {
                w.u8(0)?;
                w.u16(*len)?;
                samples[..*len as usize]
                    .iter()
                    .try_for_each(|sample| w.f32(*sample))
            }
            BulkPayload::Midi { len, bytes } => {
                w.u8(1)?;
                w.u16(*len)?;
                w.bytes(&bytes[..*len as usize])
            }
            BulkPayload::Refused(refusal) => {
                w.u8(2)?;
                refusal.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = r.u32()?;
        let offset = r.u64()?;
        let total = r.u64()?;
        let payload = match r.u8()? {
            0 => {
                let len = r.u16()? as usize;
                if len > BULK_CHUNK_SAMPLES {
                    return Err(CodecError::Invalid);
                }
                let mut samples = [0.; BULK_CHUNK_SAMPLES];
                for sample in &mut samples[..len] {
                    *sample = r.f32()?;
                }
                BulkPayload::Audio {
                    len: len as u16,
                    samples,
                }
            }
            1 => {
                let len = r.u16()? as usize;
                if len > BULK_CHUNK_BYTES {
                    return Err(CodecError::Invalid);
                }
                let mut bytes = [0; BULK_CHUNK_BYTES];
                bytes[..len].copy_from_slice(r.bytes(len)?);
                BulkPayload::Midi {
                    len: len as u16,
                    bytes,
                }
            }
            2 => BulkPayload::Refused(Refusal::get(r)?),
            _ => return Err(CodecError::Invalid),
        };
        Ok(Self {
            id,
            offset,
            total,
            payload,
        })
    }
}

impl Wire for ClientMessage {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Hello { version } => {
                w.u8(0)?;
                w.u16(*version)
            }
            Self::Request {
                correlation,
                request,
            } => {
                w.u8(1)?;
                w.u64(*correlation)?;
                request.put(w)
            }
            Self::Resync => w.u8(2),
            Self::Bulk(read) => {
                w.u8(3)?;
                read.put(w)
            }
            Self::Goodbye => w.u8(4),
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Hello { version: r.u16()? },
            1 => Self::Request {
                correlation: r.u64()?,
                request: Request::get(r)?,
            },
            2 => Self::Resync,
            3 => Self::Bulk(BulkRead::get(r)?),
            4 => Self::Goodbye,
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for EngineMessage {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Welcome {
                version,
                capabilities,
                limits,
                driver,
            } => {
                w.u8(0)?;
                w.u16(*version)?;
                w.u32(capabilities.0)?;
                limits.put(w)?;
                driver.put(w)
            }
            Self::Rejected { version } => {
                w.u8(1)?;
                w.u16(*version)
            }
            Self::Result {
                correlation,
                outcome,
            } => {
                w.u8(2)?;
                w.u64(*correlation)?;
                outcome.put(w)
            }
            Self::Telemetry(telemetry) => {
                w.u8(3)?;
                telemetry.put(w)
            }
            Self::State(record) => {
                w.u8(4)?;
                record.put(w)
            }
            Self::Bulk(chunk) => {
                w.u8(5)?;
                chunk.put(w)
            }
            Self::LimitsChanged(limits) => {
                w.u8(6)?;
                limits.put(w)
            }
            Self::Disconnected(reason) => {
                w.u8(7)?;
                reason.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Welcome {
                version: r.u16()?,
                capabilities: Capabilities(r.u32()?),
                limits: Limits::get(r)?,
                driver: DriverStatus::get(r)?,
            },
            1 => Self::Rejected { version: r.u16()? },
            2 => Self::Result {
                correlation: r.u64()?,
                outcome: Outcome::get(r)?,
            },
            3 => Self::Telemetry(Telemetry::get(r)?),
            4 => Self::State(StateRecord::get(r)?),
            5 => Self::Bulk(BulkChunk::get(r)?),
            6 => Self::LimitsChanged(Limits::get(r)?),
            7 => Self::Disconnected(DisconnectReason::get(r)?),
            _ => return Err(CodecError::Invalid),
        })
    }
}

fn encode<T: Wire>(message: &T, buf: &mut [u8]) -> Result<usize, CodecError> {
    let limit = buf.len().min(MAX_RECORD_BYTES);
    let mut w = Writer {
        buf: &mut buf[..limit],
        pos: 2,
    };
    if limit < 2 {
        return Err(CodecError::Overflow);
    }
    message.put(&mut w)?;
    let len = w.pos;
    w.buf[..2].copy_from_slice(&(len as u16).to_le_bytes());
    Ok(len)
}

fn decode<T: Wire>(record: &[u8]) -> Result<T, CodecError> {
    if record.len() < 2 {
        return Err(CodecError::Truncated);
    }
    let len = u16::from_le_bytes([record[0], record[1]]) as usize;
    if len > MAX_RECORD_BYTES || len < 2 {
        return Err(CodecError::Invalid);
    }
    if record.len() < len {
        return Err(CodecError::Truncated);
    }
    if record.len() > len {
        return Err(CodecError::Trailing);
    }
    let mut r = Reader {
        buf: record,
        pos: 2,
    };
    let message = T::get(&mut r)?;
    if r.pos != len {
        return Err(CodecError::Trailing);
    }
    Ok(message)
}

/// Encode a client message; returns the record length.
pub fn encode_client(message: &ClientMessage, buf: &mut [u8]) -> Result<usize, CodecError> {
    encode(message, buf)
}

/// Encode an engine message; returns the record length.
pub fn encode_engine(message: &EngineMessage, buf: &mut [u8]) -> Result<usize, CodecError> {
    encode(message, buf)
}

pub fn decode_client(record: &[u8]) -> Result<ClientMessage, CodecError> {
    decode(record)
}

pub fn decode_engine(record: &[u8]) -> Result<EngineMessage, CodecError> {
    decode(record)
}
