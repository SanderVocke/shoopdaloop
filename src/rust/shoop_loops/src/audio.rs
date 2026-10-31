//! Prepared audio conversion and DSP. Positions and discontinuities come from
//! the authoritative interval decision; these renderers never advance a clock.

use rt_runtime::buffer::AudioPorts;
use rt_runtime::resource::{CaptureIntervalId, Format, ReadIndex, RenderIo};
use rt_runtime::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Smoothing {
    #[default]
    None,
    LinearRestart {
        frames: u32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Playback {
    pub source: Option<ReadIndex>,
    pub position: u64,
    pub gain: f32,
    /// Stereo balance in [-1, 1]; ignored unless there are exactly two channels.
    pub balance: f32,
    /// Start, retrigger, seek, wrap, or content replacement at this boundary.
    pub discontinuity: bool,
}

pub struct AudioRenderer {
    channels: usize,
    max_frames: usize,
    bytes: Box<[u8]>,
    last: Box<[f32]>,
    anchor: Box<[f32]>,
    ramp_frames: u32,
    ramp_position: u32,
    next_ramp_frames: u32,
}

fn samples(channels: usize, max_frames: usize) -> Result<usize, Error> {
    if channels == 0 || max_frames == 0 || channels > u32::MAX as usize {
        return Err(Error::InvalidConfig);
    }
    channels
        .checked_mul(max_frames)
        .filter(|n| *n <= i32::MAX as usize / 4)
        .ok_or(Error::InvalidConfig)
}
fn buffer<T: Default + Copy>(len: usize) -> Box<[T]> {
    let mut values = vec![T::default(); len].into_boxed_slice();
    for value in &mut values {
        *std::hint::black_box(value) = T::default();
    }
    values
}
fn shape(
    channels: usize,
    max_frames: usize,
    lengths: impl Iterator<Item = usize>,
) -> Result<usize, Error> {
    let mut count = 0;
    let mut frames = None;
    for len in lengths {
        if len > max_frames || frames.is_some_and(|n| n != len) {
            return Err(Error::InvalidShape);
        }
        frames = Some(len);
        count += 1;
    }
    if count != channels {
        return Err(Error::InvalidShape);
    }
    Ok(frames.unwrap_or(0))
}
enum Outputs<'a, 'b> {
    Slices(&'a mut [&'b mut [f32]]),
    Ports(&'a mut AudioPorts<'b>),
}
impl Outputs<'_, '_> {
    fn count(&self) -> usize {
        match self {
            Self::Slices(value) => value.len(),
            Self::Ports(value) => value.output_count(),
        }
    }
    fn channel(&mut self, channel: usize) -> &mut [f32] {
        match self {
            Self::Slices(value) => value[channel],
            Self::Ports(value) => value.output(channel).expect("declared output port"),
        }
    }
}
enum Inputs<'a, 'b> {
    Slices(&'a [&'b [f32]]),
    Ports(&'a AudioPorts<'b>),
}
impl Inputs<'_, '_> {
    fn count(&self) -> usize {
        match self {
            Self::Slices(value) => value.len(),
            Self::Ports(value) => value.input_count(),
        }
    }
    fn channel(&self, channel: usize) -> &[f32] {
        match self {
            Self::Slices(value) => value[channel],
            Self::Ports(value) => value.input(channel).expect("declared input port"),
        }
    }
}
impl AudioRenderer {
    pub fn prepare(
        channels: usize,
        max_frames: usize,
        smoothing: Smoothing,
    ) -> Result<Self, Error> {
        let count = samples(channels, max_frames)?;
        count
            .checked_mul(4)
            .and_then(|n| {
                channels
                    .checked_mul(8)
                    .and_then(|extra| n.checked_add(extra))
            })
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(Error::InvalidConfig)?;
        let ramp_frames = match smoothing {
            Smoothing::None => 0,
            Smoothing::LinearRestart { frames } => frames,
        };
        Ok(Self {
            channels,
            max_frames,
            bytes: buffer(count * 4),
            last: buffer(channels),
            anchor: buffer(channels),
            ramp_frames,
            ramp_position: ramp_frames,
            next_ramp_frames: ramp_frames,
        })
    }

    /// Smoothing for the following restarts; a restart ramp in progress keeps
    /// its length.
    pub fn set_smoothing(&mut self, smoothing: Smoothing) {
        self.next_ramp_frames = match smoothing {
            Smoothing::None => 0,
            Smoothing::LinearRestart { frames } => frames,
        };
    }

    /// Render one non-wrapping region. A wrap is a resolver boundary, not a
    /// renderer-discovered occurrence. Refusal leaves output and DSP state intact.
    pub fn render(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        outputs: &mut [&mut [f32]],
    ) -> Result<(), Error> {
        self.render_output(io, playback, Outputs::Slices(outputs))
    }

