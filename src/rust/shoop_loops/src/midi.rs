//! Prepared indexed MIDI operations. Caller decisions own time and mode; cached
//! cursor positions only validate contiguous use of an immutable media prefix.
use crate::midi_restore::{self, Emission};
use alloc::{boxed::Box, vec};
use rt_runtime::Sample;
use rt_runtime::{
    buffer::MidiBuffer,
    midi::{Message, ResetState},
    midi_state::MidiState,
    resource::{CaptureTickId, Format, MidiCursor, MidiEvent, ReadIndex, RenderIo, Snapshot},
    Error,
};

#[derive(Clone, Copy, Debug)]
pub struct Playback {
    pub source: Option<ReadIndex>,
    pub position: u64,
    pub discontinuity: bool,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderReport {
    pub attempted: usize,
    pub accepted: usize,
    pub output_error: Option<Error>,
    pub parameter_history_untracked: bool,
}
impl RenderReport {
    fn emission(&mut self, emission: Emission) {
        self.attempted += emission.attempted;
        self.accepted += emission.accepted;
        self.output_error = self.output_error.or(emission.first_error);
        self.parameter_history_untracked |= emission.parameter_history_untracked;
    }
    fn packet(&mut self, output: &mut MidiBuffer, offset: u32, frames: usize, data: &[u8]) {
        self.attempted += 1;
        if self.output_error.is_none() {
            match output.push(offset, frames, data) {
                Ok(()) => self.accepted += 1,
                Err(error) => self.output_error = Some(error),
            }
        }
    }
}
#[derive(Clone, Copy)]
struct Stream {
    index: ReadIndex,
    snapshot: Snapshot,
    cursor: MidiCursor,
    next: Option<MidiEvent>,
    end_position: u64,
}
pub struct MidiRenderer {
    max_frames: usize,
    packet: Box<[u8]>,
    state: Box<MidiState>,
    active: bool,
    stream: Option<Stream>,
}
fn prepared_state() -> Box<MidiState> {
    let mut state = Box::new(MidiState::default());
    *core::hint::black_box(&mut *state) = MidiState::default();
    state
}
fn dimensions(max_frames: usize, max_bytes: usize) -> Result<(), Error> {
    if max_frames == 0
        || max_frames > u32::MAX as usize
        || max_bytes < 3
        || max_bytes > i32::MAX as usize
    {
        return Err(Error::InvalidConfig);
    }
    Ok(())
}
impl MidiRenderer {
    pub fn prepare(max_frames: usize, max_packet_bytes: usize) -> Result<Self, Error> {
        dimensions(max_frames, max_packet_bytes)?;
        max_packet_bytes
            .checked_add(core::mem::size_of::<Self>())
            .and_then(|n| n.checked_add(core::mem::size_of::<MidiState>()))
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(Error::InvalidConfig)?;
        let mut packet = vec![0; max_packet_bytes].into_boxed_slice();
        for byte in &mut packet {
            *core::hint::black_box(byte) = 0;
        }
        Ok(Self {
            max_frames,
            packet,
            state: prepared_state(),
            active: false,
            stream: None,
        })
    }
    /// Intended source projection, not acknowledgement of an external device.
    pub fn state(&self) -> &MidiState {
        &self.state
    }

    /// Output must be empty and packet scratch must cover the store's packet
    /// limit. Full is reported after all source events advance; the processor
    /// must propagate it to runtime recovery after other required processing.
    pub fn render<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        playback: Playback,
        frames: usize,
        output: &mut MidiBuffer,
    ) -> Result<RenderReport, Error> {
        self.render_range(io, playback, frames, 0, frames, output)
    }

