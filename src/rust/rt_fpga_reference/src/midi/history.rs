use super::{
    ComparisonId, ContentStore, DifferenceService, Differences, Error, Interpreter, Message,
    MidiState, SnapshotId, ViewId, STATE_CELLS,
};
use alloc::collections::VecDeque;

#[derive(Clone, Copy)]
struct Checkpoint {
    order: u32,
    state: SnapshotId,
}
pub(super) struct History {
    pub writer: ViewId,
    checkpoints: VecDeque<Checkpoint>,
    changed: Differences,
    prefix: u32,
    next_order: u32,
    retention: u64,
    pub earliest: u64,
    pub frozen: bool,
    pub replay_work: u64,
}
impl History {
    pub fn reserved_snapshots(&self, max_events: u32) -> u32 {
        (max_events / 32 + 2).saturating_sub(self.checkpoints.len() as u32)
    }
    pub fn prepare(
        store: &mut ContentStore,
        differences: &mut DifferenceService,
        destinations: &[Interpreter],
        retention: u64,
    ) -> Result<Self, Error> {
        let initial = store.snapshot(&MidiState::default())?;
        let writer = store.writer(initial)?;
        for (destination, current) in destinations.iter().enumerate() {
            differences.initialize(
                store,
                ComparisonId {
                    target: initial,
                    destination: destination as u16,
                },
                current.state()?,
                STATE_CELLS as u32,
            )?;
        }
        let mut checkpoints = VecDeque::new();
        checkpoints
            .try_reserve(store.limits().snapshots as usize)
            .map_err(|_| Error::Capacity)?;
        checkpoints.push_back(Checkpoint {
            order: 0,
            state: initial,
        });
        Ok(Self {
            writer,
            checkpoints,
            changed: Differences::default(),
            prefix: 0,
            next_order: 0,
            retention,
            earliest: 0,
            frozen: false,
            replay_work: 0,
        })
    }
    pub fn prepared_bytes(&self) -> usize {
        size_of::<Self>() + self.checkpoints.capacity() * size_of::<Checkpoint>()
    }
    fn expensive(message: Message) -> bool {
        let bytes = message.as_bytes();
        bytes[0] == 0xff
            || (bytes[0] & 0xf0 == 0xb0 && matches!(bytes[1], 64 | 66 | 69 | 120 | 121 | 123..=127))
    }
    pub fn observe(
        &mut self,
        store: &mut ContentStore,
        maps: &mut DifferenceService,
        destinations: &[Interpreter],
        input: &mut Interpreter,
        frame: u64,
        message: Message,
    ) -> Result<(), Error> {
        let checkpoint = self.prefix == 31 || Self::expensive(message);
        let can_capture = !self.frozen
            && store.can_append(self.writer)?
            && (!checkpoint || store.usage().snapshots < store.limits().snapshots as usize);
        let appended = if can_capture {
            store.append(self.writer, frame, message)
        } else {
            Err(Error::Capacity)
        };
        input.begin(message)?;
        while !input.step_observed(|_, cell| self.changed.set(cell, true)) {}
        if let Err(error) = appended {
            self.frozen = true;
            return Err(error);
        }
        self.next_order = self.next_order.checked_add(1).ok_or(Error::Overflow)?;
        self.prefix += 1;
        if checkpoint {
            let state = store.snapshot(input.state()?)?;
            if let Err(error) = maps.derive(
                store,
                state,
                self.checkpoints.back().unwrap().state,
                &self.changed,
                destinations,
            ) {
                store.release_snapshot(state)?;
                self.frozen = true;
                return Err(error);
            }
            self.checkpoints.push_back(Checkpoint {
                order: self.next_order,
                state,
            });
            self.changed = Differences::default();
            self.prefix = 0;
        }
        Ok(())
    }
    pub fn advance(&mut self, store: &mut ContentStore, end: u64) -> Result<(), Error> {
        if self.frozen {
            return Ok(());
        }
        store.advance(self.writer, end)?;
        self.earliest = end.saturating_sub(self.retention);
        let index = store.lower_bound(self.writer, self.earliest)?;
        let order = store
            .read(self.writer, index)?
            .map_or(self.next_order, |r| r.order);
        while self.checkpoints.len() > 1 && self.checkpoints[1].order <= order {
            let old = self.checkpoints.pop_front().unwrap();
            store.release_snapshot(old.state)?;
        }
        let first = store
            .read(self.writer, 0)?
            .map_or(self.next_order, |r| r.order);
        let discard = self
            .checkpoints
            .front()
            .unwrap()
            .order
            .saturating_sub(first);
        if discard != 0 {
            store.discard_events(self.writer, discard)?;
        }
        Ok(())
    }
    pub fn state_at(
        &mut self,
        store: &mut ContentStore,
        maps: &mut DifferenceService,
        destinations: &[Interpreter],
        frame: u64,
    ) -> Result<SnapshotId, Error> {
        let end = store.describe(self.writer)?.frames;
        if frame < self.earliest || frame > end {
            return Err(Error::InvalidEvent);
        }
        let index = store.lower_bound(self.writer, frame)?;
        let order = store
            .read(self.writer, index)?
            .map_or(self.next_order, |r| r.order);
        let mut low = 0;
        let mut high = self.checkpoints.len();
        while low + 1 < high {
            let mid = (low + high) / 2;
            if self.checkpoints[mid].order <= order {
                low = mid;
            } else {
                high = mid;
            }
        }
        let checkpoint = self.checkpoints[low];
        let mut state = Interpreter::new(*store.state(checkpoint.state)?);
        let mut changed = Differences::default();
        let first = store
            .read(self.writer, 0)?
            .map_or(self.next_order, |r| r.order);
        let begin = checkpoint
            .order
            .checked_sub(first)
            .ok_or(Error::InvalidState)?;
        if index - begin > 31 {
            return Err(Error::InvalidState);
        }
        for i in begin..index {
            let event = store.read(self.writer, i)?.ok_or(Error::InvalidState)?;
            let before = state.steps();
            state.begin(event.message)?;
            while !state.step_observed(|_, cell| changed.set(cell, true)) {}
            self.replay_work = self.replay_work.saturating_add(state.steps() - before);
        }
        let target = store.snapshot(state.state()?)?;
        if let Err(error) = maps.derive(store, target, checkpoint.state, &changed, destinations) {
            store.release_snapshot(target)?;
            return Err(error);
        }
        Ok(target)
    }
    pub fn range(
        &mut self,
        store: &mut ContentStore,
        maps: &mut DifferenceService,
        destinations: &[Interpreter],
        start: u64,
        end: u64,
    ) -> Result<ViewId, Error> {
        if start >= end || end > store.describe(self.writer)?.frames {
            return Err(Error::InvalidEvent);
        }
        let state = self.state_at(store, maps, destinations, start)?;
        let result = store.range(self.writer, start, end, state);
        store.release_snapshot(state)?;
        result
    }
}
