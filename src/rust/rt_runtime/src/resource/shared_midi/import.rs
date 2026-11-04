use super::*;
use edit::Edit;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportId(EditId);

impl MidiStore {
    pub(super) fn drop_base(
        &mut self,
        store: &mut ResourceStore,
        slot: usize,
    ) -> Result<(), Error> {
        let base = self.bases[slot].as_mut().expect("owned immutable base");
        base.references -= 1;
        if base.references == 0 {
            let base = self.bases[slot].take().expect("owned base");
            store.release_snapshot(base.snapshot.id)?;
            self.free_bases.push(slot);
        }
        Ok(())
    }
    /// Admit an ordinary immutable indexed upload. Only descriptors/checkpoints
    /// are prepared; packet bodies remain in the retained original resource.
    pub fn prepare_import(
        &mut self,
        io: &mut RenderIo<'_>,
        snapshot: crate::resource::SnapshotId,
    ) -> Result<ImportId, Error> {
        self.check(io.store)?;
        let lease = get(&io.store.snapshots, snapshot.slot, snapshot.generation)?;
        let version = get(
            &io.store.versions,
            lease.version.slot,
            lease.version.generation,
        )?;
        if lease.snapshot.description.format != crate::resource::Format::Midi
            || lease.audio.is_some()
            || lease.shared_midi.is_some()
            || version.capture.is_some()
        {
            return Err(Error::InvalidShape);
        }
        let info = io
            .store
            .midi_indexes
            .info(version.midi.ok_or(Error::InvalidState)?, lease.version)?;
        let credit = usize::try_from(info.events)
            .map_err(|_| Error::Capacity)?
            .div_ceil(EVENTS_PER_CHUNK)
            .max(1);
        if info.events > self.config.chunks as u64 * EVENTS_PER_CHUNK as u64
            || self.free_bases.is_empty()
            || self.free_edits.is_empty()
            || credit > self.debt_limit - self.debt
        {
            return Err(Error::Capacity);
        }
        let retained = io.store.retain_snapshot(snapshot)?;
        let index = io
            .store
            .read_index(retained.id)
            .expect("retained indexed base");
        let position = io
            .store
            .midi_seek_impl(index, 0)
            .expect("validated immutable MIDI initial state");
        let base = self.free_bases.pop().expect("prepared base");
        self.bases[base] = Some(Base {
            snapshot: retained,
            index,
            references: 1,
        });
        let slot = self.free_edits.pop().expect("prepared transaction");
        self.debt += credit;
        self.owners += 1;
        for state in &mut self.edit_states[slot * 3..slot * 3 + 3] {
            *state = position.state;
        }
        self.work.add(0, 3, 0, 0);
        self.edits[slot].value = Some(Edit {
            target: None,
            source: None,
            expected: Revision(0),
            view: View {
                root: None,
                first: Point::default(),
                last: Point::default(),
                origin: 0,
                frames: retained.description.frames,
                bytes: 0,
                events: 0,
                revision: Revision(0),
                writable: true,
                retiring: false,
                credit,
            },
            original: Point::default(),
            serial: 0,
            records: [Record::default(); EVENTS_PER_CHUNK],
            count: 0,
            input_count: 0,
            input: 0,
            position: 0,
            frames: 0,
            ready: false,
            retiring: false,
            base: Some(base),
            import_cursor: Some(position.cursor),
            credit,
        });
        Ok(ImportId(EditId(Handle {
            scope: self.scope,
            slot: slot as u32,
            generation: self.edits[slot].generation,
        })))
    }
    fn flush_import(&mut self, store: &mut ResourceStore, slot: usize, edit: &mut Edit) {
        let records = edit.records;
        let count = edit.count;
        self.flush_edit(store, slot, edit);
        for record in &records[..count] {
            self.blocks[record.block]
                .as_mut()
                .expect("published base reference")
                .references -= 1;
        }
    }
    pub(super) fn progress_import(
        &mut self,
        io: &mut RenderIo<'_>,
        id: EditId,
        quantum: usize,
    ) -> Result<EditStatus, Error> {
        let mut edit = self.edit(id)?;
        let slot = id.0.slot as usize;
        let base = edit.base.ok_or(Error::InvalidState)?;
        let mut cursor = edit.import_cursor.ok_or(Error::InvalidState)?;
        for _ in 0..quantum {
            let event = io.store.midi_next_impl(&mut cursor)?;
            let Some(event) = event else {
                if edit.count > 0 {
                    self.flush_import(io.store, slot, &mut edit);
                }
                edit.view.last = Point {
                    serial: edit.serial,
                    event: 0,
                };
                let (bytes, events) =
                    self.between(edit.view.root, edit.view.first, edit.view.last)?;
                edit.view.bytes = bytes;
                edit.view.events = events;
                edit.ready = true;
                self.edits[slot].value = Some(edit);
                return Ok(EditStatus::Ready);
            };
            if event.byte_len() > self.config.max_packet {
                return Err(Error::Capacity);
            }
            let block = self.free_blocks.pop().expect("prepared base descriptors");
            self.blocks[block] = Some(Block {
                references: 1,
                bytes: event.byte_len(),
                pages: 0,
                base: Some((base, cursor.byte_position() - event.byte_len())),
            });
            self.bases[base].as_mut().expect("owned base").references += 1;
            let record = Record {
                frame: event.frame(),
                block,
                offset: 0,
                len: event.byte_len(),
            };
            edit.records[edit.count] = record;
            edit.count += 1;
            let mut state = self.edit_states[slot * 3 + 1];
            self.apply_record(io.store, &mut state, record)
                .expect("validated immutable packet prefix");
            self.edit_states[slot * 3 + 1] = state;
            self.work.add(0, 2, 0, 0);
            if edit.count == EVENTS_PER_CHUNK {
                self.flush_import(io.store, slot, &mut edit);
            }
            edit.import_cursor = Some(cursor);
            self.edits[slot].value = Some(edit);
        }
        Ok(EditStatus::Building)
    }
    pub fn import_status(&self, id: ImportId) -> Result<EditStatus, Error> {
        self.edit_status(id.0)
    }
    pub fn progress_imported(
        &mut self,
        io: &mut RenderIo<'_>,
        id: ImportId,
        quantum: usize,
    ) -> Result<EditStatus, Error> {
        self.progress_edit(io, id.0, quantum)
    }
    pub fn cancel_import(&mut self, id: ImportId) -> Result<(), Error> {
        self.cancel_edit(id.0)
    }
    pub fn finish_import(&mut self, id: ImportId) -> Result<TakeId, Error> {
        let edit = self.edit(id.0)?;
        if !edit.ready {
            return Err(Error::Busy);
        }
        let initial = self.edit_states[id.0 .0.slot as usize * 3];
        let take = self.own(edit.view, &initial, true)?;
        self.retire_edit(id.0, edit, edit.view.root);
        Ok(TakeId(take))
    }
}
