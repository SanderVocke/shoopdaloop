//! Prepared, page-aligned mono audio history. Windows pin ranges in constant
//! metadata work; eviction acquires page leases for covering windows. Each prepared
//! window has a fixed page map, populated on eviction and recycled incrementally.
//! Live and retained capacity is reserved independently of ordinary uploads.

use super::{
    get, index, release,
    shared_pages::{LeaseId, SharedPages},
    slots, ResourceStore, Slot, WorkClass,
};
use crate::{Error, Frame};
use alloc::{boxed::Box, vec, vec::Vec};
use core::ops::Range;

#[derive(Clone, Copy)]
pub(super) struct Config {
    pub history_pages: usize,
    /// Conservatively charged per window until its deferred release completes.
    /// History and retention pages are reserved together during preparation.
    pub retained_pages: usize,
    /// Each slot prepares a map of history_pages entries.
    pub windows: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WindowId {
    scope: u32,
    slot: u32,
    generation: u32,
}

#[derive(Clone, Copy)]
struct Chunk {
    ordinal: u64,
    lease: LeaseId,
}

#[derive(Clone, Copy)]
struct Window {
    first: u64,
    pages: usize,
    recycle: Option<usize>,
}

impl Window {
    fn covers(self, ordinal: u64) -> bool {
        self.recycle.is_none() && ordinal >= self.first && ordinal - self.first < self.pages as u64
    }
}

pub(super) struct RollingHistory {
    scope: u32,
    store_scope: u32,
    origin: Frame,
    next: Frame,
    ordinal: u64,
    page_frames: usize,
    ring: Box<[Option<Chunk>]>,
    windows: Vec<Slot<Window>>,
    free_windows: Vec<usize>,
    retained: Box<[Option<LeaseId>]>,
    retained_limit: usize,
    debt: usize,
    reserved: usize,
    pages: SharedPages,
    cleanup: Option<usize>,
}

impl RollingHistory {
    pub fn prepare(
        store: &mut ResourceStore,
        config: Config,
        origin: Frame,
    ) -> Result<Self, Error> {
        if config.history_pages == 0 || config.retained_pages == 0 || config.windows == 0 {
            return Err(Error::InvalidConfig);
        }
        let reserved = config
            .history_pages
            .checked_add(config.retained_pages)
            .ok_or(Error::Capacity)?;
        if reserved > store.free_pages.len() - store.reserved_pages {
            return Err(Error::Capacity);
        }
        let maps = config
            .history_pages
            .checked_mul(config.windows)
            .ok_or(Error::Capacity)?;
        let leases = maps
            .checked_add(config.history_pages)
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Capacity)?;
        maps.checked_mul(size_of::<Option<LeaseId>>())
            .and_then(|n| {
                n.checked_add(
                    config
                        .history_pages
                        .checked_mul(size_of::<Option<Chunk>>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    config
                        .windows
                        .checked_mul(size_of::<Slot<Window>>() + size_of::<usize>())?,
                )
            })
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let mut ring = vec![None; config.history_pages].into_boxed_slice();
        let mut retained = vec![None; maps].into_boxed_slice();
        let mut windows = slots(config.windows);
        for chunk in &mut ring {
            *core::hint::black_box(chunk) = None;
        }
        for lease in &mut retained {
            *core::hint::black_box(lease) = None;
        }
        for window in &mut windows {
            *core::hint::black_box(window) = Slot {
                generation: 1,
                value: None,
            };
        }
        let pages = SharedPages::prepare(store, leases)?;
        let scope = index::new_scope()?;
        store.reserved_pages += reserved;
        Ok(Self {
            scope,
            store_scope: store.scope,
            origin,
            next: origin,
            ordinal: 0,
            page_frames: store.config.limits().page_bytes / 4,
            ring,
            windows,
            free_windows: (0..config.windows).rev().collect(),
            retained,
            retained_limit: config.retained_pages,
            debt: 0,
            reserved,
            pages,
            cleanup: None,
        })
    }

    fn check_store(&self, store: &ResourceStore) -> Result<(), Error> {
        if store.scope != self.store_scope {
            return Err(Error::InvalidId);
        }
        Ok(())
    }

