use super::*;

impl MidiStore {
    pub(super) fn retain(&mut self, root: Option<usize>) -> Result<(), Error> {
        if let Some(n) = root {
            let n = self.nodes[n].as_mut().expect("live node");
            n.references = n.references.checked_add(1).ok_or(Error::Exhausted)?;
        }
        Ok(())
    }
    pub(super) fn unreference(&mut self, root: Option<usize>) -> Option<usize> {
        root.filter(|&n| {
            let n = self.nodes[n].as_mut().expect("owned node");
            n.references -= 1;
            n.references == 0
        })
    }
    pub(super) fn find(&self, root: Option<usize>, serial: u64) -> Result<usize, Error> {
        let key = serial as usize & (self.config.chunks - 1);
        let mut next = root;
        for bit in (0..self.depth).rev() {
            let Kind::Branch(children) = self.nodes[next.ok_or(Error::InvalidState)?]
                .expect("live node")
                .kind
            else {
                return Err(Error::InvalidState);
            };
            next = children[(key >> bit) & 1];
        }
        let Kind::Leaf(chunk) = self.nodes[next.ok_or(Error::InvalidState)?]
            .expect("live leaf")
            .kind
        else {
            return Err(Error::InvalidState);
        };
        if self.chunks[chunk].expect("live chunk").serial != serial {
            return Err(Error::InvalidState);
        }
        Ok(chunk)
    }
    pub(super) fn node(&mut self, kind: Kind) -> usize {
        let (bytes, events) = match kind {
            Kind::Leaf(c) => {
                let c = self.chunks[c].expect("owned chunk");
                (c.bytes, c.count)
            }
            Kind::Branch(children) => {
                children
                    .iter()
                    .flatten()
                    .fold((0usize, 0usize), |(b, e), &n| {
                        let n = self.nodes[n].expect("live child");
                        (
                            b.checked_add(n.bytes).expect("prepared byte ceiling"),
                            e + n.events,
                        )
                    })
            }
        };
        let slot = self.free_nodes.pop().expect("prepared index credit");
        self.nodes[slot] = Some(Node {
            references: 1,
            kind,
            bytes,
            events,
        });
        slot
    }
    pub(super) fn set(
        &mut self,
        root: Option<usize>,
        depth: usize,
        key: usize,
        chunk: usize,
    ) -> usize {
        self.work.add(0, 0, 1, 0);
        if depth == 0 {
            return self.node(Kind::Leaf(chunk));
        }
        let mut children = match root.map(|n| self.nodes[n].expect("live branch").kind) {
            Some(Kind::Branch(c)) => c,
            None => [None; 2],
            _ => unreachable!("branch depth"),
        };
        let branch = (key >> (depth - 1)) & 1;
        children[branch] = Some(self.set(children[branch], depth - 1, key, chunk));
        self.retain(children[1 - branch])
            .expect("prepared reference bound");
        self.node(Kind::Branch(children))
    }
    pub(super) fn enqueue(&mut self, garbage: Garbage, owner: usize, immediate: bool) {
        let job = Job { garbage, owner };
        if immediate {
            self.immediate.push(job);
        } else {
            self.retired.push(job);
        }
        self.pending[owner] += 1;
    }
    pub(super) fn drop_root(&mut self, root: Option<usize>, owner: usize, immediate: bool) {
        if let Some(n) = self.unreference(root) {
            self.enqueue(Garbage::Node(n), owner, immediate);
        }
    }
    pub(super) fn release_now(&mut self, store: &mut ResourceStore, root: Option<usize>) {
        let owner = self.views.len() + 1;
        self.drop_root(root, owner, true);
        while let Some(job) = self.immediate.pop() {
            self.process_job(store, job, true)
                .expect("owned transaction garbage");
        }
    }
    pub(super) fn process_job(
        &mut self,
        store: &mut ResourceStore,
        job: Job,
        immediate: bool,
    ) -> Result<(), Error> {
        let class = if immediate {
            WorkClass::Render
        } else {
            WorkClass::Background
        };
        match job.garbage {
            Garbage::Node(index) => {
                store.charge_work(class, 0, 1)?;
                let node = self.nodes[index].take().expect("retired node");
                match node.kind {
                    Kind::Branch(children) => {
                        for child in children {
                            self.drop_root(child, job.owner, immediate);
                        }
                    }
                    Kind::Leaf(chunk) => {
                        let c = self.chunks[chunk].as_mut().expect("owned chunk");
                        c.references -= 1;
                        if c.references == 0 {
                            self.enqueue(Garbage::Chunk(chunk, 0), job.owner, immediate);
                        }
                    }
                }
                self.free_nodes.push(index);
            }
            Garbage::Chunk(index, event) => {
                store.charge_work(class, 0, 1)?;
                let chunk = self.chunks[index].expect("retired chunk");
                if event < chunk.count {
                    let block = chunk.records[event].block;
                    let b = self.blocks[block].as_mut().expect("owned bytes");
                    b.references -= 1;
                    if b.references == 0 {
                        self.enqueue(Garbage::Block(block, 0), job.owner, immediate);
                    }
                    self.enqueue(Garbage::Chunk(index, event + 1), job.owner, immediate);
                } else {
                    self.chunks[index] = None;
                    self.free_chunks.push(index);
                }
            }
            Garbage::Block(index, page) => {
                let block = self.blocks[index].expect("retired bytes");
                let lease = self.block_map[index * self.block_pages + page]
                    .take()
                    .expect("owned page");
                let freed = if immediate {
                    self.pages.reclaim_render(store, lease)?
                } else {
                    self.pages.reclaim_lease(store, lease)?
                };
                if freed {
                    self.reserved += 1;
                    store.reserved_pages += 1;
                }
                if page + 1 < block.pages {
                    self.enqueue(Garbage::Block(index, page + 1), job.owner, immediate);
                } else {
                    self.blocks[index] = None;
                    self.free_blocks.push(index);
                }
            }
        }
        self.pending[job.owner] -= 1;
        self.work.add(0, 0, 0, 1);
        self.finish_owner(job.owner);
        Ok(())
    }
    pub(super) fn block_read(
        &self,
        store: &ResourceStore,
        block: usize,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), Error> {
        let b = self.blocks[block].ok_or(Error::InvalidState)?;
        if offset
            .checked_add(out.len())
            .is_none_or(|end| end > b.bytes)
        {
            return Err(Error::InvalidOffset);
        }
        let mut copied = 0;
        while copied < out.len() {
            let position = offset + copied;
            let page = position / self.page_bytes;
            let lease =
                self.block_map[block * self.block_pages + page].ok_or(Error::InvalidState)?;
            let bytes = self.pages.bytes(store, lease)?;
            let local = position % self.page_bytes;
            let count = (out.len() - copied).min(self.page_bytes - local);
            out[copied..copied + count].copy_from_slice(&bytes[local..local + count]);
            copied += count;
        }
        Ok(())
    }
    pub(super) fn block_from(&mut self, store: &mut ResourceStore, data: &[u8]) -> usize {
        let slot = self.free_blocks.pop().expect("prepared byte block");
        let count = data.len().div_ceil(self.page_bytes);
        for page in 0..count {
            self.scratch.fill(0);
            let start = page * self.page_bytes;
            let len = (data.len() - start).min(self.page_bytes);
            self.scratch[..len].copy_from_slice(&data[start..start + len]);
            self.reserved -= 1;
            store.reserved_pages -= 1;
            let lease = self
                .pages
                .insert_bytes_render(store, &self.scratch)
                .expect("prepared payload page");
            self.block_map[slot * self.block_pages + page] = Some(lease);
        }
        self.blocks[slot] = Some(Block {
            references: 0,
            bytes: data.len(),
            pages: count,
        });
        slot
    }
    pub(super) fn block_from_scratch(&mut self, store: &mut ResourceStore, len: usize) -> usize {
        let slot = self.free_blocks.pop().expect("prepared byte block");
        self.scratch[len..].fill(0);
        self.reserved -= 1;
        store.reserved_pages -= 1;
        let lease = self
            .pages
            .insert_bytes_render(store, &self.scratch)
            .expect("prepared payload page");
        self.block_map[slot * self.block_pages] = Some(lease);
        self.blocks[slot] = Some(Block {
            references: 0,
            bytes: len,
            pages: 1,
        });
        slot
    }
    pub(super) fn publish_chunk(
        &mut self,
        store: &mut ResourceStore,
        root: &mut Option<usize>,
        serial: u64,
        count: usize,
        state: &MidiState,
    ) {
        let slot = self.free_chunks.pop().expect("prepared checkpoint credit");
        let mut bytes = 0;
        for record in &self.records[..count] {
            self.blocks[record.block]
                .as_mut()
                .expect("owned bytes")
                .references += 1;
            bytes += 12 + record.len;
        }
        self.chunks[slot] = Some(Chunk {
            references: 1,
            serial,
            count,
            records: self.records,
            bytes,
        });
        self.chunk_states[slot] = *state;
        self.work.add(0, 1, 0, 0);
        let next = self.set(
            *root,
            self.depth,
            serial as usize & (self.config.chunks - 1),
            slot,
        );
        let old = core::mem::replace(root, Some(next));
        self.release_now(store, old);
    }
    pub(super) fn sum(
        &self,
        root: Option<usize>,
        low: usize,
        high: usize,
        start: usize,
        end: usize,
    ) -> (usize, usize) {
        if end <= low || high <= start {
            return (0, 0);
        }
        let Some(index) = root else {
            return (0, 0);
        };
        let node = self.nodes[index].expect("live index");
        if start <= low && high <= end {
            return (node.bytes, node.events);
        }
        let Kind::Branch(children) = node.kind else {
            return (node.bytes, node.events);
        };
        let middle = (low + high) / 2;
        let a = self.sum(children[0], low, middle, start, end);
        let b = self.sum(children[1], middle, high, start, end);
        (a.0 + b.0, a.1 + b.1)
    }
    pub(super) fn between(
        &self,
        root: Option<usize>,
        first: Point,
        last: Point,
    ) -> Result<(usize, usize), Error> {
        let count = usize::try_from(
            last.serial
                .checked_sub(first.serial)
                .ok_or(Error::InvalidState)?,
        )
        .map_err(|_| Error::Capacity)?;
        if count > self.config.chunks {
            return Err(Error::Capacity);
        }
        let start = first.serial as usize & (self.config.chunks - 1);
        let end = start + count;
        let mut total = if end <= self.config.chunks {
            self.sum(root, 0, self.config.chunks, start, end)
        } else {
            let a = self.sum(root, 0, self.config.chunks, start, self.config.chunks);
            let b = self.sum(root, 0, self.config.chunks, 0, end - self.config.chunks);
            (a.0 + b.0, a.1 + b.1)
        };
        if last.event > 0 {
            let chunk = self.chunks[self.find(root, last.serial)?].expect("end chunk");
            for r in &chunk.records[..last.event] {
                total.0 += 12 + r.len;
                total.1 += 1;
            }
        }
        if first.event > 0 {
            let chunk = self.chunks[self.find(root, first.serial)?].expect("start chunk");
            for r in &chunk.records[..first.event] {
                total.0 -= 12 + r.len;
                total.1 -= 1;
            }
        }
        Ok(total)
    }
    pub(super) fn apply_record(
        &mut self,
        store: &mut ResourceStore,
        state: &mut MidiState,
        record: Record,
    ) -> Result<(), Error> {
        let mut short = [0; 3];
        let count = record.len.min(3);
        self.block_read(store, record.block, record.offset, &mut short[..count])?;
        super::super::midi_index::apply_short(state, short);
        self.work.add(1, 0, 0, 0);
        store.charge_work(WorkClass::Render, count, 1)
    }
}
