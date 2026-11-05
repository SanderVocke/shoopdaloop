//! Bounded binary records.
//!
//! A record is a little-endian `u16` total length followed by one message. Encoding
//! writes into a caller-provided buffer of at most [`MAX_RECORD_BYTES`]; decoding
//! reads a borrowed slice. Neither allocates. Decoding checks every tag, length and
//! name and rejects trailing bytes.

use crate::ids::{BusId, HostPortId, LoopSlotId, SlotId, StripId};
use crate::message::{ClientMessage, DisconnectReason, EngineMessage};
use crate::schema::*;
use crate::timing::*;
use crate::{
    BULK_CHUNK_BYTES, BULK_CHUNK_SAMPLES, MAX_MIDI_BYTES, MAX_NAME_BYTES, MAX_RECORD_BYTES,
    MAX_STRIP_CHANNELS,
};

pub use shoop_transport::CodecError;

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

tags!(LoopMode { 0 => LoopMode::Stopped, 1 => LoopMode::Playing, 2 => LoopMode::Recording, 3 => LoopMode::Replacing });
tags!(PortKind { 0 => PortKind::AudioIn, 1 => PortKind::AudioOut, 2 => PortKind::MidiIn, 3 => PortKind::MidiOut });
tags!(Refusal {
    0 => Refusal::Unsupported, 1 => Refusal::Capacity, 2 => Refusal::InvalidId, 3 => Refusal::InvalidValue,
    4 => Refusal::Busy, 5 => Refusal::Overloaded, 6 => Refusal::NotConnected, 7 => Refusal::Internal,
});
tags!(DriverKind { 0 => DriverKind::Dummy, 1 => DriverKind::Cpal, 2 => DriverKind::WebAudio, 3 => DriverKind::Jack });
tags!(DriverRunState {
    0 => DriverRunState::Starting, 1 => DriverRunState::Running, 2 => DriverRunState::Stopped, 3 => DriverRunState::Failed,
});
tags!(HostPortKind { 0 => HostPortKind::Audio, 1 => HostPortKind::Midi });
tags!(Direction { 0 => Direction::Input, 1 => Direction::Output });
impl Wire for BusId {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.0.put(w)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self(SlotId::get(r)?))
    }
}
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

impl Wire for i32 {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(*self as u32)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(r.u32()? as i32)
    }
}
impl Wire for ChannelWindow {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u32(self.gain_bits)?;
        self.start_offset.put(w)?;
        self.capture_alignment.put(w)?;
        w.u32(self.preplay)?;
        self.raw_frames.put(w)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            gain_bits: r.u32()?,
            start_offset: i32::get(r)?,
            capture_alignment: i32::get(r)?,
            preplay: r.u32()?,
            raw_frames: Option::get(r)?,
        })
    }
}
impl Wire for PlaybackWindows {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        for channel in &self.audio {
            channel.put(w)?;
        }
        self.midi.put(w)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut value = Self::default();
        for channel in &mut value.audio {
            *channel = ChannelWindow::get(r)?;
        }
        value.midi = ChannelWindow::get(r)?;
        Ok(value)
    }
}
impl Wire for TimingEdit {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        if !self.canonical() {
            return Err(CodecError::Invalid);
        }
        self.length.put(w)?;
        self.start_offset.put(w)?;
        self.preplay.put(w)?;
        w.u8(self.count)?;
        for channel in &self.channels[..usize::from(self.count)] {
            w.u8(channel.channel)?;
            w.bool(channel.midi)?;
            channel.start_offset.put(w)?;
            channel.preplay.put(w)?;
        }
        Ok(())
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut value = Self {
            length: Option::get(r)?,
            start_offset: Option::get(r)?,
            preplay: Option::get(r)?,
            count: r.u8()?,
            ..Self::default()
        };
        if usize::from(value.count) > MAX_TIMING_UPDATES {
            return Err(CodecError::Invalid);
        }
        for channel in &mut value.channels[..usize::from(value.count)] {
            *channel = ChannelTimingUpdate {
                channel: r.u8()?,
                midi: r.bool()?,
                start_offset: Option::get(r)?,
                preplay: Option::get(r)?,
            };
        }
        if !value.canonical() {
            return Err(CodecError::Invalid);
        }
        Ok(value)
    }
}
impl Wire for TimingState {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u64(self.revision)?;
        w.u64(self.content_revision)?;
        w.u64(self.length)?;
        w.u64(self.raw_audio_frames)?;
        w.u64(self.raw_midi_frames)?;
        self.windows.put(w)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            revision: r.u64()?,
            content_revision: r.u64()?,
            length: r.u64()?,
            raw_audio_frames: r.u64()?,
            raw_midi_frames: r.u64()?,
            windows: PlaybackWindows::get(r)?,
        })
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