    fn check_open(&self) -> Result<(), Error> {
        if self.cleanup.is_some() {
            return Err(Error::InvalidState);
        }
        Ok(())
    }

    pub fn coverage(&self) -> Result<Range<Frame>, Error> {
        self.check_open()?;
        let first = self.ordinal.saturating_sub(self.ring.len() as u64);
        Ok(Frame(self.origin.0 + first * self.page_frames as u64)..self.next)
    }

    fn insert(&mut self, store: &mut ResourceStore, samples: &[f32]) -> Result<LeaseId, Error> {
        if self.reserved == 0 {
            return Err(Error::Capacity);
        }
        self.reserved -= 1;
        store.reserved_pages -= 1;
        match self.pages.insert_audio(store, samples) {
            Ok(lease) => Ok(lease),
            Err(error) => {
                self.reserved += 1;
                store.reserved_pages += 1;
                Err(error)
            }
        }
    }

    pub fn append(
        &mut self,
        store: &mut ResourceStore,
        frame: Frame,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.check_store(store)?;
        self.check_open()?;
        if frame != self.next {
            return Err(Error::InvalidFrame);
        }
        if samples.len() != self.page_frames {
            return Err(Error::InvalidShape);
        }
        let next = frame.checked_add(self.page_frames)?;
        let slot = (self.ordinal % self.ring.len() as u64) as usize;
        let lease = if let Some(old) = self.ring[slot] {
            let covering = self
                .windows
                .iter()
                .filter_map(|slot| slot.value)
                .filter(|window| window.covers(old.ordinal))
                .count();
            if covering == 0 {
                self.pages.replace_audio(store, old.lease, samples)?
            } else {
                self.pages.preflight_retain(old.lease, covering)?;
                if self.pages.available_leases() <= covering {
                    return Err(Error::Capacity);
                }
                let fresh = self.insert(store, samples)?;
                for (slot, entry) in self.windows.iter().enumerate() {
                    if let Some(window) = entry.value.filter(|window| window.covers(old.ordinal)) {
                        let index = slot * self.ring.len() + (old.ordinal - window.first) as usize;
                        self.retained[index] = Some(
                            self.pages
                                .retain(old.lease)
                                .expect("preflighted window lease"),
                        );
                    }
                }
                self.pages
                    .release(store, old.lease)
                    .expect("retained history page");
                fresh
            }
        } else {
            self.insert(store, samples)?
        };
        self.ring[slot] = Some(Chunk {
            ordinal: self.ordinal,
            lease,
        });
        self.ordinal += 1;
        self.next = next;
        Ok(())
    }

    pub fn lease_window(
        &mut self,
        store: &mut ResourceStore,
        range: Range<Frame>,
    ) -> Result<WindowId, Error> {
        self.check_store(store)?;
        let coverage = self.coverage()?;
        if range.start < coverage.start || range.end > coverage.end || range.start >= range.end {
            return Err(Error::InvalidFrame);
        }
        let start = range.start.0 - self.origin.0;
        let end = range.end.0 - self.origin.0;
        if start % self.page_frames as u64 != 0 || end % self.page_frames as u64 != 0 {
            return Err(Error::InvalidFrame);
        }
        let pages = ((end - start) / self.page_frames as u64) as usize;
        let debt = self
            .debt
            .checked_add(pages)
            .filter(|&n| n <= self.retained_limit)
            .ok_or(Error::Capacity)?;
        if self.free_windows.is_empty() {
            return Err(Error::Capacity);
        }
        store.charge_external(0, 1)?;
        let slot = self.free_windows.pop().expect("preflighted window credit");
        self.windows[slot].value = Some(Window {
            first: start / self.page_frames as u64,
            pages,
            recycle: None,
        });
        self.debt = debt;
        Ok(WindowId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.windows[slot].generation,
        })
    }

    fn window(&self, id: WindowId) -> Result<Window, Error> {
        self.check_open()?;
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        get(&self.windows, id.slot, id.generation).and_then(|window| {
            if window.recycle.is_none() {
                Ok(window)
            } else {
                Err(Error::InvalidId)
            }
        })
    }

