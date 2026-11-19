use super::*;

impl MidiSession {
    pub(super) fn reserved_blocks(&self) -> Result<u32, Error> {
        let maximum = self
            .limits
            .store
            .max_events
            .div_ceil(self.limits.store.block_events);
        let mut reserved = 2u32;
        for h in &self.histories {
            reserved = reserved
                .saturating_add((maximum + 1).saturating_sub(self.store.view_blocks(h.writer)?));
        }
        for c in &self.contexts {
            if c.status == TakeStatus::Recording {
                if let Some(writer) = c.writer {
                    reserved = reserved
                        .saturating_add(maximum.saturating_sub(self.store.view_blocks(writer)?));
                }
            }
        }
        Ok(reserved)
    }
    pub fn begin_import(&mut self, initial: &MidiState) -> Result<ViewId, Error> {
        if self.next_frame.is_some() && self.transfer_metadata == 0 {
            return Err(Error::Busy);
        }
        let usage = self.store.usage();
        let reserved = self
            .histories
            .iter()
            .map(|h| h.reserved_snapshots(self.limits.store.max_events))
            .sum::<u32>()
            + 2;
        if self
            .limits
            .store
            .snapshots
            .saturating_sub(usage.snapshots as u32)
            <= reserved
            || self.limits.store.views.saturating_sub(usage.views as u32) <= 2
        {
            return Err(Error::Capacity);
        }
        let snapshot = self.store.snapshot(initial)?;
        let result = self.store.writer(snapshot);
        self.store.release_snapshot(snapshot)?;
        if result.is_ok() && self.next_frame.is_some() {
            self.transfer_metadata -= 1;
        }
        result
    }
    pub fn import_events(&mut self, view: ViewId, records: &[Record]) -> Result<usize, Error> {
        if records.len() > 64 {
            return Err(Error::Capacity);
        }
        for event in records {
            Message::managed(event.message.as_bytes())?;
        }
        if records.windows(2).any(|w| w[0].frame > w[1].frame) {
            return Err(Error::InvalidEvent);
        }
        let d = self.store.describe(view)?;
        if d.status != TakeStatus::Recording {
            return Err(Error::InvalidState);
        }
        let needed = (d.events as usize + records.len())
            .div_ceil(self.limits.store.block_events as usize) as u32
            - self.store.view_blocks(view)?;
        if self
            .limits
            .store
            .blocks
            .saturating_sub(self.store.usage().blocks as u32)
            < self.reserved_blocks()?.saturating_add(needed)
        {
            return Err(Error::Capacity);
        }
        if self.next_frame.is_some() {
            if records.len() as u32 > self.transfer_events {
                return Err(Error::Busy);
            }
            self.transfer_events -= records.len() as u32;
        }
        for event in records {
            self.store.append(view, event.frame, event.message)?;
        }
        Ok(records.len())
    }
    pub fn finish_import(&mut self, view: ViewId, frames: u64, budget: u32) -> Result<bool, Error> {
        if self.store.describe(view)?.status == TakeStatus::Recording {
            self.store.finish(view, frames)?;
        }
        if self.store.describe(view)?.frames != frames {
            return Err(Error::InvalidEvent);
        }
        self.prepare_content(view, budget.min(256))
    }
    pub fn export(&mut self, view: ViewId) -> Result<ViewId, Error> {
        if self.next_frame.is_some() && self.transfer_metadata == 0 {
            return Err(Error::Busy);
        }
        let result = self.store.retain(view);
        if result.is_ok() && self.next_frame.is_some() {
            self.transfer_metadata -= 1;
        }
        result
    }
    pub fn export_events(
        &mut self,
        view: ViewId,
        first: u32,
        records: &mut [Record],
    ) -> Result<usize, Error> {
        if records.len() > 64 {
            return Err(Error::Capacity);
        }
        let d = self.store.describe(view)?;
        if d.status != TakeStatus::Complete || first > d.events {
            return Err(Error::InvalidState);
        }
        let n = records.len().min((d.events - first) as usize);
        if self.next_frame.is_some() {
            if n as u32 > self.transfer_events {
                return Err(Error::Busy);
            }
            self.transfer_events -= n as u32;
        }
        for (i, dest) in records[..n].iter_mut().enumerate() {
            *dest = self.store.read(view, first + i as u32)?.unwrap();
        }
        Ok(n)
    }
    pub fn clear_group(&mut self, state: &mut StateProcessor, slots: &[u16]) -> Result<(), Error> {
        if slots.len() > self.contexts.len() || self.next_frame.is_some_and(|f| f != state.frame())
        {
            return Err(Error::InvalidState);
        }
        self.old_views.clear();
        let mut updates = [(
            Identity {
                slot: 0,
                generation: 0,
            },
            0,
        ); 128];
        for (i, &slot) in slots.iter().enumerate() {
            if slots[..i].contains(&slot) {
                return Err(Error::InvalidState);
            }
            let c = self
                .contexts
                .get(usize::from(slot))
                .ok_or(Error::InvalidState)?;
            updates[i] = (c.binding.ok_or(Error::InvalidState)?.loop_id, 0);
            if let Some(v) = c.view {
                self.old_views.push(v);
            }
            if let Some(v) = c.writer {
                self.old_views.push(v);
            }
        }
        if self.next_frame.is_some() && slots.len() > self.publication_bindings {
            return Err(Error::Busy);
        }
        state
            .validate_midi_contents(&updates[..slots.len()])
            .map_err(|_| Error::InvalidState)?;
        self.store.can_release(&self.old_views)?;
        for &slot in slots {
            let c = &mut self.contexts[usize::from(slot)];
            c.view = None;
            c.writer = None;
            c.status = TakeStatus::Empty;
            if c.mode == Mode::Recording {
                c.mode = Mode::Stopped;
            }
        }
        for &view in &self.old_views {
            self.store.release(view)?;
        }
        if self.next_frame.is_some() {
            self.publication_bindings -= slots.len();
        }
        state
            .publish_midi_contents(&updates[..slots.len()])
            .map_err(|_| Error::InvalidState)?;
        Ok(())
    }
    pub fn duplicate(
        &mut self,
        state: &mut StateProcessor,
        source: u16,
        destination: u16,
    ) -> Result<(), Error> {
        let view = self.content(source)?.ok_or(Error::Incomplete)?;
        self.replace_group(state, &[destination], view)
    }
    pub fn release(&mut self, view: ViewId) -> Result<(), Error> {
        if self.next_frame.is_some() && self.transfer_metadata == 0 {
            return Err(Error::Busy);
        }
        self.store.release(view)?;
        if self.next_frame.is_some() {
            self.transfer_metadata -= 1;
        }
        Ok(())
    }
    pub fn maintenance(&mut self, budget: u32) -> u32 {
        self.store.maintenance(budget)
    }
    pub fn history_range(&mut self, input: u16, start: u64, end: u64) -> Result<ViewId, Error> {
        self.histories
            .get_mut(usize::from(input))
            .ok_or(Error::InvalidState)?
            .range(&mut self.store, &mut self.maps, &self.outputs, start, end)
    }
    pub fn history_bounds(&self, input: u16) -> Result<(u64, u64, bool), Error> {
        let h = self
            .histories
            .get(usize::from(input))
            .ok_or(Error::InvalidState)?;
        Ok((h.earliest, self.store.describe(h.writer)?.frames, h.frozen))
    }
    pub fn prepare_content(&mut self, view: ViewId, mut budget: u32) -> Result<bool, Error> {
        let description = self.store.describe(view)?;
        if description.status != TakeStatus::Complete {
            return Err(Error::Incomplete);
        }
        if self.next_frame.is_some() {
            budget = budget.min(self.transfer_cells);
        }
        let starting_work = self.maps.work();
        let mut ready = true;
        for destination in 0..self.limits.destinations {
            let before = self.maps.work();
            let done = self.maps.initialize(
                &self.store,
                ComparisonId {
                    target: description.initial,
                    destination,
                },
                self.outputs[usize::from(destination)].state()?,
                budget,
            )?;
            budget = budget.saturating_sub((self.maps.work() - before) as u32);
            ready &= done;
        }
        if self.next_frame.is_some() {
            self.transfer_cells = self
                .transfer_cells
                .saturating_sub((self.maps.work() - starting_work) as u32);
        }
        Ok(ready)
    }
    pub fn replace_group(
        &mut self,
        state: &mut StateProcessor,
        slots: &[u16],
        view: ViewId,
    ) -> Result<(), Error> {
        if slots.is_empty()
            || slots.len() > self.contexts.len()
            || self.next_frame.is_some_and(|f| f != state.frame())
        {
            return Err(Error::InvalidState);
        }
        let description = self.store.describe(view)?;
        if description.status != TakeStatus::Complete || description.frames == 0 {
            return Err(Error::Incomplete);
        }
        self.old_views.clear();
        let mut updates = [(
            Identity {
                slot: 0,
                generation: 0,
            },
            0,
        ); 128];
        for (i, &slot) in slots.iter().enumerate() {
            if slots[..i].contains(&slot) {
                return Err(Error::InvalidState);
            }
            let c = self
                .contexts
                .get(usize::from(slot))
                .ok_or(Error::InvalidState)?;
            let b = c.binding.ok_or(Error::InvalidState)?;
            self.maps.get(
                &self.store,
                ComparisonId {
                    target: description.initial,
                    destination: b.destination,
                },
            )?;
            if self.recovery[usize::from(b.destination)] != 32 {
                return Err(Error::Busy);
            }
            updates[i] = (b.loop_id, description.frames);
            if let Some(old) = c.view {
                self.old_views.push(old);
            }
            if let Some(old) = c.writer {
                self.old_views.push(old);
            }
        }
        if self.next_frame.is_some() && slots.len() > self.publication_bindings {
            return Err(Error::Busy);
        }
        state
            .validate_midi_contents(&updates[..slots.len()])
            .map_err(|_| Error::InvalidState)?;
        self.store.can_release(&self.old_views)?;
        self.store.can_retain(view, slots.len() as u32)?;
        for &slot in slots {
            self.store.retain(view)?;
            let c = &mut self.contexts[usize::from(slot)];
            c.view = Some(view);
            c.writer = None;
            c.status = TakeStatus::Complete;
            if c.mode == Mode::Recording {
                c.mode = Mode::Stopped;
            }
        }
        for &old in &self.old_views {
            self.store.release(old)?;
        }
        if self.next_frame.is_some() {
            self.publication_bindings -= slots.len();
        }
        state
            .publish_midi_contents(&updates[..slots.len()])
            .map_err(|_| Error::InvalidState)?;
        self.work.published_bindings = self
            .work
            .published_bindings
            .saturating_add(slots.len() as u64);
        Ok(())
    }
    pub fn grab(
        &mut self,
        state: &mut StateProcessor,
        input: u16,
        start: u64,
        end: u64,
        slots: &[u16],
    ) -> Result<(), Error> {
        if self.store.usage().retirements >= self.limits.store.retirements as usize {
            return Err(Error::Capacity);
        }
        let view = self.history_range(input, start, end)?;
        let result = self.replace_group(state, slots, view);
        self.store.release(view)?;
        result
    }
}