impl Wire for u64 {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u64(*self)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        r.u64()
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
        let text = core::str::from_utf8(r.bytes(len)?).map_err(|_| CodecError::Invalid)?;
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

impl Wire for BusPort {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.bus.put(w)?;
        w.u8(self.index)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            bus: BusId::get(r)?,
            index: r.u8()?,
        })
    }
}

impl Wire for EnginePort {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Strip(port) => {
                w.u8(0)?;
                port.put(w)
            }
            Self::Bus(port) => {
                w.u8(1)?;
                port.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Strip(StripPort::get(r)?),
            1 => Self::Bus(BusPort::get(r)?),
            _ => return Err(CodecError::Invalid),
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
            Self::ContentWindow {
                id,
                frames,
                windows,
            } => {
                w.u8(37)?;
                w.u64(id)?;
                w.u64(frames)?;
                windows.put(w)
            }
            Self::LoopTiming {
                loop_id,
                expected_revision,
                expected_content,
                edit,
            } => {
                w.u8(36)?;
                loop_id.put(w)?;
                w.u64(expected_revision)?;
                w.u64(expected_content)?;
                edit.put(w)
            }
            Self::Profiling(enabled) => {
                w.u8(35)?;
                w.bool(enabled)
            }
            Self::ContentFamilyBegin {
                id,
                root,
                count,
                sample_rate,
                quantum,
            } => {
                w.u8(31)?;
                w.u64(id)?;
                root.put(w)?;
                w.u32(count)?;
                w.u32(sample_rate)?;
                w.u32(quantum)
            }
            Self::ContentFamilyEdit { id, edit } => {
                w.u8(32)?;
                w.u64(id)?;
                edit.loop_id.put(w)?;
                edit.sync_source.put(w)?;
                w.u64(edit.expected_revision)?;
                w.u64(edit.expected_frames)?;
                w.u64(edit.frames)?;
                edit.audio.put(w)?;
                edit.midi.put(w)?;
                w.bool(edit.replace)?;
                w.bool(edit.restart)?;
                w.bool(edit.fence_content)
            }
            Self::ContentFamilyCommit { id } => {
                w.u8(33)?;
                w.u64(id)
            }
            Self::ContentFamilyCancel { id } => {
                w.u8(34)?;
                w.u64(id)
            }
            Self::PreviewStart {
                request_id,
                bus,
                content,
                frames,
            } => {
                w.u8(29)?;
                w.u64(request_id)?;
                bus.put(w)?;
                w.u64(content)?;
                w.u64(frames)
            }
            Self::PreviewStop => w.u8(30),
            Self::CreateBus { bus, channels } => {
                w.u8(27)?;
                bus.put(w)?;
                w.u8(channels)
            }
            Self::RemoveBus { bus } => {
                w.u8(28)?;
                bus.put(w)
            }
            Self::CancelBulk => w.u8(26),
            Self::ReplacementBegin { id } => {
                w.u8(19)?;
                w.u64(id)
            }
            Self::ReplacementEdit { id, edit } => {
                w.u8(20)?;
                w.u64(id)?;
                edit.put(w)
            }
            Self::ReplacementPrepare { id } => {
                w.u8(21)?;
                w.u64(id)
            }
            Self::ReplacementCommit { id } => {
                w.u8(22)?;
                w.u64(id)
            }
            Self::ReplacementCancel { id } => {
                w.u8(23)?;
                w.u64(id)
            }
            Self::ContentBegin(description) => {
                w.u8(15)?;
                w.u64(description.id)?;
                w.bool(description.midi)?;
                w.u64(description.frames)?;
                w.u64(description.bytes)
            }
            Self::ContentWrite { id, offset, data } => {
                w.u8(16)?;
                w.u64(id)?;
                w.u64(offset)?;
                w.u16(data.as_bytes().len() as u16)?;
                w.bytes(data.as_bytes())
            }
            Self::ContentFinish { id } => {
                w.u8(17)?;
                w.u64(id)
            }
            Self::ContentInitial { id, message } => {
                w.u8(24)?;
                w.u64(id)?;
                message.put(w)
            }
            Self::ContentApply {
                loop_id,
                frames,
                audio,
                midi,
            } => {
                w.u8(25)?;
                loop_id.put(w)?;
                w.u64(frames)?;
                audio.put(w)?;
                midi.put(w)
            }
            Self::ContentRelease { id } => {
                w.u8(18)?;
                w.u64(id)
            }
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
                port,
                bus,
                connected,
            } => {
                w.u8(11)?;
                port.put(w)?;
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
            37 => Self::ContentWindow {
                id: r.u64()?,
                frames: r.u64()?,
                windows: PlaybackWindows::get(r)?,
            },
            36 => Self::LoopTiming {
                loop_id: LoopSlotId::get(r)?,
                expected_revision: r.u64()?,
                expected_content: r.u64()?,
                edit: TimingEdit::get(r)?,
            },
            35 => Self::Profiling(r.bool()?),
            31 => Self::ContentFamilyBegin {
                id: r.u64()?,
                root: LoopSlotId::get(r)?,
                count: r.u32()?,
                sample_rate: r.u32()?,
                quantum: r.u32()?,
            },
            32 => Self::ContentFamilyEdit {
                id: r.u64()?,
                edit: crate::ContentFamilyEdit {
                    loop_id: LoopSlotId::get(r)?,
                    sync_source: LoopSlotId::get(r)?,
                    expected_revision: r.u64()?,
                    expected_frames: r.u64()?,
                    frames: r.u64()?,
                    audio: Option::get(r)?,
                    midi: Option::get(r)?,
                    replace: r.bool()?,
                    restart: r.bool()?,
                    fence_content: r.bool()?,
                },
            },
            33 => Self::ContentFamilyCommit { id: r.u64()? },
            34 => Self::ContentFamilyCancel { id: r.u64()? },
            29 => Self::PreviewStart {
                request_id: r.u64()?,
                bus: BusId::get(r)?,
                content: r.u64()?,
                frames: r.u64()?,
            },
            30 => Self::PreviewStop,
            27 => Self::CreateBus {
                bus: BusId::get(r)?,
                channels: r.u8()?,
            },
            28 => Self::RemoveBus {
                bus: BusId::get(r)?,
            },
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
                port: EnginePort::get(r)?,
                host: HostPortId::get(r)?,
                connected: r.bool()?,
            },
            11 => Self::Route {
                port: StripPort::get(r)?,
                bus: BusPort::get(r)?,
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
            15 => Self::ContentBegin(crate::ContentDescription {
                id: r.u64()?,
                midi: r.bool()?,
                frames: r.u64()?,
                bytes: r.u64()?,
            }),
            16 => {
                let id = r.u64()?;
                let offset = r.u64()?;
                let length = usize::from(r.u16()?);
                if length > crate::content::CONTENT_CHUNK_BYTES {
                    return Err(CodecError::Invalid);
                }
                let data = crate::ContentChunk::new(r.bytes(length)?).ok_or(CodecError::Invalid)?;
                Self::ContentWrite { id, offset, data }
            }
            17 => Self::ContentFinish { id: r.u64()? },
            18 => Self::ContentRelease { id: r.u64()? },
            19 => Self::ReplacementBegin { id: r.u64()? },
            20 => Self::ReplacementEdit {
                id: r.u64()?,
                edit: crate::ReplacementEdit::get(r)?,
            },
            21 => Self::ReplacementPrepare { id: r.u64()? },
            22 => Self::ReplacementCommit { id: r.u64()? },
            23 => Self::ReplacementCancel { id: r.u64()? },
            26 => Self::CancelBulk,
            24 => Self::ContentInitial {
                id: r.u64()?,
                message: MidiMessage::get(r)?,
            },
            25 => Self::ContentApply {
                loop_id: LoopSlotId::get(r)?,
                frames: r.u64()?,
                audio: Option::<u64>::get(r)?,
                midi: Option::<u64>::get(r)?,
            },
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for crate::ReplacementEdit {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match *self {
            Self::Bus { bus, channels } => {
                w.u8(5)?;
                bus.put(w)?;
                w.u8(channels)
            }
            Self::BusControl { bus, control } => {
                w.u8(6)?;
                bus.put(w)?;
                control.put(w)
            }
            Self::Strip {
                strip,
                shape,
                controls,
            } => {
                w.u8(0)?;
                strip.put(w)?;
                shape.put(w)?;
                w.f32(controls.input_gain)?;
                w.f32(controls.input_balance)?;
                w.bool(controls.monitoring)?;
                w.f32(controls.output_gain)?;
                w.f32(controls.output_balance)?;
                w.bool(controls.output_mute)
            }
            Self::Loop {
                loop_id,
                strip,
                sync,
                frames,
                audio,
                midi,
                gain,
                balance,
            } => {
                w.u8(1)?;
                loop_id.put(w)?;
                strip.put(w)?;
                sync.put(w)?;
                w.u64(frames)?;
                audio.put(w)?;
                midi.put(w)?;
                w.f32(gain)?;
                w.f32(balance)
            }
            Self::Connect { port, host } => {
                w.u8(2)?;
                port.put(w)?;
                host.put(w)
            }
            Self::Route { port, bus } => {
                w.u8(3)?;
                port.put(w)?;
                bus.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            5 => Self::Bus {
                bus: BusId::get(r)?,
                channels: r.u8()?,
            },
            6 => Self::BusControl {
                bus: BusId::get(r)?,
                control: BusControl::get(r)?,
            },
            0 => Self::Strip {
                strip: StripId::get(r)?,
                shape: StripShape::get(r)?,
                controls: crate::StripPreset {
                    input_gain: r.f32()?,
                    input_balance: r.f32()?,
                    monitoring: r.bool()?,
                    output_gain: r.f32()?,
                    output_balance: r.f32()?,
                    output_mute: r.bool()?,
                },
            },
            1 => Self::Loop {
                loop_id: LoopSlotId::get(r)?,
                strip: StripId::get(r)?,
                sync: Option::get(r)?,
                frames: r.u64()?,
                audio: Option::get(r)?,
                midi: Option::get(r)?,
                gain: r.f32()?,
                balance: r.f32()?,
            },
            2 => Self::Connect {
                port: EnginePort::get(r)?,
                host: HostPortId::get(r)?,
            },
            3 => Self::Route {
                port: StripPort::get(r)?,
                bus: BusPort::get(r)?,
            },
            _ => return Err(CodecError::Invalid),
        })
    }
}

impl Wire for Outcome {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::TimingApplied(state) => {
                w.u8(4)?;
                state.put(w)
            }
            Self::Done => w.u8(0),
            Self::ContentApplied { revision } => {
                w.u8(2)?;
                w.u64(*revision)
            }
            Self::ContentPrepared { revision } => {
                w.u8(3)?;
                w.u64(*revision)
            }
            Self::Refused(refusal) => {
                w.u8(1)?;
                refusal.put(w)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            4 => Self::TimingApplied(TimingState::get(r)?),
            0 => Self::Done,
            2 => Self::ContentApplied { revision: r.u64()? },
            3 => Self::ContentPrepared { revision: r.u64()? },
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
        w.u16(self.content_chunk_bytes)?;
        w.u32(self.strips)?;
        w.u32(self.loops)?;
        w.u8(self.strip_audio_channels)?;
        w.u8(self.master_channels)?;
        w.u8(self.buses)?;
        w.u8(self.bus_channels)?;
        w.u16(self.host_audio_inputs)?;
        w.u16(self.host_audio_outputs)?;
        w.u16(self.host_midi_inputs)?;
        w.u16(self.host_midi_outputs)?;
        w.u32(self.max_frames)?;
        w.u32(self.quantum)?;
        w.u32(self.processing_latency_frames)?;
        w.u32(self.smoothing_limit_frames)?;
        w.u16(self.reliable_credits)?;
        w.u16(self.bulk_credits)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            content_chunk_bytes: r.u16()?,
            strips: r.u32()?,
            loops: r.u32()?,
            strip_audio_channels: r.u8()?,
            master_channels: r.u8()?,
            buses: r.u8()?,
            bus_channels: r.u8()?,
            host_audio_inputs: r.u16()?,
            host_audio_outputs: r.u16()?,
            host_midi_inputs: r.u16()?,
            host_midi_outputs: r.u16()?,
            max_frames: r.u32()?,
            quantum: r.u32()?,
            processing_latency_frames: r.u32()?,
            smoothing_limit_frames: r.u32()?,
            reliable_credits: r.u16()?,
            bulk_credits: r.u16()?,
        })
    }
}

