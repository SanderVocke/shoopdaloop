use super::{Error, Message, MidiState};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Event {
    pub offset: u32,
    pub message: Message,
}
impl Event {
    pub fn validate_window(events: &[Self], frames: u32) -> Result<(), Error> {
        if frames == 0
            || events.iter().any(|event| event.offset >= frames)
            || events
                .windows(2)
                .any(|pair| pair[0].offset > pair[1].offset)
        {
            return Err(Error::InvalidEvent);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Record {
    pub frame: u64,
    pub order: u32,
    pub message: Message,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TakeStatus {
    Empty,
    Recording,
    Complete,
    Incomplete,
}

pub struct Take {
    records: Vec<Record>,
    limit: u32,
    initial: MidiState,
    frames: u64,
    revision: u64,
    status: TakeStatus,
    playing: bool,
}
impl Take {
    pub fn prepare(events: u32) -> Result<Self, Error> {
        if events == 0 || (events as u64) * (size_of::<Record>() as u64) > isize::MAX as u64 {
            return Err(Error::Capacity);
        }
        let mut records = Vec::new();
        records
            .try_reserve_exact(events as usize)
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            records,
            limit: events,
            initial: MidiState::default(),
            frames: 0,
            revision: 0,
            status: TakeStatus::Empty,
            playing: false,
        })
    }
    pub fn begin(&mut self, initial: &MidiState) -> Result<(), Error> {
        if self.playing || self.status == TakeStatus::Recording {
            return Err(Error::Busy);
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Overflow)?;
        self.records.clear();
        self.initial = *initial;
        self.frames = 0;
        self.revision = revision;
        self.status = TakeStatus::Recording;
        Ok(())
    }
    pub fn append_window(&mut self, frames: u32, events: &[Event]) -> Result<(), Error> {
        Event::validate_window(events, frames)?;
        self.append_window_at(frames, 0, events)
    }
    pub(super) fn append_window_at(
        &mut self,
        frames: u32,
        base: u32,
        events: &[Event],
    ) -> Result<(), Error> {
        if self.status != TakeStatus::Recording {
            return Err(Error::InvalidState);
        }
        let end = self
            .frames
            .checked_add(u64::from(frames))
            .ok_or(Error::Overflow)?;
        for event in events {
            if self.records.len() == self.limit as usize {
                self.status = TakeStatus::Incomplete;
                return Err(Error::Capacity);
            }
            self.records.push(Record {
                frame: self.frames + u64::from(event.offset - base),
                order: self.records.len() as u32,
                message: event.message,
            });
        }
        self.frames = end;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        match self.status {
            TakeStatus::Recording => {
                self.status = TakeStatus::Complete;
                Ok(())
            }
            TakeStatus::Incomplete => Err(Error::Incomplete),
            _ => Err(Error::InvalidState),
        }
    }
    pub fn start_playback(&mut self) -> Result<Cursor, Error> {
        if self.playing {
            return Err(Error::Busy);
        }
        if self.status != TakeStatus::Complete || self.frames == 0 {
            return Err(Error::Incomplete);
        }
        self.playing = true;
        Ok(Cursor {
            revision: self.revision,
            next: 0,
        })
    }
    pub fn end_playback(&mut self, cursor: Cursor) -> Result<(), Error> {
        cursor.validate(self)?;
        self.playing = false;
        Ok(())
    }
    pub fn records(&self) -> &[Record] {
        &self.records
    }
    /// Fully accepted windows; an incomplete take may also contain a partial window prefix.
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn initial(&self) -> &MidiState {
        &self.initial
    }
    pub fn status(&self) -> TakeStatus {
        self.status
    }
    pub fn prepared_bytes(&self) -> usize {
        size_of::<Self>() + self.records.capacity() * size_of::<Record>()
    }
}

#[derive(Debug)]
pub struct Cursor {
    revision: u64,
    next: u32,
}
impl Cursor {
    fn validate(&self, take: &Take) -> Result<(), Error> {
        if take.playing && self.revision == take.revision {
            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }
    pub fn peek(&self, take: &Take) -> Result<Option<Record>, Error> {
        self.validate(take)?;
        Ok(take.records.get(self.next as usize).copied())
    }
    pub fn acknowledge(&mut self, take: &Take) -> Result<(), Error> {
        if self.peek(take)?.is_none() {
            return Err(Error::InvalidState);
        }
        self.next = self.next.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn rewind(&mut self, take: &Take) -> Result<(), Error> {
        self.validate(take)?;
        self.next = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn counters_refuse_overflow_without_mutating_the_take() {
        let mut take = Take::prepare(1).unwrap();
        take.revision = u64::MAX;
        assert_eq!(take.begin(&MidiState::default()), Err(Error::Overflow));
        assert_eq!(take.status, TakeStatus::Empty);
        take.revision = 0;
        take.begin(&MidiState::default()).unwrap();
        take.frames = u64::MAX;
        assert_eq!(take.append_window(1, &[]), Err(Error::Overflow));
        assert_eq!(take.frames, u64::MAX);
        assert_eq!(take.status, TakeStatus::Recording);
    }
}
