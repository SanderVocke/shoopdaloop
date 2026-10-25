//! Lease-scoped MIDI records and bounded checkpoint-based positioning.
use super::*;
use crate::midi_state::MidiState;

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
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiPosition {
    pub cursor: MidiCursor,
    pub state: MidiState,
    pub replayed_events: usize,
}

impl ResourceStore {
    fn midi_lease(&self, index: ReadIndex) -> Result<Lease, Error> {
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
        self.midi_lease(index)?;
        Ok(MidiCursor { index, offset: 0 })
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
    pub(super) fn midi_seek_impl(
        &mut self,
        index: ReadIndex,
        frame: u64,
    ) -> Result<MidiPosition, Error> {
        let lease = self.midi_lease(index)?;
        if frame > lease.snapshot.description.frames {
            return Err(Error::InvalidFrame);
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
impl RenderIo<'_> {
    pub fn midi_cursor(&self, index: ReadIndex) -> Result<MidiCursor, Error> {
        self.store.midi_cursor_impl(index)
    }
    pub fn midi_next(&mut self, cursor: &mut MidiCursor) -> Result<Option<MidiEvent>, Error> {
        self.store.midi_next_impl(cursor)
    }
    pub fn midi_read(&mut self, event: MidiEvent, output: &mut [u8]) -> Result<usize, Error> {
        self.store.midi_read_impl(event, output)
    }
    pub fn midi_seek(&mut self, index: ReadIndex, frame: u64) -> Result<MidiPosition, Error> {
        self.store.midi_seek_impl(index, frame)
    }
}
