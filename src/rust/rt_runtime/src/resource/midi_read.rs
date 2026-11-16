//! Lease-scoped MIDI records and bounded checkpoint-based positioning.
use super::*;
use crate::midi_state::MidiState;
use crate::Sample;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiCursor {
    index: ReadIndex,
    offset: usize,
}
impl MidiCursor {
    pub fn byte_position(self) -> usize {
        self.offset
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiEvent {
    index: ReadIndex,
    frame: u64,
    offset: usize,
    bytes: usize,
}
impl MidiEvent {
    pub fn frame(self) -> u64 {
        self.frame
    }
    pub fn byte_len(self) -> usize {
        self.bytes
    }
    /// Header position of this validated record in its immutable MIDI prefix.
    pub fn byte_position(self) -> usize {
        self.offset - 12
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiPosition {
    pub cursor: MidiCursor,
    pub state: MidiState,
    pub replayed_events: usize,
}

impl<S: Sample> ResourceStore<S> {
    fn midi_initial_channel_impl(
        &mut self,
        index: ReadIndex,
        channel: u8,
    ) -> Result<(crate::midi_state::ChannelState, bool), Error> {
        let lease = self.midi_lease(index)?;
        let select = |state: &MidiState| {
            Ok((
                *state.channel(channel).ok_or(Error::InvalidId)?,
                state.system_reset_seen(),
            ))
        };
        if let Some(midi) = lease.indexed_midi {
            return self.with_indexed_midi(midi.store, |store, pool| {
                let state =
                    pool.core
                        .state(&mut store.render_io(), pool.views.get(midi.media)?, 0)?;
                select(&state)
            });
        }
        if let Some(midi) = lease.shared_midi {
            return self.with_midi(midi.store, |_, store| {
                select(store.initial_state(midi.index)?)
            });
        }
        let version = get(&self.versions, lease.version.slot, lease.version.generation)?;
        select(self.midi_indexes.initial_state(
            version.midi.ok_or(Error::InvalidState)?,
            lease.version,
            lease.snapshot.description.bytes,
        )?)
    }
    pub(super) fn midi_lease(&self, index: ReadIndex) -> Result<Lease, Error> {
        let lease = self.index_lease(index)?;
        if lease.snapshot.description.format != Format::Midi {
            return Err(Error::InvalidShape);
        }
        Ok(lease)
    }
    fn read_exact_indexed(
        &mut self,
        index: ReadIndex,
        offset: usize,
        output: &mut [u8],
    ) -> Result<(), Error> {
        let lease = self.index_lease(index)?;
        if offset
            .checked_add(output.len())
            .is_none_or(|end| end > lease.snapshot.description.bytes)
        {
            return Err(Error::InvalidOffset);
        }
        let mut cursor = self.cursor_at_with(index, offset, WorkClass::Render)?;
        let mut written = 0;
        while written < output.len() {
            let count = self.read_with(&mut cursor, &mut output[written..], WorkClass::Render)?;
            if count == 0 {
                return Err(Error::InvalidState);
            }
            written += count;
        }
        Ok(())
    }
    pub(super) fn midi_cursor_impl(&self, index: ReadIndex) -> Result<MidiCursor, Error> {
        self.midi_cursor_at_impl(index, 0)
    }
    fn midi_cursor_at_impl(&self, index: ReadIndex, offset: usize) -> Result<MidiCursor, Error> {
        let lease = self.midi_lease(index)?;
        if offset > lease.snapshot.description.bytes {
            return Err(Error::InvalidOffset);
        }
        Ok(MidiCursor { index, offset })
    }
    pub(super) fn midi_next_impl(
        &mut self,
        cursor: &mut MidiCursor,
    ) -> Result<Option<MidiEvent>, Error> {
        let lease = self.midi_lease(cursor.index)?;
        if cursor.offset == lease.snapshot.description.bytes {
            return Ok(None);
        }
        let mut header = [0; 12];
        self.read_exact_indexed(cursor.index, cursor.offset, &mut header)?;
        let frame = u64::from_le_bytes(header[..8].try_into().map_err(|_| Error::InvalidState)?);
        let bytes =
            u32::from_le_bytes(header[8..].try_into().map_err(|_| Error::InvalidState)?) as usize;
        if frame >= lease.snapshot.description.frames
            || bytes == 0
            || bytes > self.config.limits().midi_bytes_per_port
        {
            return Err(Error::InvalidEvent);
        }
        let offset = cursor.offset.checked_add(12).ok_or(Error::Capacity)?;
        let end = offset.checked_add(bytes).ok_or(Error::Capacity)?;
        if end > lease.snapshot.description.bytes {
            return Err(Error::Incomplete);
        }
        let event = MidiEvent {
            index: cursor.index,
            frame,
            offset,
            bytes,
        };
        cursor.offset = end;
        Ok(Some(event))
    }
    pub(super) fn midi_read_impl(
        &mut self,
        event: MidiEvent,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        self.midi_lease(event.index)?;
        if output.len() < event.bytes {
            return Err(Error::Capacity);
        }
        self.read_exact_indexed(event.index, event.offset, &mut output[..event.bytes])?;
        Ok(event.bytes)
    }
    fn midi_history_impl(&mut self, index: ReadIndex, frame: u64) -> Result<bool, Error> {
        let lease = self.midi_lease(index)?;
        if frame > lease.snapshot.description.frames {
            return Err(Error::InvalidFrame);
        }
        if let Some(take) = lease.indexed_midi {
            self.with_indexed_midi(take.store, |_, pool| pool.views.get(take.media).map(|_| ()))?;
            return Ok(false);
        }
        if let Some(midi) = lease.shared_midi {
            return self.with_midi(midi.store, |store, midi_store| {
                midi_store.parameter_history_untracked(&mut store.render_io(), midi.index, frame)
            });
        }
        let version = get(&self.versions, lease.version.slot, lease.version.generation)?;
        let (mut history, offset, span) = self.midi_indexes.history_before(
            version.midi.ok_or(Error::InvalidState)?,
            lease.version,
            frame,
            lease.snapshot.description.bytes,
        )?;
        let mut cursor = MidiCursor { index, offset };
        for _ in 0..span {
            let Some(event) = self.midi_next_impl(&mut cursor)? else {
                return Ok(history != 0);
            };
            if event.frame >= frame {
                return Ok(history != 0);
            }
            let mut short = [0; 3];
            let count = event.bytes.min(3);
            self.read_exact_indexed(index, event.offset, &mut short[..count])?;
            self.midi_indexes.replay_history(&mut history, short);
        }
        Err(Error::InvalidState)
    }
    pub(super) fn midi_seek_impl(
        &mut self,
        index: ReadIndex,
        frame: u64,
    ) -> Result<MidiPosition, Error> {
        let lease = self.midi_lease(index)?;
        if frame > lease.snapshot.description.frames {
            return Err(Error::InvalidFrame);
        }
        if let Some(take) = lease.indexed_midi {
            return self.with_indexed_midi(take.store, |store, pool| {
                let media = pool.views.get(take.media)?;
                let state = pool.core.state(&mut store.render_io(), media, frame)?;
                let offset = pool
                    .core
                    .byte_before(&mut store.render_io(), media, frame)?;
                Ok(MidiPosition {
                    cursor: MidiCursor { index, offset },
                    state,
                    replayed_events: 0,
                })
            });
        }
        if let Some(midi) = lease.shared_midi {
            return self.with_midi(midi.store, |store, midi_store| {
                let position = midi_store.seek(&mut store.render_io(), midi.index, frame)?;
                Ok(MidiPosition {
                    cursor: MidiCursor {
                        index,
                        offset: position.cursor.byte_position(),
                    },
                    state: position.state,
                    replayed_events: position.replayed_events,
                })
            });
        }
        let version = get(&self.versions, lease.version.slot, lease.version.generation)?;
        let (mut state, offset, span) = self.midi_indexes.before(
            version.midi.ok_or(Error::InvalidState)?,
            lease.version,
            frame,
            lease.snapshot.description.bytes,
        )?;
        let mut cursor = MidiCursor { index, offset };
        for replayed_events in 0..span {
            let before = cursor;
            let Some(event) = self.midi_next_impl(&mut cursor)? else {
                return Ok(MidiPosition {
                    cursor,
                    state,
                    replayed_events,
                });
            };
            if event.frame >= frame {
                return Ok(MidiPosition {
                    cursor: before,
                    state,
                    replayed_events,
                });
            }
            let mut short = [0; 3];
            let count = event.bytes.min(3);
            self.read_exact_indexed(index, event.offset, &mut short[..count])?;
            self.midi_indexes.replay(&mut state, short);
        }
        Err(Error::InvalidState)
    }
}
impl<S: Sample> RenderIo<'_, S> {
    pub fn publish_midi_position(
        &mut self,
        index: ReadIndex,
        frame: u64,
        state: &MidiState,
    ) -> Result<(), Error> {
        let lease = self.store.midi_lease(index)?;
        if let Some(take) = lease.indexed_midi {
            self.store.with_indexed_midi(take.store, |store, pool| {
                pool.core
                    .publish_position(pool.views.get(take.media)?, frame, state)?;
                pool.core.flush_work(&mut store.render_io())?;
                store.charge_work(WorkClass::Render, size_of::<MidiState>(), 0)
            })?;
        }
        Ok(())
    }
    /// Export one initial channel without copying all sixteen channel states.
    pub fn midi_initial_channel(
        &mut self,
        index: ReadIndex,
        channel: u8,
    ) -> Result<(crate::midi_state::ChannelState, bool), Error> {
        self.store.midi_initial_channel_impl(index, channel)
    }
    pub fn midi_cursor(&self, index: ReadIndex) -> Result<MidiCursor, Error> {
        self.store.midi_cursor_impl(index)
    }
    /// Resume at a previously obtained record boundary without replaying state.
    /// Subsequent reads still validate the lease, headers and packet bounds.
    pub fn midi_cursor_at(&self, index: ReadIndex, offset: usize) -> Result<MidiCursor, Error> {
        self.store.midi_cursor_at_impl(index, offset)
    }
    pub fn midi_next(&mut self, cursor: &mut MidiCursor) -> Result<Option<MidiEvent>, Error> {
        self.store.midi_next_impl(cursor)
    }
    pub fn midi_read(&mut self, event: MidiEvent, output: &mut [u8]) -> Result<usize, Error> {
        self.store.midi_read_impl(event, output)
    }
    /// Query the bounded indexed prefix without copying its full channel state.
    pub fn midi_parameter_history_untracked(
        &mut self,
        index: ReadIndex,
        frame: u64,
    ) -> Result<bool, Error> {
        self.store.midi_history_impl(index, frame)
    }
    pub fn midi_seek(&mut self, index: ReadIndex, frame: u64) -> Result<MidiPosition, Error> {
        self.store.midi_seek_impl(index, frame)
    }
}
impl<S: Sample> ResourceIo<'_, S> {
    pub fn midi_position_replay_bound(&mut self, index: ReadIndex) -> Result<usize, Error> {
        let lease = self.store.midi_lease(index)?;
        if let Some(take) = lease.indexed_midi {
            return self.store.with_indexed_midi(take.store, |_, pool| {
                pool.core.replay_bound(pool.views.get(take.media)?)
            });
        }
        Ok(0)
    }
    /// Prepare a cold position before an operation acquires an execution frame.
    pub fn prepare_midi_position(
        &mut self,
        index: ReadIndex,
        frame: u64,
        steps: usize,
    ) -> Result<bool, Error> {
        let lease = self.store.midi_lease(index)?;
        if frame > lease.snapshot.description.frames {
            return Err(Error::InvalidFrame);
        }
        if let Some(take) = lease.indexed_midi {
            return self.store.with_indexed_midi(take.store, |store, pool| {
                pool.core.prepare_state(
                    &mut store.render_io(),
                    pool.views.get(take.media)?,
                    frame,
                    steps,
                )
            });
        }
        Ok(true)
    }
}
