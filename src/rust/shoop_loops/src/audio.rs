//! Prepared audio conversion and DSP. Positions and discontinuities come from
//! the authoritative interval decision; these renderers never advance a clock.

use alloc::{boxed::Box, vec};
use rt_runtime::audio_samples::SampleSlice;
use rt_runtime::buffer::AudioPorts;
use rt_runtime::resource::shared_audio::{AudioStore, ReadIndex as SharedReadIndex};
use rt_runtime::resource::{CaptureTickId, Format, ReadIndex, RenderIo};
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

#[derive(Clone, Copy, Debug)]
pub struct SharedPlayback {
    pub source: Option<SharedReadIndex>,
    pub position: u64,
    pub gain: f32,
    pub balance: f32,
    pub discontinuity: bool,
}

#[derive(Clone, Copy)]
enum Source<'a> {
    Stored(ReadIndex),
    Shared(&'a AudioStore, SharedReadIndex),
}

pub struct AudioRenderer {
    channels: usize,
    balance_channels: usize,
    max_frames: usize,
    bytes: Box<[u8]>,
    window_bytes: Box<[u8]>,
    last: Box<[f32]>,
    anchor: Box<[f32]>,
    ramp_weights: Box<[f32]>,
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
        *core::hint::black_box(value) = T::default();
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
    Ports(&'a mut AudioPorts<'b>, usize, usize),
}
impl Outputs<'_, '_> {
    fn count(&self) -> usize {
        match self {
            Self::Slices(value) => value.len(),
            Self::Ports(_, _, count) => *count,
        }
    }
    fn channel(&mut self, channel: usize) -> &mut [f32] {
        match self {
            Self::Slices(value) => value[channel],
            Self::Ports(value, first, _) => value
                .output(first.saturating_add(channel))
                .expect("declared output port"),
        }
    }
}
enum Inputs<'a, 'b> {
    Slices(&'a [&'b [f32]]),
    Ports(&'a AudioPorts<'b>, usize, usize),
}
impl Inputs<'_, '_> {
    fn count(&self) -> usize {
        match self {
            Self::Slices(value) => value.len(),
            Self::Ports(_, _, count) => *count,
        }
    }
    fn channel(&self, channel: usize) -> &[f32] {
        match self {
            Self::Slices(value) => value[channel],
            Self::Ports(value, first, _) => value
                .input(first.saturating_add(channel))
                .expect("declared input port"),
        }
    }
}
impl AudioRenderer {
    pub fn reference_ready(&self, smoothing: Smoothing, discontinuity: bool) -> bool {
        self.ramp_position >= self.ramp_frames && (!discontinuity || smoothing == Smoothing::None)
    }
    pub fn reference_last(&mut self, channel: usize, sample: f32) {
        self.last[channel] = sample;
    }
    pub fn reference_silence(&mut self) {
        self.last.fill(0.);
        self.anchor.fill(0.);
        self.ramp_frames = self.next_ramp_frames;
        self.ramp_position = self.ramp_frames;
    }
    pub fn prepare(
        channels: usize,
        max_frames: usize,
        smoothing: Smoothing,
    ) -> Result<Self, Error> {
        let count = samples(channels, max_frames)?;
        count
            .checked_mul(8)
            .and_then(|n| {
                channels
                    .checked_mul(8)
                    .and_then(|extra| n.checked_add(extra))
            })
            .and_then(|n| {
                max_frames
                    .checked_mul(4)
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
            balance_channels: channels,
            max_frames,
            bytes: buffer(count * 4),
            window_bytes: buffer(count * 4),
            last: buffer(channels),
            anchor: buffer(channels),
            ramp_weights: buffer(max_frames),
            ramp_frames,
            ramp_position: ramp_frames,
            next_ramp_frames: ramp_frames,
        })
    }

    pub fn set_balance_channels(&mut self, channels: usize) -> Result<(), Error> {
        if channels > self.channels {
            return Err(Error::InvalidShape);
        }
        self.balance_channels = channels;
        Ok(())
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
        self.render_output(io, playback, Outputs::Slices(outputs), None)
    }