impl Wire for Telemetry {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::Preview { request_id } => {
                w.u8(4)?;
                w.u64(*request_id)
            }
            Self::Engine {
                frame,
                callbacks,
                xruns,
                load,
                input_peak,
                output_peak,
                driver,
            } => {
                w.u8(0)?;
                w.u64(*frame)?;
                w.u64(*callbacks)?;
                w.u32(*xruns)?;
                w.f32(*load)?;
                w.f32(*input_peak)?;
                w.f32(*output_peak)?;
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
                content_revision,
                recording_armed,
                timing_revision,
                length,
                position,
                peaks,
            } => {
                w.u8(2)?;
                loop_id.put(w)?;
                mode.put(w)?;
                next_mode.put(w)?;
                next_delay.put(w)?;
                w.u64(*content_revision)?;
                w.bool(*recording_armed)?;
                w.u64(*timing_revision)?;
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
            4 => Self::Preview {
                request_id: r.u64()?,
            },
            0 => Self::Engine {
                frame: r.u64()?,
                callbacks: r.u64()?,
                xruns: r.u32()?,
                load: r.f32()?,
                input_peak: r.f32()?,
                output_peak: r.f32()?,
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
                content_revision: r.u64()?,
                recording_armed: r.bool()?,
                timing_revision: r.u64()?,
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
            Self::LoopTiming { loop_id, timing } => {
                w.u8(12)?;
                loop_id.put(w)?;
                timing.put(w)
            }
            Self::BusShape { bus, channels } => {
                w.u8(10)?;
                bus.put(w)?;
                w.u8(*channels)
            }
            Self::BusGenerations { generation } => {
                w.u8(11)?;
                w.u32(*generation)
            }
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
            Self::Route { port, bus } => {
                w.u8(6)?;
                port.put(w)?;
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
            Self::Generations { strips, loops } => {
                w.u8(9)?;
                w.u32(*strips)?;
                w.u32(*loops)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            12 => Self::LoopTiming {
                loop_id: LoopSlotId::get(r)?,
                timing: TimingState::get(r)?,
            },
            10 => Self::BusShape {
                bus: BusId::get(r)?,
                channels: r.u8()?,
            },
            11 => Self::BusGenerations {
                generation: r.u32()?,
            },
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
                port: EnginePort::get(r)?,
                host: HostPortId::get(r)?,
            },
            6 => Self::Route {
                port: StripPort::get(r)?,
                bus: BusPort::get(r)?,
            },
            7 => Self::Bus {
                bus: BusId::get(r)?,
                control: BusControl::get(r)?,
            },
            8 => Self::End { epoch: r.u64()? },
            9 => Self::Generations {
                strips: r.u32()?,
                loops: r.u32()?,
            },
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
            Self::AudioFrames => w.u8(2),
            Self::MidiInitial { channel } => {
                w.u8(3)?;
                w.u8(*channel)
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match r.u8()? {
            0 => Self::Audio { channel: r.u8()? },
            1 => Self::Midi,
            2 => Self::AudioFrames,
            3 => Self::MidiInitial { channel: r.u8()? },
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

impl Wire for rt_profiling::Packet {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        if !self.valid() {
            return Err(CodecError::Invalid);
        }
        w.u64(self.generation)?;
        w.u64(self.capture)?;
        w.u64(self.sequence)?;
        w.u64(self.callback)?;
        w.u64(self.callback_frame)?;
        w.u64(self.frame)?;
        w.u64(self.epoch)?;
        w.u64(self.start_ns)?;
        w.u64(self.end_ns)?;
        w.u64(self.budget_ns)?;
        w.u32(self.graphs)?;
        w.u32(self.frames)?;
        w.u32(self.index)?;
        w.u32(self.total)?;
        w.u8(self.len)?;
        w.u32(self.flags)?;
        w.u32(self.trigger)?;
        for sample in &self.samples[..usize::from(self.len)] {
            w.u64(sample.key)?;
            w.u64(sample.at)?;
        }
        Ok(())
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut packet = Self {
            generation: r.u64()?,
            capture: r.u64()?,
            sequence: r.u64()?,
            callback: r.u64()?,
            callback_frame: r.u64()?,
            frame: r.u64()?,
            epoch: r.u64()?,
            start_ns: r.u64()?,
            end_ns: r.u64()?,
            budget_ns: r.u64()?,
            graphs: r.u32()?,
            frames: r.u32()?,
            index: r.u32()?,
            total: r.u32()?,
            len: r.u8()?,
            flags: r.u32()?,
            trigger: r.u32()?,
            ..Self::default()
        };
        if !packet.valid() {
            return Err(CodecError::Invalid);
        }
        for sample in &mut packet.samples[..usize::from(packet.len)] {
            sample.key = r.u64()?;
            sample.at = r.u64()?;
        }
        Ok(packet)
    }
}
impl Wire for rt_profiling::Port {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.bool(self.node.is_some())?;
        w.u64(self.node.unwrap_or(0))?;
        w.u8(self.kind)?;
        w.u32(self.channel)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let node = r.bool()?;
        let key = r.u64()?;
        let kind = r.u8()?;
        let channel = r.u32()?;
        if kind > 1
            || channel > u16::MAX as u32
            || (node && key >= rt_profiling::MAX_SAMPLES as u64)
        {
            return Err(CodecError::Invalid);
        }
        Ok(Self {
            node: node.then_some(key),
            kind,
            channel,
        })
    }
}
impl Wire for rt_profiling::GraphRecord {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        use rt_profiling::GraphRecord::*;
        match self {
            Begin {
                nodes,
                edges,
                sample_rate,
                quantum,
            } => {
                w.u8(0)?;
                w.u32(*nodes)?;
                w.u32(*edges)?;
                w.u32(*sample_rate)?;
                w.u32(*quantum)
            }
            Node { index, node } => {
                w.u8(1)?;
                w.u32(*index)?;
                w.u64(node.key)?;
                w.u8(node.group)?;
                w.u32(node.slot)?;
                w.u32(node.owner_generation)
            }
            Edge { index, edge } => {
                w.u8(2)?;
                w.u32(*index)?;
                edge.source.put(w)?;
                edge.destination.put(w)
            }
            End => w.u8(3),
            Unavailable => w.u8(4),
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        use rt_profiling::{GraphEdge, GraphNode};
        Ok(match r.u8()? {
            0 => {
                let nodes = r.u32()?;
                let edges = r.u32()?;
                if nodes as usize > rt_profiling::MAX_GRAPH_NODES
                    || edges as usize > rt_profiling::MAX_GRAPH_EDGES
                {
                    return Err(CodecError::Invalid);
                }
                Self::Begin {
                    nodes,
                    edges,
                    sample_rate: r.u32()?,
                    quantum: r.u32()?,
                }
            }
            1 => {
                let index = r.u32()?;
                let key = r.u64()?;
                let group = r.u8()?;
                let slot = r.u32()?;
                let owner_generation = r.u32()?;
                if index as usize >= rt_profiling::MAX_GRAPH_NODES
                    || key >= rt_profiling::MAX_SAMPLES as u64
                    || group > 3
                    || (group != 0 && owner_generation == 0)
                {
                    return Err(CodecError::Invalid);
                }
                Self::Node {
                    index,
                    node: GraphNode {
                        key,
                        group,
                        slot,
                        owner_generation,
                    },
                }
            }
            2 => Self::Edge {
                index: r.u32()?,
                edge: GraphEdge {
                    source: rt_profiling::Port::get(r)?,
                    destination: rt_profiling::Port::get(r)?,
                },
            },
            3 => Self::End,
            4 => Self::Unavailable,
            _ => return Err(CodecError::Invalid),
        })
    }
}
impl Wire for rt_profiling::Status {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.u64(self.generation)?;
        w.bool(self.requested)?;
        w.bool(self.active)?;
        w.bool(self.available)?;
        w.u64(self.resolution_ns)?;
        w.u64(self.dropped)?;
        w.u64(self.skipped_captures)?;
        w.u64(self.high_water)?;
        w.u64(self.storage_bytes)
    }
    fn get(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            generation: r.u64()?,
            requested: r.bool()?,
            active: r.bool()?,
            available: r.bool()?,
            resolution_ns: r.u64()?,
            dropped: r.u64()?,
            skipped_captures: r.u64()?,
            high_water: r.u64()?,
            storage_bytes: r.u64()?,
        })
    }
}

impl Wire for EngineMessage {
    fn put(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        match self {
            Self::ProfilingGraph {
                generation,
                epoch,
                record,
            } => {
                w.u8(11)?;
                w.u64(*generation)?;
                w.u64(*epoch)?;
                record.put(w)
            }
            Self::ProfilingStatus(status) => {
                w.u8(8)?;
                status.put(w)
            }
            Self::Profiling(packet) => {
                w.u8(9)?;
                packet.put(w)
            }
            Self::ProfilingNode {
                generation,
                key,
                name,
            } => {
                w.u8(10)?;
                w.u64(*generation)?;
                w.u64(*key)?;
                name.put(w)
            }
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
            11 => Self::ProfilingGraph {
                generation: r.u64()?,
                epoch: r.u64()?,
                record: rt_profiling::GraphRecord::get(r)?,
            },
            8 => Self::ProfilingStatus(rt_profiling::Status::get(r)?),
            9 => Self::Profiling(rt_profiling::Packet::get(r)?),
            10 => Self::ProfilingNode {
                generation: r.u64()?,
                key: r.u64()?,
                name: Name::get(r)?,
            },
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

mod host;
pub use host::{decode_host_reply, decode_host_request, encode_host_reply, encode_host_request};
