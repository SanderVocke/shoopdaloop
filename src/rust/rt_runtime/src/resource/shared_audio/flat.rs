use super::*;
use core::num::NonZeroU32;

pub(super) const TILE: usize = 256;

#[derive(Clone, Copy)]
pub(super) struct Layout {
    width: usize,
    roots: usize,
    leaves: usize,
}

impl Layout {
    pub fn new(
        pages: usize,
        history: usize,
        debt: usize,
        owners: usize,
        transactions: usize,
    ) -> Result<Self, Error> {
        let roots = owners
            .checked_add(transactions)
            .and_then(|n| n.checked_add(8))
            .and_then(|n| n.checked_mul(2))
            .ok_or(Error::Capacity)?;
        let leaves = history
            .div_ceil(TILE)
            .checked_mul(2)
            .and_then(|n| n.checked_add(debt.div_ceil(TILE).checked_mul(2)?))
            .and_then(|n| n.checked_add(owners.checked_mul(4)?))
            .and_then(|n| n.checked_add(transactions.checked_mul(2)?))
            .and_then(|n| n.checked_add(roots))
            .ok_or(Error::Capacity)?;
        if leaves > u32::MAX as usize {
            return Err(Error::Capacity);
        }
        let result = Self {
            width: pages.div_ceil(TILE),
            roots,
            leaves,
        };
        result.bytes()?;
        Ok(result)
    }
    pub fn nodes(self) -> usize {
        self.roots + self.leaves
    }
    pub fn leases(self) -> Result<usize, Error> {
        self.leaves.checked_mul(TILE).ok_or(Error::Capacity)
    }
    pub fn bytes(self) -> Result<usize, Error> {
        self.roots
            .checked_mul(self.width)
            .and_then(|n| n.checked_mul(size_of::<Option<NonZeroU32>>()))
            .and_then(|n| {
                n.checked_add(
                    self.leases()
                        .ok()?
                        .checked_mul(size_of::<Option<LeaseId>>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    self.nodes()
                        .checked_mul(size_of::<Entry>() + 3 * size_of::<usize>())?,
                )
            })
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
}

#[derive(Clone, Copy, Default)]
struct Entry {
    references: usize,
    cursor: usize,
}

pub(super) enum Retired {
    Child(usize),
    Payload(LeaseId),
    Progress,
    Done,
}

pub(super) struct Tables {
    layout: Layout,
    roots: Box<[Entry]>,
    leaves: Box<[Entry]>,
    directories: Box<[Option<NonZeroU32>]>,
    cells: Box<[Option<LeaseId>]>,
    free_roots: Vec<usize>,
    free_leaves: Vec<usize>,
    pub allocated: u64,
    pub reclaimed: u64,
    pub released_payloads: usize,
    pub retirement_remaining: usize,
}

impl Tables {
    pub fn prepare(layout: Layout) -> Result<Self, Error> {
        let mut result = Self {
            layout,
            roots: vec![Entry::default(); layout.roots].into_boxed_slice(),
            leaves: vec![Entry::default(); layout.leaves].into_boxed_slice(),
            directories: vec![None; layout.roots * layout.width].into_boxed_slice(),
            cells: vec![None; layout.leaves * TILE].into_boxed_slice(),
            free_roots: (0..layout.roots).rev().collect(),
            free_leaves: (0..layout.leaves).rev().collect(),
            allocated: 0,
            reclaimed: 0,
            released_payloads: 0,
            retirement_remaining: 512,
        };
        for entry in result.roots.iter_mut().chain(result.leaves.iter_mut()) {
            *core::hint::black_box(entry) = Entry::default();
        }
        for cell in &mut result.directories {
            *core::hint::black_box(cell) = None;
        }
        for cell in &mut result.cells {
            *core::hint::black_box(cell) = None;
        }
        Ok(result)
    }
    pub fn reset_work(&mut self) {
        self.allocated = 0;
        self.reclaimed = 0;
        self.retirement_remaining = 512;
    }
    pub fn live(&self) -> usize {
        self.layout.nodes() - self.free_roots.len() - self.free_leaves.len()
    }
    pub fn credit(&self, leases: usize) -> usize {
        2 * self
            .free_roots
            .len()
            .min(self.free_leaves.len())
            .min(leases / TILE)
    }
    fn entry(&mut self, id: usize) -> &mut Entry {
        if id & 1 == 0 {
            &mut self.roots[id / 2]
        } else {
            &mut self.leaves[id / 2]
        }
    }
    pub fn retain(&mut self, id: usize) -> Result<(), Error> {
        let entry = self.entry(id);
        entry.references = entry.references.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(())
    }
    pub fn unreference(&mut self, id: usize) -> bool {
        let entry = self.entry(id);
        entry.references = entry.references.checked_sub(1).expect("owned table");
        entry.references == 0
    }
    fn new_root(&mut self) -> usize {
        let slot = self.free_roots.pop().expect("prepared directory credit");
        self.roots[slot] = Entry {
            references: 1,
            cursor: 0,
        };
        self.directories[slot * self.layout.width..(slot + 1) * self.layout.width].fill(None);
        self.allocated += 1;
        slot * 2
    }
    fn new_leaf(&mut self) -> usize {
        let slot = self.free_leaves.pop().expect("prepared tile credit");
        self.leaves[slot] = Entry {
            references: 1,
            cursor: 0,
        };
        self.cells[slot * TILE..(slot + 1) * TILE].fill(None);
        self.allocated += 1;
        slot
    }
    fn leaf(&self, root: usize, page: usize) -> Option<usize> {
        self.directories[root / 2 * self.layout.width + page / TILE].map(|v| v.get() as usize - 1)
    }
    pub fn find(&self, root: Option<usize>, page: usize) -> Option<LeaseId> {
        self.cells[self.leaf(root?, page)? * TILE + page % TILE]
    }
    fn copy_root(&mut self, root: Option<usize>) -> usize {
        let result = self.new_root();
        if let Some(root) = root {
            for column in 0..self.layout.width {
                let value = self.directories[root / 2 * self.layout.width + column];
                self.directories[result / 2 * self.layout.width + column] = value;
                if let Some(value) = value {
                    self.retain((value.get() as usize - 1) * 2 + 1)
                        .expect("prepared references");
                }
            }
        }
        result
    }
    pub fn set(
        &mut self,
        root: Option<usize>,
        page: usize,
        payload: LeaseId,
        exclusive: bool,
        pages: &mut SharedPages,
        store: &mut ResourceStore,
    ) -> usize {
        let result = match root.filter(|&r| exclusive && self.roots[r / 2].references == 1) {
            Some(root) => {
                self.retain(root).expect("owned directory");
                root
            }
            None => self.copy_root(root),
        };
        let column = result / 2 * self.layout.width + page / TILE;
        let old = self.leaf(result, page);
        let leaf = match old.filter(|&l| self.leaves[l].references == 1) {
            Some(leaf) => leaf,
            None => {
                let leaf = self.new_leaf();
                if let Some(old) = old {
                    for offset in 0..TILE {
                        self.cells[leaf * TILE + offset] = self.cells[old * TILE + offset]
                            .map(|p| pages.retain(p).expect("prepared lease credit"));
                    }
                    assert!(!self.unreference(old * 2 + 1));
                }
                self.directories[column] = NonZeroU32::new(leaf as u32 + 1);
                leaf
            }
        };
        if let Some(previous) = self.cells[leaf * TILE + page % TILE].replace(payload) {
            self.released_payloads += usize::from(
                pages
                    .reclaim_render(store, previous)
                    .expect("owned payload"),
            );
        }
        result
    }
    pub fn undo_append(
        &mut self,
        root: usize,
        page: usize,
        pages: &mut SharedPages,
        store: &mut ResourceStore,
    ) {
        let leaf = self.leaf(root, page).expect("staged tile");
        let payload = self.cells[leaf * TILE + page % TILE]
            .take()
            .expect("staged append");
        self.released_payloads += usize::from(
            pages
                .reclaim_render(store, payload)
                .expect("staged payload"),
        );
    }
    pub fn clip(
        &mut self,
        root: Option<usize>,
        ranges: &[Range<usize>; 2],
        pages: &mut SharedPages,
    ) -> Option<usize> {
        let root = root?;
        let result = self.new_root();
        for column in 0..self.layout.width {
            let low = column * TILE;
            let high = low + TILE;
            if ranges.iter().all(|r| r.end <= low || r.start >= high) {
                continue;
            }
            let Some(old) = self.leaf(root, low) else {
                continue;
            };
            let leaf = if ranges.iter().any(|r| r.start <= low && r.end >= high) {
                self.retain(old * 2 + 1).expect("owned tile");
                old
            } else {
                let leaf = self.new_leaf();
                for offset in 0..TILE {
                    if ranges.iter().any(|r| r.contains(&(low + offset))) {
                        self.cells[leaf * TILE + offset] = self.cells[old * TILE + offset]
                            .map(|p| pages.retain(p).expect("prepared clip leases"));
                    }
                }
                leaf
            };
            self.directories[result / 2 * self.layout.width + column] =
                NonZeroU32::new(leaf as u32 + 1);
        }
        Some(result)
    }
    pub fn retire_step(&mut self, id: usize) -> Retired {
        if id & 1 == 0 {
            let slot = id / 2;
            let cursor = self.roots[slot].cursor;
            if cursor == self.layout.width {
                self.free_roots.push(slot);
                self.reclaimed += 1;
                return Retired::Done;
            }
            self.roots[slot].cursor += 1;
            self.directories[slot * self.layout.width + cursor]
                .take()
                .map_or(Retired::Progress, |v| {
                    Retired::Child((v.get() as usize - 1) * 2 + 1)
                })
        } else {
            let slot = id / 2;
            let cursor = self.leaves[slot].cursor;
            if cursor == TILE {
                self.free_leaves.push(slot);
                self.reclaimed += 1;
                return Retired::Done;
            }
            self.leaves[slot].cursor += 1;
            self.cells[slot * TILE + cursor]
                .take()
                .map_or(Retired::Progress, Retired::Payload)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn retained_ranges_survive_flat_history_wrap_and_cancelled_capture() {
        let config = crate::Config {
            max_frames: 16,
            resource_pages: 256,
            page_bytes: 128,
            ..crate::Config::default()
        }
        .prepare()
        .unwrap();
        let mut resources = ResourceStore::new(config, crate::Quantum::new(16).unwrap()).unwrap();
        resources.set_audio_render_mode(crate::AudioRenderMode::Compact);
        let mut audio = AudioStore::prepare_histories_windowed(
            &mut resources,
            super::super::Config {
                channels: 2,
                history_frames: 64,
                views: 16,
                pages: 200,
            },
            Frame(0),
            1,
        )
        .unwrap();
        resources.begin_callback();
        audio.reset_work();
        let mut retained = Vec::with_capacity(16);
        assert_no_heap(|| {
            for frame in (0..64).step_by(16) {
                resources.begin_callback();
                audio.reset_work();
                audio
                    .append(
                        &mut resources.render_io(),
                        Frame(frame),
                        &[frame as f32; 32],
                    )
                    .unwrap();
            }
            let take = audio
                .grab(&mut resources.render_io(), Frame(32)..Frame(64))
                .unwrap();
            let export = audio.export(take).unwrap();
            for frame in (64..1024).step_by(16) {
                resources.begin_callback();
                audio.reset_work();
                audio
                    .append(
                        &mut resources.render_io(),
                        Frame(frame),
                        &[frame as f32; 32],
                    )
                    .unwrap();
                audio.maintain(&mut resources).unwrap();
            }
            let mut bytes = [0; 256];
            for frame in [0, 16] {
                audio
                    .read_bytes(
                        &mut resources.render_io(),
                        audio.export_index(export).unwrap(),
                        frame as u64,
                        &mut bytes[frame * 8..(frame + 16) * 8],
                    )
                    .unwrap();
            }
            let actual: [f32; 64] = core::array::from_fn(|i| {
                f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
            });
            assert_eq!(&actual[..32], &[32.; 32]);
            assert_eq!(&actual[32..], &[48.; 32]);
            let capture = audio.empty_take().unwrap();
            let reserved = audio.reserve_export(capture, 0).unwrap();
            for (commit, value) in [(true, 0.25), (false, 0.75), (true, 0.5)] {
                resources.begin_callback();
                audio.reset_work();
                let live = audio.work().payload_live;
                let write = audio
                    .reserve_capture_block(&mut resources.render_io(), capture, reserved)
                    .unwrap();
                audio
                    .fill_capture_block(&mut resources.render_io(), &write, &[value; 32])
                    .unwrap();
                if commit {
                    audio
                        .commit_capture_block(&mut resources.render_io(), write)
                        .unwrap();
                } else {
                    audio
                        .cancel_capture_block(&mut resources.render_io(), write)
                        .unwrap();
                    assert_eq!(audio.work().payload_live, live);
                }
                audio.maintain(&mut resources).unwrap();
            }
            assert_eq!(
                audio
                    .info(audio.take_index(capture).unwrap())
                    .unwrap()
                    .frames,
                32
            );
            let growing = audio.fork(take).unwrap();
            audio.reserve_growth(growing, 1024).unwrap();
            for step in 0..60 {
                resources.begin_callback();
                audio.reset_work();
                let live = audio.work().payload_live;
                let write = audio
                    .stage_write(
                        &mut resources.render_io(),
                        growing,
                        None,
                        &[step as f32; 32],
                    )
                    .unwrap();
                audio
                    .cancel_write(&mut resources.render_io(), write)
                    .unwrap();
                if step > 0 {
                    assert_eq!(audio.work().payload_live, live);
                }
                let write = audio
                    .stage_write(
                        &mut resources.render_io(),
                        growing,
                        None,
                        &[step as f32; 32],
                    )
                    .unwrap();
                audio
                    .commit_write(&mut resources.render_io(), write)
                    .unwrap();
                audio.maintain(&mut resources).unwrap();
            }
            audio.finish_growth(growing).unwrap();
            audio
                .read_bytes(
                    &mut resources.render_io(),
                    audio.export_index(export).unwrap(),
                    0,
                    &mut bytes[..128],
                )
                .unwrap();
            assert_eq!(f32::from_le_bytes(bytes[..4].try_into().unwrap()), 32.);
            audio.release_take(growing).unwrap();
            for _ in 0..16 {
                let live = audio.work().payload_live;
                let tables = audio.work().index_live;
                match audio.grab(&mut resources.render_io(), Frame(992)..Frame(1024)) {
                    Ok(take) => retained.push(take),
                    Err(Error::Capacity) => {
                        assert_eq!(audio.work().payload_live, live);
                        assert_eq!(audio.work().index_live, tables);
                        break;
                    }
                    Err(error) => panic!("grab admission {error:?}"),
                }
            }
            assert_eq!(audio.free_views.len(), 0);
            resources.begin_callback();
            audio.reset_work();
            audio
                .append(&mut resources.render_io(), Frame(1024), &[1024.; 32])
                .unwrap();
            for take in retained.drain(..) {
                audio.release_take(take).unwrap();
            }
            audio.release_take(capture).unwrap();
            audio.release_export(reserved).unwrap();
            audio.release_take(take).unwrap();
            audio.release_export(export).unwrap();
            for _ in 0..2000 {
                resources.begin_callback();
                audio.reset_work();
                assert!(audio.maintain(&mut resources).unwrap() <= 32);
                if audio.debt == 0 {
                    break;
                }
            }
            audio.begin_cleanup().unwrap();
            for _ in 0..2000 {
                resources.begin_callback();
                audio.reset_work();
                assert!(audio.maintain(&mut resources).unwrap() <= 32);
                if audio.work().index_live == 0 {
                    break;
                }
            }
            assert_eq!(audio.work().payload_live, 0);
            assert_eq!(audio.work().index_live, 0);
        });
    }
}
