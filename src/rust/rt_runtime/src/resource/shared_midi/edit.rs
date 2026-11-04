use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditId(pub(super) Handle);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditStatus {
    Building,
    Ready,
}

#[derive(Clone, Copy)]
pub(super) struct Edit {
    pub(super) target: Option<Handle>,
    pub(super) source: Option<Handle>,
    pub(super) expected: Revision,
    pub(super) view: View,
    pub(super) original: Point,
    pub(super) serial: u64,
    pub(super) records: [Record; EVENTS_PER_CHUNK],
    pub(super) count: usize,
    pub(super) input_count: usize,
    pub(super) input: usize,
    pub(super) position: u64,
    pub(super) frames: u64,
    pub(super) ready: bool,
    pub(super) retiring: bool,
    pub(super) base: Option<usize>,
    pub(super) import_cursor: Option<crate::resource::MidiCursor>,
    pub(super) credit: usize,
}

impl MidiStore {
    pub(super) fn edit(&self, id: EditId) -> Result<Edit, Error> {
        if id.0.scope != self.scope {
            return Err(Error::InvalidId);
        }
        get(&self.edits, id.0.slot, id.0.generation).and_then(|e| {
            if e.retiring {
                Err(Error::InvalidId)
            } else {
                Ok(e)
            }
        })
    }
    pub fn edit_status(&self, id: EditId) -> Result<EditStatus, Error> {
        self.edit(id).map(|e| {
            if e.ready {
                EditStatus::Ready
            } else {
                EditStatus::Building
            }
        })
    }
    /// Own the input now; preparation never borrows a future callback's buffer.
    /// `input_initial` is the projection strictly before this input interval.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_replace(
        &mut self,
        io: &mut RenderIo<'_>,
        take: TakeId,
        expected: Revision,
        position: u64,
        frames: usize,
        input: &MidiBuffer,
        input_initial: &MidiState,
    ) -> Result<EditId, Error> {
        self.check(io.store)?;
        let view = self.view(take.0)?;
        if !view.writable {
            return Err(Error::InvalidId);
        }
        if expected != view.revision {
            return Err(Error::StaleRevision);
        }
        let frames = u64::try_from(frames).map_err(|_| Error::InvalidFrame)?;
        if position >= view.frames || frames > view.frames {
            return Err(Error::InvalidFrame);
        }
        if input.len() > self.config.max_events {
            return Err(Error::Capacity);
        }
        let end = position.checked_add(frames).ok_or(Error::Exhausted)?;
        let wraps = end > view.frames;
        let touches_zero = frames > 0 && (position == 0 || wraps);
        let zero = if position == 0 {
            0
        } else {
            view.frames - position
        };
        let mut bytes = 0usize;
        let mut previous = 0;
        for event in 0..input.len() {
            let (offset, data) = input.get(event).ok_or(Error::InvalidEvent)?;
            if u64::from(offset) >= frames || (event > 0 && offset < previous) {
                return Err(Error::InvalidFrame);
            }
            Message::parse(data, self.config.max_packet)?;
            bytes = bytes.checked_add(data.len()).ok_or(Error::Capacity)?;
            previous = offset;
        }
        if bytes > self.config.max_bytes {
            return Err(Error::Capacity);
        }
        let initial = if touches_zero {
            let mut state = *input_initial;
            for event in 0..input.len() {
                let (offset, data) = input.get(event).expect("validated input");
                if u64::from(offset) >= zero {
                    break;
                }
                state.apply_packet(data, self.config.max_packet)?;
            }
            state
        } else {
            self.initial[take.0.slot as usize]
        };
        let mut start = if touches_zero {
            view.first
        } else {
            self.locate(view, view.origin + position)?
        };
        if start.serial > view.first.serial {
            let previous =
                self.chunks[self.find(view.root, start.serial - 1)?].expect("prefix chunk");
            if previous.count < EVENTS_PER_CHUNK {
                start = Point {
                    serial: start.serial - 1,
                    event: 0,
                };
            }
        }
        start.event = if start.serial == view.first.serial {
            view.first.event
        } else {
            0
        };
        let removed = if wraps {
            let a = self.locate(view, view.origin + position)?;
            let b = self.locate(view, view.origin + end - view.frames)?;
            self.between(view.root, a, view.last)?.1 + self.between(view.root, view.first, b)?.1
        } else {
            let a = self.locate(view, view.origin + position)?;
            let b = self.locate(view, view.origin + end)?;
            self.between(view.root, a, b)?.1
        };
        let suffix = self.between(view.root, start, view.last)?.1;
        let output = suffix
            .checked_sub(removed)
            .and_then(|n| n.checked_add(input.len()))
            .ok_or(Error::Capacity)?;
        let prefix =
            usize::try_from(start.serial - view.first.serial).map_err(|_| Error::Capacity)?;
        if prefix
            .checked_add(output.div_ceil(EVENTS_PER_CHUNK))
            .is_none_or(|n| n > self.config.chunks)
        {
            return Err(Error::Capacity);
        }
        start
            .serial
            .checked_add(output.div_ceil(EVENTS_PER_CHUNK) as u64)
            .ok_or(Error::Exhausted)?;
        let revision = if frames == 0 {
            expected
        } else {
            expected.checked_next()?
        };
        let credit = if frames == 0 {
            view.credit
        } else {
            (prefix + output.div_ceil(EVENTS_PER_CHUNK)).max(1)
        };
        let reserved = credit.max(view.credit);
        let needed = bytes.div_ceil(self.page_bytes) + input.len();
        if self.free_edits.is_empty()
            || self.free_views.is_empty()
            || view
                .credit
                .checked_add(reserved)
                .is_none_or(|n| n > self.debt_limit - self.debt)
            || self.pages.available_leases() < needed
            || self.reserved < needed
        {
            return Err(Error::Capacity);
        }
        let state = if frames == 0 || start == view.first {
            initial
        } else if start == view.last {
            self.state_at(io.store, view, view.origin + view.frames)?.0
        } else {
            self.chunk_states[self.find(view.root, start.serial)?]
        };
        let source_state = self.initial[take.0.slot as usize];
        let source = self.own(view, &source_state, false)?;
        self.retain(view.root).expect("prepared root references");
        let slot = self.free_edits.pop().expect("prepared transaction");
        self.debt += reserved;
        self.owners += 1;
        let mut staged = view;
        staged.revision = revision;
        staged.credit = credit;
        if prefix == 0 && frames > 0 {
            staged.first = Point {
                serial: start.serial,
                event: 0,
            };
        }
        self.edit_states[slot * 3] = initial;
        self.edit_states[slot * 3 + 1] = state;
        self.edit_states[slot * 3 + 2] = state;
        self.work.add(
            0,
            if frames > 0 && start == view.last && start != view.first {
                5
            } else {
                6
            },
            0,
            0,
        );
        let mut count = 0;
        // Across a wrap, the later input portion is first in logical loop order.
        for pass in 0..if wraps { 2 } else { 1 } {
            for event in 0..input.len() {
                let (offset, data) = input.get(event).expect("validated input");
                if wraps && ((u64::from(offset) >= zero) != (pass == 0)) {
                    continue;
                }
                let block = self.block_from(io.store, data);
                self.blocks[block].as_mut().expect("owned input").references = 1;
                self.edit_records[slot * self.config.max_events + count] = Record {
                    frame: view.origin + (position + u64::from(offset)) % view.frames,
                    block,
                    offset: 0,
                    len: data.len(),
                };
                count += 1;
            }
        }
        self.edits[slot].value = Some(Edit {
            target: Some(take.0),
            source: Some(source),
            expected,
            view: staged,
            original: start,
            serial: start.serial,
            records: [Record::default(); EVENTS_PER_CHUNK],
            count: 0,
            input_count: count,
            input: 0,
            position,
            frames,
            ready: frames == 0,
            retiring: false,
            base: None,
            import_cursor: None,
            credit: reserved,
        });
        io.store
            .charge_work(WorkClass::Render, 4 * size_of::<MidiState>(), input.len())?;
        Ok(EditId(Handle {
            scope: self.scope,
            slot: slot as u32,
            generation: self.edits[slot].generation,
        }))
    }
    fn overwritten(edit: Edit, frame: u64) -> bool {
        let end = edit.position + edit.frames;
        if end > edit.view.frames {
            frame >= edit.position || frame < end - edit.view.frames
        } else {
            frame >= edit.position && frame < end
        }
    }
    fn advance_original(&self, source: View, point: Point) -> Result<Point, Error> {
        let chunk = self.chunks[self.find(source.root, point.serial)?].expect("source chunk");
        Ok(if point.event + 1 == chunk.count {
            Point {
                serial: point.serial + 1,
                event: 0,
            }
        } else {
            Point {
                serial: point.serial,
                event: point.event + 1,
            }
        })
    }
    pub(super) fn flush_edit(&mut self, store: &mut ResourceStore, slot: usize, edit: &mut Edit) {
        self.records = edit.records;
        let checkpoint = self.edit_states[slot * 3 + 2];
        self.publish_chunk(
            store,
            &mut edit.view.root,
            edit.serial,
            edit.count,
            &checkpoint,
        );
        edit.serial += 1;
        edit.count = 0;
        self.edit_states[slot * 3 + 2] = self.edit_states[slot * 3 + 1];
        self.work.add(0, 2, 0, 0);
    }
    /// Reserved preparation work, independent of ordinary background allowances.
    /// Each quantum consumes at most one source or inserted event, plus at most
    /// one fixed-size chunk/index publication. Zero work never advances readiness.
    pub fn progress_edit(
        &mut self,
        io: &mut RenderIo<'_>,
        id: EditId,
        quantum: usize,
    ) -> Result<EditStatus, Error> {
        self.check(io.store)?;
        if quantum > self.config.edit_steps {
            return Err(Error::Capacity);
        }
        let mut edit = self.edit(id)?;
        let slot = id.0.slot as usize;
        if edit.ready || quantum == 0 {
            return self.edit_status(id);
        }
        if edit.base.is_some() {
            return self.progress_import(io, id, quantum);
        }
        let source = self.view(edit.source.ok_or(Error::InvalidState)?)?;
        for _ in 0..quantum {
            let original = if edit.original == source.last {
                None
            } else {
                Some(self.record(source, edit.original)?)
            };
            if let Some(record) = original {
                if Self::overwritten(edit, record.frame - source.origin) {
                    edit.original = self.advance_original(source, edit.original)?;
                    self.work.add(1, 0, 0, 0);
                    continue;
                }
            }
            let incoming = if edit.input < edit.input_count {
                Some(self.edit_records[slot * self.config.max_events + edit.input])
            } else {
                None
            };
            let record = match (original, incoming) {
                (None, None) => break,
                (Some(a), Some(b)) if a.frame <= b.frame => {
                    edit.original = self.advance_original(source, edit.original)?;
                    a
                }
                (Some(a), None) => {
                    edit.original = self.advance_original(source, edit.original)?;
                    a
                }
                (_, Some(b)) => {
                    edit.input += 1;
                    b
                }
            };
            edit.records[edit.count] = record;
            edit.count += 1;
            let mut state = self.edit_states[slot * 3 + 1];
            self.apply_record(io.store, &mut state, record)?;
            self.edit_states[slot * 3 + 1] = state;
            self.work.add(0, 2, 0, 0);
            if edit.count == EVENTS_PER_CHUNK {
                self.flush_edit(io.store, slot, &mut edit);
            }
        }
        if edit.original == source.last && edit.input == edit.input_count {
            if edit.count > 0 {
                self.flush_edit(io.store, slot, &mut edit);
            }
            edit.view.last = Point {
                serial: edit.serial,
                event: 0,
            };
            let (bytes, events) = self.between(edit.view.root, edit.view.first, edit.view.last)?;
            edit.view.bytes = bytes;
            edit.view.events = events;
            self.edits[slot].value = Some(edit);
            let clipped = self.clipped_root(io, edit.view)?;
            self.release_now(io.store, edit.view.root);
            edit.view.root = clipped;
            edit.ready = true;
        }
        self.edits[slot].value = Some(edit);
        self.edit_status(id)
    }
    pub(super) fn retire_edit(&mut self, id: EditId, mut edit: Edit, root: Option<usize>) {
        let slot = id.0.slot as usize;
        edit.retiring = true;
        edit.view.root = None;
        self.edits[slot].value = Some(edit);
        let owner = self.views.len() + 2 + slot;
        if edit.base.is_some() {
            for record in &edit.records[..edit.count] {
                let block = self.blocks[record.block]
                    .as_mut()
                    .expect("owned private import");
                block.references -= 1;
                if block.references == 0 {
                    self.enqueue(Garbage::Block(record.block, 0), owner, false);
                }
            }
        }
        self.drop_root(root, owner, false);
        self.enqueue(Garbage::Edit(slot, 0), owner, false);
        if let Some(source) = edit.source {
            self.release_view(source).expect("owned source pin");
        }
    }
    pub fn cancel_edit(&mut self, id: EditId) -> Result<(), Error> {
        let edit = self.edit(id)?;
        self.retire_edit(id, edit, edit.view.root);
        Ok(())
    }
    pub fn commit_edit(&mut self, io: &mut RenderIo<'_>, id: EditId) -> Result<Revision, Error> {
        self.check(io.store)?;
        let mut edit = self.edit(id)?;
        if !edit.ready {
            return Err(Error::Busy);
        }
        let target = edit.target.ok_or(Error::InvalidState)?;
        let old = self.view(target)?;
        if old.revision != edit.expected {
            return Err(Error::StaleRevision);
        }
        self.views[target.slot as usize].value = Some(edit.view);
        self.debt -= edit.credit - edit.view.credit;
        edit.credit = old.credit;
        self.initial[target.slot as usize] = self.edit_states[id.0.slot as usize * 3];
        self.work.add(0, 1, 0, 0);
        self.retire_edit(id, edit, old.root);
        io.store
            .charge_work(WorkClass::Render, size_of::<MidiState>(), 1)?;
        Ok(edit.view.revision)
    }
}