    /// Render directly into runtime ports without retaining channel borrows.
    /// Only the output ports and their current interval are written.
    pub fn render_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        outputs: &mut AudioPorts<'_>,
    ) -> Result<(), Error> {
        self.render_output(io, playback, Outputs::Ports(outputs))
    }

    fn render_output(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        mut outputs: Outputs<'_, '_>,
    ) -> Result<(), Error> {
        let channels = outputs.count();
        let frames = shape(
            self.channels,
            self.max_frames,
            (0..channels).map(|channel| outputs.channel(channel).len()),
        )?;
        if frames == 0 {
            return Ok(());
        }
        if !playback.gain.is_finite() || !(-1.0..=1.0).contains(&playback.balance) {
            return Err(Error::InvalidParameter);
        }
        let factors = balance_factors(channels, playback.balance);
        let Some(index) = playback.source else {
            for channel in 0..channels {
                outputs.channel(channel).fill(0.0);
            }
            self.last.fill(0.0);
            self.anchor.fill(0.0);
            self.ramp_frames = self.next_ramp_frames;
            self.ramp_position = self.ramp_frames;
            return Ok(());
        };
        let description = io.snapshot(index)?.description;
        if description.format
            != (Format::Audio {
                channels: self.channels as u32,
            })
        {
            return Err(Error::InvalidShape);
        }
        let end = playback
            .position
            .checked_add(frames as u64)
            .ok_or(Error::InvalidFrame)?;
        if end > description.frames {
            return Err(Error::InvalidFrame);
        }
        let offset = usize::try_from(playback.position)
            .ok()
            .and_then(|p| p.checked_mul(self.channels * 4))
            .ok_or(Error::InvalidOffset)?;
        let bytes = frames * self.channels * 4;
        let mut cursor = io.cursor_at(index, offset)?;
        let mut copied = 0;
        while copied < bytes {
            let count = io.read(&mut cursor, &mut self.bytes[copied..bytes])?;
            if count == 0 {
                return Err(Error::Incomplete);
            }
            copied += count;
        }
        if playback.discontinuity {
            self.anchor.copy_from_slice(&self.last);
            self.ramp_frames = self.next_ramp_frames;
            self.ramp_position = 0;
        }
        for frame in 0..frames {
            let ramping = self.ramp_position < self.ramp_frames;
            let weight = if ramping {
                self.ramp_position += 1;
                self.ramp_position as f32 / self.ramp_frames as f32
            } else {
                1.0
            };
            for channel in 0..channels {
                let output = outputs.channel(channel);
                let byte = (frame * self.channels + channel) * 4;
                let raw =
                    f32::from_le_bytes(self.bytes[byte..byte + 4].try_into().expect("four bytes"))
                        * playback.gain
                        * factors[channel.min(1)];
                let value = if ramping {
                    self.anchor[channel] * (1.0 - weight) + raw * weight
                } else {
                    raw
                };
                output[frame] = value;
                self.last[channel] = value;
            }
        }
        Ok(())
    }
}

pub struct AudioCapture {
    channels: usize,
    max_frames: usize,
    interleaved: Box<[f32]>,
}
impl AudioCapture {
    pub fn prepare(channels: usize, max_frames: usize) -> Result<Self, Error> {
        Ok(Self {
            channels,
            max_frames,
            interleaved: buffer(samples(channels, max_frames)?),
        })
    }
    /// Convert planar input into prepared scratch and stage one group member.
    /// Only the planner's later group commit publishes the captured prefix.
    pub fn stage(
        &mut self,
        io: &mut RenderIo<'_>,
        interval: CaptureIntervalId,
        member: usize,
        inputs: &[&[f32]],
    ) -> Result<(), Error> {
        self.stage_input(io, interval, member, Inputs::Slices(inputs))
    }

    /// Stage directly from runtime input ports; output ports are not observed.
    pub fn stage_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        interval: CaptureIntervalId,
        member: usize,
        inputs: &AudioPorts<'_>,
    ) -> Result<(), Error> {
        self.stage_input(io, interval, member, Inputs::Ports(inputs))
    }

    fn stage_input(
        &mut self,
        io: &mut RenderIo<'_>,
        interval: CaptureIntervalId,
        member: usize,
        inputs: Inputs<'_, '_>,
    ) -> Result<(), Error> {
        let channels = inputs.count();
        let frames = shape(
            self.channels,
            self.max_frames,
            (0..channels).map(|channel| inputs.channel(channel).len()),
        )?;
        if frames == 0 {
            return Err(Error::InvalidFrame);
        }
        for frame in 0..frames {
            for channel in 0..channels {
                self.interleaved[frame * self.channels + channel] = inputs.channel(channel)[frame];
            }
        }
        io.stage_audio(
            interval,
            member,
            &self.interleaved[..frames * self.channels],
        )
    }
}

/// Left/right factors of a stereo balance; mono and multichannel outputs are unscaled.
fn balance_factors(channels: usize, balance: f32) -> [f32; 2] {
    match channels {
        2 if balance < 0.0 => [1.0, 1.0 + balance],
        2 => [1.0 - balance, 1.0],
        _ => [1.0, 1.0],
    }
}
