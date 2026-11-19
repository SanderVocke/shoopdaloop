use super::{ContentStore, Error, MidiState, SnapshotId};
use alloc::{boxed::Box, vec::Vec};

pub const STATE_CELLS: usize = 16 * 264 + 1;
const WORDS: usize = STATE_CELLS.div_ceil(64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComparisonId {
    pub target: SnapshotId,
    pub destination: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Differences {
    words: [u64; WORDS],
    summary: u128,
}
impl Default for Differences {
    fn default() -> Self {
        Self {
            words: [0; WORDS],
            summary: 0,
        }
    }
}
impl Differences {
    pub fn is_empty(&self) -> bool {
        self.summary == 0
    }
    pub fn contains(&self, cell: usize) -> bool {
        self.words[cell / 64] & (1 << (cell % 64)) != 0
    }
    pub fn next(&self, from: usize) -> Option<usize> {
        if from >= STATE_CELLS {
            return None;
        }
        let word = from / 64;
        let bits = self.words[word] & (u64::MAX << (from % 64));
        if bits != 0 {
            return Some(word * 64 + bits.trailing_zeros() as usize);
        }
        let later = self.summary & (u128::MAX << (word + 1));
        if later == 0 {
            return None;
        }
        let word = later.trailing_zeros() as usize;
        Some(word * 64 + self.words[word].trailing_zeros() as usize)
    }
    pub(super) fn set(&mut self, cell: usize, different: bool) {
        let word = cell / 64;
        let bit = 1 << (cell % 64);
        if different {
            self.words[word] |= bit;
        } else {
            self.words[word] &= !bit;
        }
        if self.words[word] == 0 {
            self.summary &= !(1 << word);
        } else {
            self.summary |= 1 << word;
        }
    }
    pub(super) fn compare(&mut self, target: &MidiState, current: &MidiState, cell: usize) {
        self.set(cell, differs(target, current, cell));
    }
}
fn differs(target: &MidiState, current: &MidiState, cell: usize) -> bool {
    if cell == STATE_CELLS - 1 {
        return false;
    }
    let field = cell % 264;
    let t = target.cell(cell);
    let c = current.cell(cell);
    match field {
        0..128 => (t & 0xffff) != (c & 0xffff) || ((t >> 16) < 128 && (t >> 16) != (c >> 16)),
        128..256 | 260 => t < 128 && t != c,
        256..=258 => {
            target
                .channel((cell / 264) as u8)
                .unwrap()
                .program()
                .is_some()
                && t < 128
                && t != c
        }
        259 => t != c,
        _ => false,
    }
}
#[derive(Clone, Copy)]
struct Entry {
    generation: u32,
    cursor: usize,
    differences: Differences,
}
pub struct DifferenceService {
    entries: Box<[Entry]>,
    destinations: u16,
    work: u64,
}
impl DifferenceService {
    pub fn prepare(snapshots: u32, destinations: u16) -> Result<Self, Error> {
        if snapshots == 0 || destinations == 0 {
            return Err(Error::Capacity);
        }
        let n = (snapshots as usize)
            .checked_mul(usize::from(destinations))
            .ok_or(Error::Capacity)?;
        let mut entries = Vec::new();
        entries.try_reserve_exact(n).map_err(|_| Error::Capacity)?;
        entries.resize(
            n,
            Entry {
                generation: 0,
                cursor: 0,
                differences: Differences::default(),
            },
        );
        Ok(Self {
            entries: entries.into_boxed_slice(),
            destinations,
            work: 0,
        })
    }
    pub fn work(&self) -> u64 {
        self.work
    }
    pub fn prepared_bytes(&self) -> usize {
        size_of::<Self>() + self.entries.len() * size_of::<Entry>()
    }
    fn index(&self, id: ComparisonId) -> Result<usize, Error> {
        if id.destination >= self.destinations {
            return Err(Error::InvalidState);
        }
        let i =
            id.target.slot as usize * usize::from(self.destinations) + usize::from(id.destination);
        if i >= self.entries.len() {
            return Err(Error::InvalidState);
        }
        Ok(i)
    }
    pub fn get(&self, store: &ContentStore, id: ComparisonId) -> Result<&Differences, Error> {
        store.state(id.target)?;
        let e = &self.entries[self.index(id)?];
        if e.generation != id.target.generation || e.cursor != STATE_CELLS {
            return Err(Error::Busy);
        }
        Ok(&e.differences)
    }
    pub fn initialize(
        &mut self,
        store: &ContentStore,
        id: ComparisonId,
        current: &MidiState,
        budget: u32,
    ) -> Result<bool, Error> {
        let target = store.state(id.target)?;
        let index = self.index(id)?;
        let e = &mut self.entries[index];
        if e.generation != id.target.generation {
            *e = Entry {
                generation: id.target.generation,
                cursor: 0,
                differences: Differences::default(),
            };
        }
        for _ in 0..budget {
            if e.cursor == STATE_CELLS {
                break;
            }
            e.differences.compare(target, current, e.cursor);
            e.cursor += 1;
            self.work = self.work.saturating_add(1);
        }
        Ok(e.cursor == STATE_CELLS)
    }
    pub fn changed(
        &mut self,
        store: &ContentStore,
        destination: u16,
        current: &MidiState,
        cell: usize,
    ) {
        assert!(destination < self.destinations);
        for snapshot in store.snapshot_ids() {
            let i =
                snapshot.slot as usize * usize::from(self.destinations) + usize::from(destination);
            let e = &mut self.entries[i];
            if e.generation == snapshot.generation && cell < e.cursor {
                e.differences
                    .compare(store.state(snapshot).unwrap(), current, cell);
                self.work = self.work.saturating_add(1);
            }
        }
    }
    pub fn derive(
        &mut self,
        store: &ContentStore,
        target: SnapshotId,
        base: SnapshotId,
        changed: &Differences,
        currents: &[super::Interpreter],
    ) -> Result<(), Error> {
        store.state(target)?;
        if currents.len() != usize::from(self.destinations) {
            return Err(Error::InvalidState);
        }
        for destination in 0..self.destinations {
            self.get(
                store,
                ComparisonId {
                    target: base,
                    destination,
                },
            )?;
        }
        for destination in 0..self.destinations {
            let source = self.index(ComparisonId {
                target: base,
                destination,
            })?;
            let dest = self.index(ComparisonId {
                target,
                destination,
            })?;
            self.entries[dest] = self.entries[source];
            self.entries[dest].generation = target.generation;
            let mut cursor = 0;
            while let Some(cell) = changed.next(cursor) {
                self.entries[dest].differences.compare(
                    store.state(target)?,
                    currents[usize::from(destination)].state()?,
                    cell,
                );
                self.work = self.work.saturating_add(1);
                cursor = cell + 1;
            }
            self.work = self.work.saturating_add(WORDS as u64);
        }
        Ok(())
    }
}
