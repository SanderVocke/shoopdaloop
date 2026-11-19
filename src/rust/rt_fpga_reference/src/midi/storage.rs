use super::{ContentStore, Error, Message, MidiState, SnapshotId, StoreLimits, ViewId};

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
    store: ContentStore,
    view: Option<ViewId>,
    initial: SnapshotId,
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
        let mut store = ContentStore::prepare(StoreLimits {
            block_events: events,
            blocks: 1,
            max_events: events,
            views: 1,
            snapshots: 2,
            retirements: 1,
        })?;
        let initial = store.snapshot(&MidiState::default())?;
        Ok(Self {
            store,
            view: None,
            initial,
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
        let next = self.store.snapshot(initial)?;
        if let Some(view) = self.view.take() {
            self.store.release(view)?;
            self.store.maintenance(2);
        }
        self.store.release_snapshot(self.initial)?;
        self.initial = next;
        self.view = Some(self.store.writer(next)?);
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
            if let Err(error) = self.store.append(
                self.view.ok_or(Error::InvalidState)?,
                self.frames + u64::from(event.offset - base),
                event.message,
            ) {
                self.status = TakeStatus::Incomplete;
                return Err(error);
            }
        }
        self.store
            .advance(self.view.ok_or(Error::InvalidState)?, end)?;
        self.frames = end;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        match self.status {
            TakeStatus::Recording => {
                self.store
                    .finish(self.view.ok_or(Error::InvalidState)?, self.frames)?;
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
        self.view.map_or(&[], |view| self.store.contiguous(view))
    }
    /// Fully accepted windows; an incomplete take may also contain a partial window prefix.
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn initial(&self) -> &MidiState {
        self.store.state(self.initial).expect("owned initial state")
    }
    pub fn status(&self) -> TakeStatus {
        self.status
    }
    pub fn prepared_bytes(&self) -> usize {
        size_of::<Self>() + self.store.prepared_bytes() - size_of::<ContentStore>()
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
        Ok(take.records().get(self.next as usize).copied())
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