    /// Render directly into runtime ports without retaining channel borrows.
    /// Only the output ports and their current interval are written.
    pub fn render_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        outputs: &mut AudioPorts<'_>,
    ) -> Result<(), Error> {
        self.render_output(
            io,
            playback,
            Outputs::Ports(outputs, 0, outputs.output_count()),
            None,
        )
    }

    pub fn render_port_range(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        outputs: &mut AudioPorts<'_>,
        first: usize,
    ) -> Result<(), Error> {
        if first
            .checked_add(self.channels)
            .is_none_or(|end| end > outputs.output_count())
        {
            return Err(Error::InvalidShape);
        }
        self.render_output(
            io,
            playback,
            Outputs::Ports(outputs, first, self.channels),
            None,
        )
    }

    pub fn render_windowed_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        position: i64,
        windows: &[crate::windows::ChannelWindow],
        lead: bool,
        outputs: &mut AudioPorts<'_>,
    ) -> Result<(), Error> {
        self.render_output(
            io,
            playback,
            Outputs::Ports(outputs, 0, self.channels),
            Some((position, windows, lead)),
        )
    }

    pub fn render_windowed_port_range(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        position: i64,
        windows: &[crate::windows::ChannelWindow],
        lead: bool,
        outputs: &mut AudioPorts<'_>,
        first: usize,
    ) -> Result<(), Error> {
        if first
            .checked_add(self.channels)
            .is_none_or(|end| end > outputs.output_count())
        {
            return Err(Error::InvalidShape);
        }
        self.render_output(
            io,
            playback,
            Outputs::Ports(outputs, first, self.channels),
            Some((position, windows, lead)),
        )
    }

    fn render_output(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        outputs: Outputs<'_, '_>,
        windows: Option<(i64, &[crate::windows::ChannelWindow], bool)>,
    ) -> Result<(), Error> {
        self.render_source(
            io,
            playback,
            playback.source.map(Source::Stored),
            outputs,
            windows,
        )
    }

    pub fn render_shared(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &AudioStore,
        playback: SharedPlayback,
        outputs: &mut [&mut [f32]],
    ) -> Result<(), Error> {
        self.render_source(
            io,
            Playback {
                source: None,
                position: playback.position,
                gain: playback.gain,
                balance: playback.balance,
                discontinuity: playback.discontinuity,
            },
            playback.source.map(|index| Source::Shared(media, index)),
            Outputs::Slices(outputs),
            None,
        )
    }

    pub fn render_shared_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &AudioStore,
        playback: SharedPlayback,
        outputs: &mut AudioPorts<'_>,
    ) -> Result<(), Error> {
        self.render_source(
            io,
            Playback {
                source: None,
                position: playback.position,
                gain: playback.gain,
                balance: playback.balance,
                discontinuity: playback.discontinuity,
            },
            playback.source.map(|index| Source::Shared(media, index)),
            Outputs::Ports(outputs, 0, outputs.output_count()),
            None,
        )
    }

    fn render_source(
        &mut self,
        io: &mut RenderIo<'_>,
        playback: Playback,
        source: Option<Source<'_>>,
        mut outputs: Outputs<'_, '_>,
        windows: Option<(i64, &[crate::windows::ChannelWindow], bool)>,
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
        let factors = balance_factors(self.balance_channels, playback.balance);
        let Some(source) = source else {
            for channel in 0..channels {
                outputs.channel(channel).fill_audio_samples(0.0);
            }
            self.last.fill(0.0);
            self.anchor.fill(0.0);
            self.ramp_frames = self.next_ramp_frames;
            self.ramp_position = self.ramp_frames;
            return Ok(());
        };
        let description = match source {
            Source::Stored(index) => io.snapshot(index)?.description,
            Source::Shared(media, index) => {
                let info = media.info(index)?;
                rt_runtime::resource::Description {
                    format: Format::Audio {
                        channels: info.channels,
                    },
                    frames: info.frames,
                    bytes: 0,
                }
            }
        };
        if description.format
            != (Format::Audio {
                channels: self.channels as u32,
            })
        {
            return Err(Error::InvalidShape);
        }
        if let Some((position, windows, lead)) = windows {
            if windows.len() != channels || channels > crate::windows::MAX_AUDIO_CHANNELS {
                return Err(Error::InvalidShape);
            }
            self.bytes[..frames * channels * 4].fill_audio_samples(0);
            for (channel, window) in windows.iter().enumerate() {
                let span =
                    crate::windows::span(position, *window, lead, frames, description.frames);
                if span.frames == 0 {
                    continue;
                }
                read_audio_source(
                    io,
                    source,
                    span.position,
                    channels,
                    &mut self.window_bytes[..span.frames * channels * 4],
                )?;
                rt_runtime::audio_samples::process(|| {
                    for local in 0..span.frames {
                        let offset = (local * channels + channel) * 4;
                        let sample = f32::from_le_bytes(
                            self.window_bytes[offset..offset + 4]
                                .try_into()
                                .expect("audio sample"),
                        ) * f32::from_bits(window.gain_bits);
                        let byte = ((span.offset + local) * channels + channel) * 4;
                        self.bytes[byte..byte + 4].copy_from_slice(&sample.to_le_bytes());
                    }
                });
            }
        } else {
            let end = playback
                .position
                .checked_add(frames as u64)
                .ok_or(Error::InvalidFrame)?;
            if end > description.frames {
                return Err(Error::InvalidFrame);
            }
            read_audio_source(
                io,
                source,
                playback.position,
                channels,
                &mut self.bytes[..frames * channels * 4],
            )?;
        }
        if playback.discontinuity {
            self.anchor.copy_from_slice(&self.last);
            self.ramp_frames = self.next_ramp_frames;
            self.ramp_position = 0;
        }
        let ramp_start = self.ramp_position;
        let ramping_frames = (self.ramp_frames.saturating_sub(ramp_start) as usize).min(frames);
        rt_runtime::audio_samples::process(|| {
            for (frame, weight) in self.ramp_weights[..ramping_frames].iter_mut().enumerate() {
                *weight = (ramp_start + frame as u32 + 1) as f32 / self.ramp_frames as f32;
            }
        });
        for channel in 0..channels {
            let output = outputs.channel(channel);
            let factor = factors[channel.min(1)];
            rt_runtime::audio_samples::process(|| {
                for (frame, sample) in output[..ramping_frames].iter_mut().enumerate() {
                    let byte = (frame * self.channels + channel) * 4;
                    let raw = f32::from_le_bytes(
                        self.bytes[byte..byte + 4].try_into().expect("four bytes"),
                    ) * playback.gain
                        * factor;
                    let weight = self.ramp_weights[frame];
                    *sample = self.anchor[channel] * (1.0 - weight) + raw * weight;
                }
            });
            rt_runtime::audio_samples::process(|| {
                for (offset, sample) in output[ramping_frames..].iter_mut().enumerate() {
                    let frame = ramping_frames + offset;
                    let byte = (frame * self.channels + channel) * 4;
                    *sample = f32::from_le_bytes(
                        self.bytes[byte..byte + 4].try_into().expect("four bytes"),
                    ) * playback.gain
                        * factor;
                }
            });
            self.last[channel] = if rt_runtime::audio_samples::ENABLED {
                output[frames - 1]
            } else {
                0.
            };
        }
        self.ramp_position += ramping_frames as u32;
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

    /// Append one common multichannel interval to prepared retrospective history.
    pub fn append_shared(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &mut AudioStore,
        frame: rt_runtime::Frame,
        inputs: &[&[f32]],
    ) -> Result<(), Error> {
        self.append_shared_input(io, media, frame, Inputs::Slices(inputs))
    }

    pub fn append_shared_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &mut AudioStore,
        frame: rt_runtime::Frame,
        inputs: &AudioPorts<'_>,
    ) -> Result<(), Error> {
        self.append_shared_input(
            io,
            media,
            frame,
            Inputs::Ports(inputs, 0, inputs.input_count()),
        )
    }

    fn append_shared_input(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &mut AudioStore,
        frame: rt_runtime::Frame,
        inputs: Inputs<'_, '_>,
    ) -> Result<(), Error> {
        if media.channels() as usize != self.channels || inputs.count() != self.channels {
            return Err(Error::InvalidShape);
        }
        let frames = shape(
            self.channels,
            self.max_frames,
            (0..self.channels).map(|channel| inputs.channel(channel).len()),
        )?;
        rt_runtime::audio_samples::process(|| {
            for frame in 0..frames {
                for channel in 0..self.channels {
                    self.interleaved[frame * self.channels + channel] =
                        inputs.channel(channel)[frame];
                }
            }
        });
        media.append(io, frame, &self.interleaved[..frames * self.channels])
    }
    /// Convert planar input into prepared scratch and stage one group member.
    /// Only the planner's later group commit publishes the captured prefix.
    pub fn stage(
        &mut self,
        io: &mut RenderIo<'_>,
        interval: CaptureTickId,
        member: usize,
        inputs: &[&[f32]],
    ) -> Result<(), Error> {
        self.stage_input(io, interval, member, Inputs::Slices(inputs))
    }

    /// Stage directly from runtime input ports; output ports are not observed.
    pub fn stage_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        interval: CaptureTickId,
        member: usize,
        inputs: &AudioPorts<'_>,
    ) -> Result<(), Error> {
        self.stage_input(
            io,
            interval,
            member,
            Inputs::Ports(inputs, 0, inputs.input_count()),
        )
    }

    pub fn stage_port_range(
        &mut self,
        io: &mut RenderIo<'_>,
        interval: CaptureTickId,
        member: usize,
        inputs: &AudioPorts<'_>,
        first: usize,
    ) -> Result<(), Error> {
        if first
            .checked_add(self.channels)
            .is_none_or(|end| end > inputs.input_count())
        {
            return Err(Error::InvalidShape);
        }
        self.stage_input(
            io,
            interval,
            member,
            Inputs::Ports(inputs, first, self.channels),
        )
    }
    pub fn replace_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        resource: rt_runtime::ResourceId,
        revision: rt_runtime::Revision,
        position: u64,
        inputs: &AudioPorts<'_>,
    ) -> Result<rt_runtime::Revision, Error> {
        if inputs.input_count() < self.channels {
            return Err(Error::InvalidShape);
        }
        let frames = shape(
            self.channels,
            self.max_frames,
            (0..self.channels).map(|channel| inputs.input(channel).expect("validated port").len()),
        )?;
        rt_runtime::audio_samples::process(|| {
            for frame in 0..frames {
                for channel in 0..self.channels {
                    self.interleaved[frame * self.channels + channel] =
                        inputs.input(channel).expect("validated port")[frame];
                }
            }
        });
        io.replace_audio(
            resource,
            revision,
            position,
            &self.interleaved[..frames * self.channels],
        )
    }

    pub fn replace_windowed_ports(
        &mut self,
        io: &mut RenderIo<'_>,
        resource: rt_runtime::ResourceId,
        mut revision: rt_runtime::Revision,
        position: u64,
        raw_frames: u64,
        windows: &[crate::windows::ChannelWindow],
        inputs: &AudioPorts<'_>,
    ) -> Result<rt_runtime::Revision, Error> {
        if windows.len() > self.channels || inputs.input_count() < self.channels {
            return Err(Error::InvalidShape);
        }
        if windows.iter().all(|window| window.start_offset == 0) {
            return self.replace_ports(io, resource, revision, position, inputs);
        }
        let raw_frames = windows
            .iter()
            .filter_map(|window| window.raw_frames)
            .fold(raw_frames, u64::max);
        for (channel, window) in windows.iter().copied().enumerate() {
            let input = inputs.input(channel).ok_or(Error::InvalidShape)?;
            let span = crate::windows::span(
                i64::try_from(position).map_err(|_| Error::InvalidFrame)?,
                crate::windows::ChannelWindow {
                    capture_alignment: 0,
                    ..window
                },
                false,
                input.len(),
                raw_frames,
            );
            if span.frames > 0 {
                revision = io.replace_audio_channel(
                    resource,
                    revision,
                    channel,
                    span.position,
                    &input[span.offset..span.offset + span.frames],
                )?;
            }
        }
        Ok(revision)
    }

    fn stage_input(
        &mut self,
        io: &mut RenderIo<'_>,
        interval: CaptureTickId,
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
        rt_runtime::audio_samples::process(|| {
            for frame in 0..frames {
                for channel in 0..channels {
                    self.interleaved[frame * self.channels + channel] =
                        inputs.channel(channel)[frame];
                }
            }
        });
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

fn read_audio_source(
    io: &mut RenderIo<'_>,
    source: Source<'_>,
    position: u64,
    channels: usize,
    output: &mut [u8],
) -> Result<(), Error> {
    match source {
        Source::Stored(index) => {
            let offset = usize::try_from(position)
                .ok()
                .and_then(|p| p.checked_mul(channels * 4))
                .ok_or(Error::InvalidOffset)?;
            let mut cursor = io.cursor_at(index, offset)?;
            let mut copied = 0;
            while copied < output.len() {
                let count = io.read(&mut cursor, &mut output[copied..])?;
                if count == 0 {
                    return Err(Error::Incomplete);
                }
                copied += count;
            }
        }
        Source::Shared(media, index) => {
            if media.read_bytes(io, index, position, output)? != output.len() / (channels * 4) {
                return Err(Error::Incomplete);
            }
        }
    }
    Ok(())
}