    pub fn render_windowed<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        mut playback: Playback,
        position: i64,
        window: crate::windows::ChannelWindow,
        lead: bool,
        frames: usize,
        output: &mut MidiBuffer,
    ) -> Result<RenderReport, Error> {
        let raw_frames = playback
            .source
            .map(|index| io.snapshot(index).map(|s| s.description.frames))
            .transpose()?
            .unwrap_or(0);
        let span = crate::windows::span(position, window, lead, frames, raw_frames);
        playback.position = span.position;
        if span.frames == 0 {
            playback.source = None;
        }
        if let Some(stream) = self.stream {
            playback.discontinuity |=
                playback.source != Some(stream.index) || stream.end_position != span.position;
        }
        self.render_range(io, playback, frames, span.offset, span.frames, output)
    }

    fn render_range<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        playback: Playback,
        frames: usize,
        offset: usize,
        active_frames: usize,
        output: &mut MidiBuffer,
    ) -> Result<RenderReport, Error> {
        if frames == 0 {
            return Ok(RenderReport::default());
        }
        if frames > self.max_frames {
            return Err(Error::InvalidFrame);
        }
        if !output.is_empty() {
            return Err(Error::InvalidState);
        }
        let mut report = RenderReport::default();
        let Some(index) = playback.source else {
            if self.active {
                report.emission(midi_restore::cleanup(&self.state, |packet| {
                    output.push(offset.min(frames.saturating_sub(1)) as u32, frames, packet)
                }));
                *self.state = MidiState::default();
            }
            self.stream = None;
            self.active = false;
            return Ok(report);
        };
        let snapshot = io.snapshot(index)?;
        let description = snapshot.description;
        if description.format != Format::Midi {
            return Err(Error::InvalidShape);
        }
        let end = playback
            .position
            .checked_add(active_frames as u64)
            .ok_or(Error::InvalidFrame)?;
        if end > description.frames {
            return Err(Error::InvalidFrame);
        }
        let mut stream = if playback.discontinuity || self.stream.is_none() {
            let position = io.midi_seek(index, playback.position)?;
            if self.active {
                report.emission(midi_restore::cleanup(&self.state, |packet| {
                    output.push(offset.min(frames.saturating_sub(1)) as u32, frames, packet)
                }));
            }
            let prior_error = report.output_error;
            report.emission(midi_restore::restore(
                &position.state,
                |packet| match prior_error {
                    Some(error) => Err(error),
                    None => {
                        output.push(offset.min(frames.saturating_sub(1)) as u32, frames, packet)
                    }
                },
            ));
            *self.state = position.state;
            self.active = true;
            Stream {
                index,
                snapshot,
                cursor: position.cursor,
                next: None,
                end_position: playback.position,
            }
        } else {
            let mut stream = self.stream.expect("active stream");
            if stream.index != index {
                let old = stream.snapshot;
                if old.resource != snapshot.resource
                    || old.revision != snapshot.revision
                    || old.description != snapshot.description
                    || old.cut_frame != snapshot.cut_frame
                    || old.capture != snapshot.capture
                {
                    return Err(Error::InvalidState);
                }
                let offset = stream
                    .next
                    .map_or(stream.cursor.byte_position(), MidiEvent::byte_position);
                stream.cursor = io.midi_cursor_at(index, offset)?;
                stream.index = index;
                stream.snapshot = snapshot;
                stream.next = None;
            }
            if stream.end_position != playback.position {
                return Err(Error::InvalidFrame);
            }
            stream
        };
        // A source/read fault invalidates the cache; a later call must position
        // afresh. No rollback of an enclosing runtime callback is implied.
        self.stream = None;
        loop {
            if stream.next.is_none() {
                stream.next = io.midi_next(&mut stream.cursor)?;
            }
            let Some(event) = stream.next else {
                break;
            };
            if event.frame() >= end {
                break;
            }
            if event.frame() < playback.position {
                return Err(Error::InvalidFrame);
            }
            let bytes = io.midi_read(event, &mut self.packet)?;
            let packet = &self.packet[..bytes];
            self.state.apply(Message::parse(packet, self.packet.len())?);
            report.packet(
                output,
                (offset as u64 + event.frame() - playback.position) as u32,
                frames,
                packet,
            );
            stream.next = None;
        }
        stream.end_position = end;
        self.stream = Some(stream);
        if offset + active_frames < frames && self.active {
            report.emission(midi_restore::cleanup(&self.state, |packet| {
                output.push((offset + active_frames) as u32, frames, packet)
            }));
            self.stream = None;
            self.active = false;
            *self.state = MidiState::default();
        }
        report.parameter_history_untracked |= (0..16).any(|channel| {
            self.state
                .channel(channel)
                .expect("MIDI channel")
                .parameter_history_untracked()
        });
        Ok(report)
    }
}

pub struct MidiCapture {
    max_frames: usize,
    max_events: usize,
    max_bytes: usize,
    state: Box<MidiState>,
    state_dirty: bool,
}
impl MidiCapture {
    /// Observe input before storing it. On a storage refusal the tracker stays
    /// advanced; resume history with an explicit discontinuity and this state.
    pub fn append_history<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        history: rt_runtime::resource::MidiStoreId,
        frame: rt_runtime::Frame,
        frames: usize,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        self.process(io, input, frames, None)?;
        io.append_midi_history(history, frame, frames, input)
    }
    pub fn prepare(max_frames: usize, max_events: usize, max_bytes: usize) -> Result<Self, Error> {
        dimensions(max_frames, max_bytes)?;
        max_events
            .checked_add(ResetState::EVENTS)
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(Error::InvalidConfig)?;
        max_bytes
            .checked_add(ResetState::BYTES)
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(Error::InvalidConfig)?;
        Ok(Self {
            max_frames,
            max_events,
            max_bytes,
            state: prepared_state(),
            state_dirty: false,
        })
    }
    /// Copy at the capture-start boundary before consuming that interval's input.
    pub fn state(&self) -> &MidiState {
        &self.state
    }
    pub(crate) fn reset(&mut self) {
        *self.state = MidiState::default();
        self.state_dirty = true;
    }
    pub(crate) fn publish_state(&mut self, output: &mut MidiState) {
        if self.state_dirty {
            *output = *self.state;
            self.state_dirty = false;
        }
    }
    /// Track connected input even while not recording. Invalid input leaves the
    /// tracker unchanged; a later staging refusal does not undo observed input.
    pub fn process<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        input: &MidiBuffer,
        frames: usize,
        capture: Option<(CaptureTickId, usize)>,
    ) -> Result<(), Error> {
        if frames == 0 {
            return Ok(());
        }
        if frames > self.max_frames {
            return Err(Error::InvalidFrame);
        }
        let (events, bytes) = if input.is_recovery() {
            (ResetState::EVENTS, ResetState::BYTES)
        } else {
            (self.max_events, self.max_bytes)
        };
        if input.len() > events || input.byte_len() > bytes {
            return Err(Error::Capacity);
        }
        if !input.is_empty() {
            let mut next = *self.state;
            for index in 0..input.len() {
                let (offset, packet) = input.get(index).ok_or(Error::InvalidEvent)?;
                if offset as usize >= frames {
                    return Err(Error::InvalidFrame);
                }
                next.apply(Message::parse(packet, self.max_bytes)?);
            }
            *self.state = next;
            self.state_dirty = true;
        }
        if let Some((interval, member)) = capture {
            io.stage_midi(interval, member, input)?;
        }
        Ok(())
    }
}
