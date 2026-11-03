use super::*;

impl MidiStore {
    pub fn append(
        &mut self,
        io: &mut RenderIo<'_>,
        frame: Frame,
        frames: usize,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        self.check(io.store)?;
        if frame != self.next {
            return Err(Error::InvalidFrame);
        }
        if frames > io.store.config.limits().max_frames || input.len() > self.config.max_events {
            return Err(Error::Capacity);
        }
        let next = frame.checked_add(frames)?;
        let mut bytes = 0usize;
        let mut prior = 0;
        for i in 0..input.len() {
            let (at, data) = input.get(i).ok_or(Error::InvalidEvent)?;
            if at as usize >= frames || (i > 0 && at < prior) {
                return Err(Error::InvalidFrame);
            }
            Message::parse(data, self.config.max_packet)?;
            bytes = bytes.checked_add(data.len()).ok_or(Error::Capacity)?;
            prior = at;
        }
        if bytes > self.config.max_bytes {
            return Err(Error::Capacity);
        }
        if frames == 0 {
            return Ok(());
        }
        if input.is_empty() {
            self.next = next;
            return Ok(());
        }
        let touched = input.len() + 1;
        let needed = bytes.div_ceil(self.page_bytes) + input.len() + 1;
        if self.reserved < needed
            || self.free_chunks.len() < touched
            || self.free_nodes.len() < touched * (self.depth + 1)
        {
            return Err(Error::Capacity);
        }
        self.end
            .checked_add(input.len() as u64)
            .ok_or(Error::Exhausted)?;
        io.store.note_operation(WorkClass::Render);
        let mut root = self.root;
        self.retain(root)?;
        let mut end = self.end;
        let mut state = *self.current;
        let mut checkpoint = state;
        let mut serial = end;
        let mut count = 0;
        let mut used = 0;
        if end > 0 {
            let index = self.find(root, end - 1)?;
            let chunk = self.chunks[index].expect("last chunk");
            let block = self.blocks[chunk.records[0].block].expect("last payload");
            if chunk.count < EVENTS_PER_CHUNK && block.bytes < self.page_bytes {
                serial = end - 1;
                count = chunk.count;
                self.records = chunk.records;
                checkpoint = self.chunk_states[index];
                used = block.bytes;
                let mut copied = 0;
                while copied < used {
                    let lease = self.block_map[chunk.records[0].block * self.block_pages]
                        .expect("partial page");
                    let data = self.pages.bytes(io.store, lease)?;
                    self.scratch[copied..used].copy_from_slice(&data[copied..used]);
                    copied = used;
                }
                io.store.charge_work(WorkClass::Render, used, 0)?;
            }
        }
        for i in 0..input.len() {
            let (offset, data) = input.get(i).expect("validated input");
            if count > 0 && (count == EVENTS_PER_CHUNK || used + data.len() > self.page_bytes) {
                self.flush(
                    io.store,
                    &mut root,
                    serial,
                    count,
                    used,
                    &checkpoint,
                    &mut end,
                );
                serial = end;
                count = 0;
                used = 0;
                checkpoint = state;
            }
            if data.len() > self.page_bytes {
                let block = self.block_from(io.store, data);
                self.records[0] = Record {
                    frame: frame.0 + u64::from(offset),
                    block,
                    offset: 0,
                    len: data.len(),
                };
                self.flush_existing(io.store, &mut root, serial, 1, &state, &mut end);
                state
                    .apply_packet(data, self.config.max_packet)
                    .expect("validated packet");
                serial = end;
                checkpoint = state;
                self.work.add(1, 0, 0, 0);
            } else {
                if count == 0 {
                    checkpoint = state;
                    serial = end;
                }
                self.scratch[used..used + data.len()].copy_from_slice(data);
                self.records[count] = Record {
                    frame: frame.0 + u64::from(offset),
                    block: 0,
                    offset: used,
                    len: data.len(),
                };
                used += data.len();
                count += 1;
                state
                    .apply_packet(data, self.config.max_packet)
                    .expect("validated packet");
                self.work.add(1, 0, 0, 0);
            }
        }
        if count > 0 {
            self.flush(
                io.store,
                &mut root,
                serial,
                count,
                used,
                &checkpoint,
                &mut end,
            );
        }
        let old = core::mem::replace(&mut self.root, root);
        self.release_now(io.store, old);
        self.end = end;
        *self.current = state;
        self.work.add(0, 1, 0, 0);
        self.next = next;
        Ok(())
    }
    pub(super) fn flush(
        &mut self,
        store: &mut ResourceStore,
        root: &mut Option<usize>,
        serial: u64,
        count: usize,
        used: usize,
        state: &MidiState,
        end: &mut u64,
    ) {
        let block = self.block_from_scratch(store, used);
        for r in &mut self.records[..count] {
            r.block = block;
        }
        self.flush_existing(store, root, serial, count, state, end);
    }
    pub(super) fn flush_existing(
        &mut self,
        store: &mut ResourceStore,
        root: &mut Option<usize>,
        serial: u64,
        count: usize,
        state: &MidiState,
        end: &mut u64,
    ) {
        if serial == *end && serial >= self.config.chunks as u64 {
            if let Ok(index) = self.find(*root, serial - self.config.chunks as u64) {
                let chunk = self.chunks[index].expect("evicted chunk");
                self.pressure_start = Frame(
                    self.pressure_start
                        .0
                        .max(chunk.records[chunk.count - 1].frame + 1),
                );
            }
        }
        self.publish_chunk(store, root, serial, count, state);
        *end = (*end).max(serial + 1);
    }
    pub fn grab(&mut self, io: &mut RenderIo<'_>, range: Range<Frame>) -> Result<TakeId, Error> {
        self.check(io.store)?;
        let coverage = self.coverage()?;
        if range.start < coverage.start || range.end > coverage.end || range.start >= range.end {
            return Err(Error::InvalidFrame);
        }
        let source = self.history_view();
        let first = self.locate(source, range.start.0)?;
        let last = self.locate(source, range.end.0)?;
        let state = self.state_at(io.store, source, range.start.0)?.0;
        let (bytes, events) = self.between(self.root, first, last)?;
        self.own(
            View {
                root: self.root,
                first,
                last,
                origin: range.start.0,
                frames: range.end.0 - range.start.0,
                bytes,
                events,
                revision: Revision(0),
                writable: true,
                retiring: false,
            },
            &state,
            true,
        )
        .map(TakeId)
    }
    pub(super) fn history_view(&self) -> View {
        View {
            root: self.root,
            first: Point {
                serial: self.end.saturating_sub(self.config.chunks as u64),
                event: 0,
            },
            last: Point {
                serial: self.end,
                event: 0,
            },
            origin: self.origin.0,
            frames: self.next.0 - self.origin.0,
            bytes: 0,
            events: 0,
            revision: Revision(0),
            writable: false,
            retiring: false,
        }
    }
}