    /// Read at most one page remainder; offsets and returned data are relative
    /// to the window. Required reads do not spend background transfer credits.
    pub fn read_audio(
        &self,
        store: &mut ResourceStore,
        id: WindowId,
        offset: u64,
        output: &mut [f32],
    ) -> Result<usize, Error> {
        self.check_store(store)?;
        let window = self.window(id)?;
        let frames = window.pages as u64 * self.page_frames as u64;
        if offset > frames {
            return Err(Error::InvalidOffset);
        }
        if offset == frames || output.is_empty() {
            return Ok(0);
        }
        let page_offset = (offset / self.page_frames as u64) as usize;
        let ordinal = window.first + page_offset as u64;
        let lease = self.ring[(ordinal % self.ring.len() as u64) as usize]
            .filter(|chunk| chunk.ordinal == ordinal)
            .map(|chunk| chunk.lease)
            .or(self.retained[id.slot as usize * self.ring.len() + page_offset])
            .ok_or(Error::InvalidState)?;
        let within = (offset % self.page_frames as u64) as usize;
        let count = output.len().min(self.page_frames - within);
        self.pages.bytes(store, lease)?;
        store.note_operation(WorkClass::Render);
        store.charge_work(WorkClass::Render, count * 4, 0)?;
        let bytes = self.pages.bytes(store, lease)?;
        for (sample, bytes) in output[..count]
            .iter_mut()
            .zip(bytes[within * 4..(within + count) * 4].chunks_exact(4))
        {
            *sample = f32::from_le_bytes(bytes.try_into().expect("one complete sample"));
        }
        Ok(count)
    }

    pub fn release_window(&mut self, id: WindowId) -> Result<(), Error> {
        self.window(id)?;
        self.windows[id.slot as usize]
            .value
            .as_mut()
            .expect("validated window")
            .recycle = Some(0);
        Ok(())
    }

    fn reclaim(&mut self, store: &mut ResourceStore, lease: LeaseId) -> Result<(), Error> {
        if self.pages.reclaim_lease(store, lease)? {
            self.reserved += 1;
            store.reserved_pages += 1;
        }
        Ok(())
    }

    pub fn maintain(&mut self, store: &mut ResourceStore) -> Result<usize, Error> {
        self.check_store(store)?;
        let mut work = 0;
        for slot in 0..self.windows.len() {
            while let Some(mut window) = self.windows[slot]
                .value
                .filter(|window| window.recycle.is_some())
            {
                if store.steps_remaining() == 0 {
                    return Ok(work);
                }
                let cursor = window.recycle.expect("recycling window");
                let index = slot * self.ring.len() + cursor;
                if let Some(lease) = self.retained[index] {
                    self.reclaim(store, lease)?;
                    self.retained[index] = None;
                } else {
                    store.charge_external(0, 1)?;
                }
                work += 1;
                if cursor + 1 == window.pages {
                    self.debt -= window.pages;
                    release(&mut self.windows, &mut self.free_windows, slot);
                } else {
                    window.recycle = Some(cursor + 1);
                    self.windows[slot].value = Some(window);
                }
            }
        }
        if let Some(mut cursor) = self.cleanup {
            while cursor < self.ring.len() && store.steps_remaining() > 0 {
                if let Some(chunk) = self.ring[cursor] {
                    self.reclaim(store, chunk.lease)?;
                    self.ring[cursor] = None;
                } else {
                    store.charge_external(0, 1)?;
                }
                cursor += 1;
                work += 1;
            }
            self.cleanup = Some(cursor);
            if cursor == self.ring.len() {
                store.reserved_pages -= self.reserved;
                self.reserved = 0;
            }
        }
        Ok(work)
    }

    /// Quiescent cleanup begins only after all window owners and their deferred
    /// releases have settled. Maintenance returns the prepared reservation.
    pub fn begin_cleanup(&mut self) -> Result<(), Error> {
        self.check_open()?;
        if self.debt != 0 {
            return Err(Error::Busy);
        }
        self.cleanup = Some(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
