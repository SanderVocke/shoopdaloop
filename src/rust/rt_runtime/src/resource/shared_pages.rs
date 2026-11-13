//! Immutable audio pages with independent leases, using the prepared resource arena.
//! Copies consume callback-wide transfer credits. Final release retires a page for
//! incremental reclamation. Copying a lease ID does not acquire another owner.

use super::{get, index, release, slots, Page, ResourceStore, Slot, WorkClass};
use crate::Error;
use crate::Sample;
use alloc::{boxed::Box, vec, vec::Vec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LeaseId {
    scope: u32,
    slot: u32,
    generation: u32,
}

pub(super) struct SharedPages<S: Sample = f32> {
    sample: core::marker::PhantomData<S>,
    scope: u32,
    store_scope: u32,
    references: Box<[usize]>,
    leases: Vec<Slot<usize>>,
    free_leases: Vec<usize>,
}

impl<S: Sample> SharedPages<S> {
    pub(super) fn allocated_bytes(&self) -> usize {
        core::mem::size_of_val(&*self.references)
            + super::memory::vec_bytes(&self.leases)
            + super::memory::vec_bytes(&self.free_leases)
    }

    pub fn prepare(store: &ResourceStore<S>, leases: usize) -> Result<Self, Error> {
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
            sample: core::marker::PhantomData,
            scope: index::new_scope()?,
            store_scope: store.scope,
            references,
            leases: leases_table,
            free_leases: (0..leases).rev().collect(),
        })
    }

    fn check_store(&self, store: &ResourceStore<S>) -> Result<(), Error> {
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
        self.references[page] += 1;
        LeaseId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.leases[slot].generation,
        }
    }

    fn reserve_page(&self, store: &mut ResourceStore<S>) -> Result<(), Error> {
        self.check_store(store)?;
        if self.free_leases.is_empty() || store.free_pages.len() == store.reserved_pages {
            return Err(Error::Capacity);
        }
        store.charge_external(store.config.limits().page_bytes, 0)
    }

    fn publish(&mut self, store: &mut ResourceStore<S>, page: usize) -> LeaseId {
        store.pages[page] = Page {
            used: store.config.limits().page_bytes,
            ..Page::default()
        };
        self.acquire(page)
    }

    pub fn insert_audio(
        &mut self,
        store: &mut ResourceStore<S>,
        samples: &[S],
    ) -> Result<LeaseId, Error> {
        self.check_store(store)?;
        let page_bytes = store.config.limits().page_bytes;
        if samples.len() != page_bytes / size_of::<S>() {
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
        store: &mut ResourceStore<S>,
        bytes: &[u8],
    ) -> Result<LeaseId, Error> {
        self.check_store(store)?;
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
        store: &mut ResourceStore<S>,
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
            store.pages[page] = Page::default();
            store.free_pages.push(page);
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
        store: &mut ResourceStore<S>,
        lease: LeaseId,
        samples: &[S],
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
        store: &mut ResourceStore<S>,
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
            store.pages[page] = Page::default();
            store.free_pages.push(page);
            store.work.reclaimed_pages += 1;
        }
        release(&mut self.leases, &mut self.free_leases, lease.slot as usize);
        Ok(remaining == 0)
    }

    pub fn bytes<'a>(
        &self,
        store: &'a ResourceStore<S>,
        lease: LeaseId,
    ) -> Result<&'a [u8], Error> {
        self.check_store(store)?;
        let page = self.page(lease)?;
        let page_bytes = store.config.limits().page_bytes;
        Ok(&store.data[page * page_bytes..(page + 1) * page_bytes])
    }

    /// Fork one full page, replacing a nonempty, sample-aligned interval. The
    /// original lease and every other reader remain unchanged on success or failure.
    pub fn fork_with_patch(
        &mut self,
        store: &mut ResourceStore<S>,
        source: LeaseId,
        first_sample: usize,
        samples: &[S],
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

    pub fn release(&mut self, store: &mut ResourceStore<S>, lease: LeaseId) -> Result<(), Error> {
        self.check_store(store)?;
        let page = self.page(lease)?;
        let remaining = self.references[page]
            .checked_sub(1)
            .ok_or(Error::InvalidState)?;
        self.references[page] = remaining;
        if remaining == 0 {
            store.retired_pages.push(page);
        }
        release(&mut self.leases, &mut self.free_leases, lease.slot as usize);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
