//! Immutable audio pages with independent leases, using the prepared resource arena.
//! Copies consume callback-wide transfer credits. Final release retires a page for
//! incremental reclamation. Copying a lease ID does not acquire another owner.

use super::{get, index, release, slots, Page, ResourceStore, Slot, WorkClass};
use crate::Error;
use alloc::{boxed::Box, vec, vec::Vec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LeaseId {
    scope: u32,
    slot: u32,
    generation: u32,
}

pub(super) struct SharedPages {
    live: usize,
    chunks: Option<Chunks>,
    scope: u32,
    store_scope: u32,
    references: Box<[usize]>,
    leases: Vec<Slot<usize>>,
    free_leases: Vec<usize>,
}

struct Chunks {
    data: Box<[f32]>,
    samples: usize,
    free: Vec<usize>,
}

impl SharedPages {
    pub fn prepare(store: &ResourceStore, leases: usize) -> Result<Self, Error> {
        if leases == 0 {
            return Err(Error::InvalidConfig);
        }
        leases
            .checked_mul(size_of::<Slot<usize>>() + size_of::<usize>())
            .and_then(|bytes| {
                store
                    .pages
                    .len()
                    .checked_mul(size_of::<usize>())?
                    .checked_add(bytes)
            })
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let mut references = vec![0; store.pages.len()].into_boxed_slice();
        for count in &mut references {
            *core::hint::black_box(count) = 0;
        }
        let mut leases_table = slots(leases);
        for lease in &mut leases_table {
            *core::hint::black_box(lease) = Slot {
                generation: 1,
                value: None,
            };
        }
        Ok(Self {
            live: 0,
            chunks: None,
            scope: index::new_scope()?,
            store_scope: store.scope,
            references,
            leases: leases_table,
            free_leases: (0..leases).rev().collect(),
        })
    }

    pub fn prepare_chunks(
        store: &mut ResourceStore,
        physical_pages: usize,
        count: usize,
        samples: usize,
        leases: usize,
    ) -> Result<Self, Error> {
        let mut result = Self::prepare(store, leases)?;
        let size = count
            .checked_mul(samples)
            .filter(|&n| n <= i32::MAX as usize / 4)
            .ok_or(Error::Capacity)?;
        store.detach_audio_pages(physical_pages)?;
        let mut data = vec![0.; size].into_boxed_slice();
        for value in &mut data {
            *core::hint::black_box(value) = 0.;
        }
        result.references = vec![0; count].into_boxed_slice();
        result.chunks = Some(Chunks {
            data,
            samples,
            free: (0..count).rev().collect(),
        });
        Ok(result)
    }

    pub fn live_payloads(&self) -> usize {
        if let Some(chunks) = &self.chunks {
            self.references.len() - chunks.free.len()
        } else {
            self.live
        }
    }
    pub fn chunked(&self) -> bool {
        self.chunks.is_some()
    }

    pub fn insert_samples_render(
        &mut self,
        store: &mut ResourceStore,
        samples: &[f32],
    ) -> Result<LeaseId, Error> {
        self.check_store(store)?;
        let chunks = self.chunks.as_mut().ok_or(Error::InvalidState)?;
        if samples.len() != chunks.samples || self.free_leases.is_empty() {
            return Err(Error::InvalidShape);
        }
        let page = chunks.free.pop().ok_or(Error::Capacity)?;
        chunks.data[page * chunks.samples..(page + 1) * chunks.samples].copy_from_slice(samples);
        store.charge_work(WorkClass::Render, samples.len() * 4, 0)?;
        Ok(self.acquire(page))
    }

    pub fn copy_span(
        &self,
        store: &ResourceStore,
        lease: LeaseId,
        start: usize,
        output: &mut [u8],
    ) -> Result<(), Error> {
        if let Some(chunks) = &self.chunks {
            let page = self.page(lease)?;
            if start
                .checked_add(output.len())
                .is_none_or(|end| end > chunks.samples * 4)
            {
                return Err(Error::InvalidOffset);
            }
            for (offset, value) in output.iter_mut().enumerate() {
                let byte = start + offset;
                *value = chunks.data[page * chunks.samples + byte / 4].to_le_bytes()[byte % 4];
            }
        } else {
            output.copy_from_slice(&self.bytes(store, lease)?[start..start + output.len()]);
        }
        Ok(())
    }

    pub fn sample_span(&self, lease: LeaseId) -> Result<&[f32], Error> {
        let page = self.page(lease)?;
        let chunks = self.chunks.as_ref().ok_or(Error::InvalidState)?;
        Ok(&chunks.data[page * chunks.samples..(page + 1) * chunks.samples])
    }

    fn check_store(&self, store: &ResourceStore) -> Result<(), Error> {
        if self.store_scope != store.scope {
            return Err(Error::InvalidId);
        }
        Ok(())
    }

    fn page(&self, lease: LeaseId) -> Result<usize, Error> {
        if lease.scope != self.scope {
            return Err(Error::InvalidId);
        }
        get(&self.leases, lease.slot, lease.generation)
    }

    fn acquire(&mut self, page: usize) -> LeaseId {
        let slot = self.free_leases.pop().expect("reserved lease credit");
        self.leases[slot].value = Some(page);
        if self.references[page] == 0 {
            self.live += 1;
        }
        self.references[page] += 1;
        LeaseId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.leases[slot].generation,
        }
    }

    fn reserve_page(&self, store: &mut ResourceStore) -> Result<(), Error> {
        self.check_store(store)?;
        if self.free_leases.is_empty() || store.free_pages.len() == store.reserved_pages {
            return Err(Error::Capacity);
        }
        store.charge_external(store.config.limits().page_bytes, 0)
    }

    fn publish(&mut self, store: &mut ResourceStore, page: usize) -> LeaseId {
        store.pages[page] = Page {
            used: store.config.limits().page_bytes,
            ..Page::default()
        };
        self.acquire(page)
    }

    pub fn insert_audio(
        &mut self,
        store: &mut ResourceStore,
        samples: &[f32],
    ) -> Result<LeaseId, Error> {
        self.check_store(store)?;
        let page_bytes = store.config.limits().page_bytes;
        if samples.len() != page_bytes / size_of::<f32>() {
            return Err(Error::InvalidShape);
        }
        self.reserve_page(store)?;
        let page = store.free_pages.pop().expect("reserved page credit");
        for (sample, output) in samples
            .iter()
            .zip(store.data[page * page_bytes..(page + 1) * page_bytes].chunks_exact_mut(4))
        {
            output.copy_from_slice(&sample.to_le_bytes());
        }
        Ok(self.publish(store, page))
    }

    pub(super) fn insert_bytes_render(
        &mut self,
        store: &mut ResourceStore,
        bytes: &[u8],
    ) -> Result<LeaseId, Error> {
        self.check_store(store)?;
        if let Some(chunks) = &mut self.chunks {
            if bytes.len() != chunks.samples * 4 || self.free_leases.is_empty() {
                return Err(Error::InvalidShape);
            }
            let page = chunks.free.pop().ok_or(Error::Capacity)?;
            for (target, source) in chunks.data[page * chunks.samples..(page + 1) * chunks.samples]
                .iter_mut()
                .zip(bytes.chunks_exact(4))
            {
                *target = f32::from_le_bytes(source.try_into().expect("sample bytes"));
            }
            store.charge_work(WorkClass::Render, bytes.len(), 0)?;
            return Ok(self.acquire(page));
        }
        let page_bytes = store.config.limits().page_bytes;
        if bytes.len() != page_bytes {
            return Err(Error::InvalidShape);
        }
        if self.free_leases.is_empty() || store.free_pages.len() == store.reserved_pages {
            return Err(Error::Capacity);
        }
        store.charge_work(WorkClass::Render, page_bytes, 0)?;
        let page = store.free_pages.pop().expect("prepared page credit");
        store.data[page * page_bytes..(page + 1) * page_bytes].copy_from_slice(bytes);
        Ok(self.publish(store, page))
    }

    pub(super) fn reclaim_render(
        &mut self,
        store: &mut ResourceStore,
        lease: LeaseId,
    ) -> Result<bool, Error> {
        self.check_store(store)?;
        let page = self.page(lease)?;
        let remaining = self.references[page]
            .checked_sub(1)
            .ok_or(Error::InvalidState)?;
        store.charge_work(WorkClass::Render, 0, 1)?;
        self.references[page] = remaining;
        if remaining == 0 {
            self.live -= 1;
        }
        if remaining == 0 {
            if let Some(chunks) = &mut self.chunks {
                chunks.free.push(page);
            } else {
                store.pages[page] = Page::default();
                store.free_pages.push(page);
            }
        }
        release(&mut self.leases, &mut self.free_leases, lease.slot as usize);
        Ok(remaining == 0)
    }

    pub fn retain(&mut self, lease: LeaseId) -> Result<LeaseId, Error> {
        self.preflight_retain(lease, 1)?;
        let page = self.page(lease)?;
        Ok(self.acquire(page))
    }

    pub fn available_leases(&self) -> usize {
        self.free_leases.len()
    }

    pub fn preflight_retain(&self, lease: LeaseId, count: usize) -> Result<(), Error> {
        let page = self.page(lease)?;
        self.references[page]
            .checked_add(count)
            .ok_or(Error::Exhausted)?;
        if self.free_leases.len() < count {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    /// Reuse exclusively owned storage, invalidating the previous lease before
    /// exposing the new contents. Shared pages must instead be forked.
    pub fn replace_audio(
        &mut self,
        store: &mut ResourceStore,
        lease: LeaseId,
        samples: &[f32],
    ) -> Result<LeaseId, Error> {
        self.check_store(store)?;
        let page = self.page(lease)?;
        if self.references[page] != 1 {
            return Err(Error::Busy);
        }
        let generation = lease.generation.checked_add(1).ok_or(Error::Exhausted)?;
        let page_bytes = store.config.limits().page_bytes;
        if samples.len() != page_bytes / 4 {
            return Err(Error::InvalidShape);
        }
        store.charge_external(page_bytes, 0)?;
        for (sample, output) in samples
            .iter()
            .zip(store.data[page * page_bytes..(page + 1) * page_bytes].chunks_exact_mut(4))
        {
            output.copy_from_slice(&sample.to_le_bytes());
        }
        self.leases[lease.slot as usize].generation = generation;
        Ok(LeaseId {
            generation,
            ..lease
        })
    }

    /// Spend one maintenance step to release a lease and, for its final owner,
    /// return the page immediately. No retirement queue is scanned.
    pub fn reclaim_lease(
        &mut self,
        store: &mut ResourceStore,
        lease: LeaseId,
    ) -> Result<bool, Error> {
        self.check_store(store)?;
        let page = self.page(lease)?;
        let remaining = self.references[page]
            .checked_sub(1)
            .ok_or(Error::InvalidState)?;
        store.charge_external(0, 1)?;
        self.references[page] = remaining;
        if remaining == 0 {
            self.live -= 1;
        }
        if remaining == 0 {
            if let Some(chunks) = &mut self.chunks {
                chunks.free.push(page);
            } else {
                store.pages[page] = Page::default();
                store.free_pages.push(page);
            }
            store.work.reclaimed_pages += 1;
        }
        release(&mut self.leases, &mut self.free_leases, lease.slot as usize);
        Ok(remaining == 0)
    }

    pub fn bytes<'a>(&self, store: &'a ResourceStore, lease: LeaseId) -> Result<&'a [u8], Error> {
        self.check_store(store)?;
        let page = self.page(lease)?;
        let page_bytes = store.config.limits().page_bytes;
        Ok(&store.data[page * page_bytes..(page + 1) * page_bytes])
    }

    /// Fork one full page, replacing a nonempty, sample-aligned interval. The
    /// original lease and every other reader remain unchanged on success or failure.
    pub fn fork_with_patch(
        &mut self,
        store: &mut ResourceStore,
        source: LeaseId,
        first_sample: usize,
        samples: &[f32],
    ) -> Result<LeaseId, Error> {
        self.check_store(store)?;
        let source = self.page(source)?;
        let page_bytes = store.config.limits().page_bytes;
        let start = first_sample.checked_mul(4).ok_or(Error::InvalidOffset)?;
        let end = samples
            .len()
            .checked_mul(4)
            .and_then(|bytes| start.checked_add(bytes))
            .filter(|&end| end <= page_bytes)
            .ok_or(Error::InvalidOffset)?;
        if samples.is_empty() {
            return Err(Error::InvalidShape);
        }
        self.reserve_page(store)?;
        let page = store.free_pages.pop().expect("reserved page credit");
        let origin = source * page_bytes;
        let destination = page * page_bytes;
        store.data.copy_within(origin..origin + start, destination);
        store
            .data
            .copy_within(origin + end..origin + page_bytes, destination + end);
        for (sample, output) in samples
            .iter()
            .zip(store.data[destination + start..destination + end].chunks_exact_mut(4))
        {
            output.copy_from_slice(&sample.to_le_bytes());
        }
        Ok(self.publish(store, page))
    }

    pub fn release(&mut self, store: &mut ResourceStore, lease: LeaseId) -> Result<(), Error> {
        self.check_store(store)?;
        let page = self.page(lease)?;
        let remaining = self.references[page]
            .checked_sub(1)
            .ok_or(Error::InvalidState)?;
        self.references[page] = remaining;
        if remaining == 0 {
            self.live -= 1;
        }
        if remaining == 0 {
            store.retired_pages.push(page);
        }
        release(&mut self.leases, &mut self.free_leases, lease.slot as usize);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
