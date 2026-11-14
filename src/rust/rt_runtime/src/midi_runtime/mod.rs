//! Fixed-record MIDI memory with explicit version ownership and bounded work.

use crate::{midi::EventRecord, Error, Frame};
use alloc::{boxed::Box, vec};
use core::mem::{size_of, size_of_val};
use core::ops::Range;
use core::sync::atomic::{AtomicU32, Ordering};
mod pool;
use pool::{Pool, Queue};
mod positions;
pub use positions::{PositionId, PositionJobs, PositionStatus, PositionWork, PreparedPosition};
mod prune;
mod writer;
pub use writer::{InputStamp, MediaImage, WriteMode, WriteSpan, WriterId, WriterSet};

pub const RECORDS_PER_PAGE: usize = 32;
const RADIX: usize = 16;
const LEVELS: usize = 16;
static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub pages: usize,
    pub tiles: usize,
    pub directories: usize,
    pub versions: usize,
    pub tile_shift: u8,
    pub pages_per_tile: usize,
    pub touched_tiles: usize,
}
impl Profile {
    pub fn validate(self) -> Result<Self, Error> {
        if [
            self.pages,
            self.tiles,
            self.directories,
            self.versions,
            self.pages_per_tile,
        ]
        .contains(&0)
            || self.tile_shift >= 64
            || self.pages_per_tile > u16::MAX as usize
            || self.touched_tiles == 0
        {
            return Err(Error::InvalidConfig);
        }
        let objects = self
            .pages
            .checked_add(self.tiles)
            .and_then(|n| n.checked_add(self.directories))
            .and_then(|n| n.checked_add(self.versions))
            .ok_or(Error::Capacity)?;
        let references = self
            .directories
            .checked_mul(RADIX)
            .and_then(|n| n.checked_add(self.versions))
            .ok_or(Error::Capacity)?;
        let tile_words = self
            .tiles
            .checked_mul(self.pages_per_tile)
            .ok_or(Error::Capacity)?;
        let bytes = self
            .pages
            .checked_mul(size_of::<Page>())
            .and_then(|n| n.checked_add(self.directories.checked_mul(size_of::<Directory>())?))
            .and_then(|n| n.checked_add(tile_words.checked_mul(4)?))
            .and_then(|n| n.checked_add(objects.checked_mul(64)?))
            .ok_or(Error::Capacity)?;
        if references >= u32::MAX as usize
            || objects >= u32::MAX as usize
            || bytes > i32::MAX as usize
        {
            return Err(Error::Capacity);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Totals {
    pub events: u64,
    pub bytes: u64,
    pub pages: u64,
}
impl Totals {
    fn then(self, other: Self) -> Self {
        Self {
            events: self.events + other.events,
            bytes: self.bytes + other.bytes,
            pages: self.pages + other.pages,
        }
    }
}
#[derive(Clone, Copy)]
struct Directory {
    children: [u32; RADIX],
    level: u8,
    totals: Totals,
    first: u64,
    last: u64,
}
#[derive(Clone, Copy)]
struct Tile {
    coordinate: u64,
    count: u32,
    pages: u16,
    bytes: u64,
}
#[derive(Clone, Copy)]
struct Page {
    records: [[u8; EventRecord::ENCODED_BYTES]; RECORDS_PER_PAGE],
}
#[derive(Clone, Copy)]
struct View {
    root: u32,
    visible: Option<(u64, u64)>,
}

#[derive(Clone, Copy)]
struct EditRange {
    start: Frame,
    end: Frame,
    erase: bool,
}
impl EditRange {
    fn contains(self, frame: Frame) -> bool {
        self.erase && self.start <= frame && frame < self.end
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Version {
    scope: u32,
    slot: u32,
    generation: u64,
}
#[derive(Clone, Copy)]
enum Release {
    Directory { id: u32, next: u8 },
    Tile { id: u32, next: u16 },
    Page(u32),
    Version(u32),
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Work {
    pub directory_reads: u64,
    pub record_reads: u64,
    pub record_writes: u64,
    pub directory_writes: u64,
    pub retirement_steps: u64,
    pub counter_overflow: bool,
}
fn count_work(counter: &mut u64, amount: u64, overflow: &mut bool) {
    match counter.checked_add(amount) {
        Some(next) => *counter = next,
        None => {
            *counter = u64::MAX;
            *overflow = true;
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    pub pages: usize,
    pub tiles: usize,
    pub directories: usize,
    pub versions: usize,
    pub retirement: usize,
    pub leases: usize,
}

pub struct Store {
    scope: u32,
    profile: Profile,
    pages: Pool<Page>,
    tiles: Pool<Tile>,
    directories: Pool<Directory>,
    versions: Pool<View>,
    generations: Box<[u64]>,
    tile_pages: Box<[u32]>,
    retirement: Queue<Release>,
    work: Work,
    leases: usize,
    exhausted: bool,
}
impl Store {
    pub fn prepare(profile: Profile) -> Result<Self, Error> {
        let profile = profile.validate()?;
        let scope = NEXT_SCOPE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        Ok(Self {
            scope,
            profile,
            pages: Pool::new(profile.pages),
            tiles: Pool::new(profile.tiles),
            directories: Pool::new(profile.directories),
            versions: Pool::new(profile.versions),
            generations: vec![1; profile.versions].into_boxed_slice(),
            tile_pages: vec![0; profile.tiles * profile.pages_per_tile].into_boxed_slice(),
            retirement: Queue::new(
                profile.pages + profile.tiles + profile.directories + profile.versions,
            ),
            work: Work::default(),
            leases: 0,
            exhausted: false,
        })
    }
    pub fn allocated_bytes(&self) -> usize {
        self.pages.allocated_bytes()
            + self.tiles.allocated_bytes()
            + self.directories.allocated_bytes()
            + self.versions.allocated_bytes()
            + size_of_val(&*self.generations)
            + size_of_val(&*self.tile_pages)
            + self.retirement.allocated_bytes()
    }
    pub fn usage(&self) -> Usage {
        Usage {
            pages: self.profile.pages - self.pages.available(),
            tiles: self.profile.tiles - self.tiles.available(),
            directories: self.profile.directories - self.directories.available(),
            versions: self.profile.versions - self.versions.available(),
            retirement: self.retirement.len(),
            leases: self.leases,
        }
    }
    pub fn work(&self) -> Work {
        self.work
    }
    pub fn reset_work(&mut self) {
        self.work = Work::default();
    }
    fn view(&self, version: Version) -> Result<View, Error> {
        if version.scope != self.scope
            || self.generations.get(version.slot.wrapping_sub(1) as usize)
                != Some(&version.generation)
        {
            return Err(Error::InvalidId);
        }
        self.versions.live(version.slot)
    }
    fn publish(&mut self, view: View) -> Version {
        self.leases += 1;
        let slot = self.versions.insert(view);
        Version {
            scope: self.scope,
            slot,
            generation: self.generations[slot as usize - 1],
        }
    }
    pub fn empty(&mut self) -> Result<Version, Error> {
        self.version_credit()?;
        Ok(self.publish(View {
            root: 0,
            visible: None,
        }))
    }
    pub fn retain(&mut self, version: Version) -> Result<Version, Error> {
        let view = self.view(version)?;
        self.version_credit()?;
        self.directories.retain(view.root);
        Ok(self.publish(view))
    }
    pub fn release(&mut self, version: Version) -> Result<(), Error> {
        self.view(version)?;
        self.leases -= 1;
        if self.versions.release(version.slot) {
            self.retirement.push(Release::Version(version.slot));
        }
        Ok(())
    }
    pub fn clip(&mut self, version: Version, visible: Range<Frame>) -> Result<Version, Error> {
        let mut view = self.view(version)?;
        if visible.start > visible.end {
            return Err(Error::InvalidFrame);
        }
        self.version_credit()?;
        let (start, end) = view.visible.unwrap_or((0, u64::MAX));
        view.visible = Some((
            start.max(visible.start.0),
            end.min(visible.end.0).max(start.max(visible.start.0)),
        ));
        self.directories.retain(view.root);
        Ok(self.publish(view))
    }
    fn tile_page(&self, tile: u32, page: usize) -> u32 {
        self.tile_pages[(tile as usize - 1) * self.profile.pages_per_tile + page]
    }
    fn record(&mut self, tile: u32, index: usize) -> EventRecord {
        count_work(
            &mut self.work.record_reads,
            1,
            &mut self.work.counter_overflow,
        );
        let page = self
            .pages
            .row(self.tile_page(tile, index / RECORDS_PER_PAGE));
        EventRecord::decode(page.records[index % RECORDS_PER_PAGE])
            .expect("validated immutable record")
    }
    fn tile_at(&mut self, mut root: u32, coordinate: u64) -> u32 {
        for level in (0..LEVELS).rev() {
            if root == 0 {
                return 0;
            }
            count_work(
                &mut self.work.directory_reads,
                1,
                &mut self.work.counter_overflow,
            );
            root =
                self.directories.value(root).children[((coordinate >> (level * 4)) & 15) as usize];
        }
        root
    }
    fn tile_after(&mut self, mut root: u32, coordinate: u64) -> u32 {
        if root == 0 {
            return 0;
        }
        for level in (0..LEVELS).rev() {
            count_work(
                &mut self.work.directory_reads,
                1,
                &mut self.work.counter_overflow,
            );
            let row = self.directories.value(root);
            let mut next = 0;
            for &child in &row.children {
                if child != 0 {
                    let last = if level == 0 {
                        self.tiles.value(child).coordinate
                    } else {
                        self.directories.value(child).last
                    };
                    if last >= coordinate {
                        next = child;
                        break;
                    }
                }
            }
            if next == 0 {
                return 0;
            }
            root = next;
        }
        root
    }
    pub fn first(
        &mut self,
        version: Version,
        after: Option<(Frame, u16)>,
    ) -> Result<Option<EventRecord>, Error> {
        let view = self.view(version)?;
        let (start, end) = view.visible.unwrap_or((0, u64::MAX));
        let frame = after.map_or(start, |(frame, _)| frame.0.max(start));
        let mut coordinate = frame >> self.profile.tile_shift;
        loop {
            let tile = self.tile_after(view.root, coordinate);
            if tile == 0 {
                return Ok(None);
            }
            let row = self.tiles.value(tile);
            if (row.coordinate << self.profile.tile_shift) >= end {
                return Ok(None);
            }
            for index in 0..row.count as usize {
                let event = self.record(tile, index);
                if event.frame.0 >= end {
                    return Ok(None);
                }
                if event.frame.0 >= start
                    && after.is_none_or(|point| (event.frame, event.order) > point)
                {
                    return Ok(Some(event));
                }
            }
            coordinate = match row.coordinate.checked_add(1) {
                Some(next) => next,
                None => return Ok(None),
            };
        }
    }
    fn prefix(&mut self, mut root: u32, frame: u64) -> Totals {
        let coordinate = frame >> self.profile.tile_shift;
        let mut result = Totals::default();
        for level in (0..LEVELS).rev() {
            if root == 0 {
                return result;
            }
            count_work(
                &mut self.work.directory_reads,
                1,
                &mut self.work.counter_overflow,
            );
            let row = self.directories.value(root);
            let index = ((coordinate >> (level * 4)) & 15) as usize;
            for &child in &row.children[..index] {
                if child != 0 {
                    result = result.then(if level == 0 {
                        self.tile_totals(child)
                    } else {
                        self.directories.value(child).totals
                    });
                }
            }
            root = row.children[index];
        }
        if root != 0 {
            for index in 0..self.tiles.value(root).count as usize {
                let event = self.record(root, index);
                if event.frame.0 >= frame {
                    break;
                }
                result.events += 1;
                result.bytes += 12 + event.message().as_bytes().len() as u64;
            }
        }
        result
    }
    fn tile_totals(&self, tile: u32) -> Totals {
        let row = self.tiles.value(tile);
        Totals {
            events: u64::from(row.count),
            bytes: row.bytes,
            pages: u64::from(row.pages),
        }
    }
    pub fn totals(&mut self, version: Version) -> Result<Totals, Error> {
        let view = self.view(version)?;
        if view.root == 0 {
            return Ok(Totals::default());
        }
        let total = self.directories.value(view.root).totals;
        let Some((start, end)) = view.visible else {
            return Ok(total);
        };
        let a = self.prefix(view.root, start);
        let b = self.prefix(view.root, end);
        Ok(Totals {
            events: b.events - a.events,
            bytes: b.bytes - a.bytes,
            pages: total.pages,
        })
    }
    pub fn byte_before(&mut self, version: Version, frame: Frame) -> Result<u64, Error> {
        let view = self.view(version)?;
        let (start, end) = view.visible.unwrap_or((0, u64::MAX));
        Ok(self.prefix(view.root, frame.0.max(start).min(end)).bytes
            - self.prefix(view.root, start).bytes)
    }
    pub fn record_at_byte(
        &mut self,
        version: Version,
        offset: u64,
    ) -> Result<Option<(EventRecord, usize)>, Error> {
        let view = self.view(version)?;
        if offset >= self.totals(version)?.bytes {
            return Ok(None);
        }
        let mut offset = offset
            + view
                .visible
                .map_or(0, |(start, _)| self.prefix(view.root, start).bytes);
        let mut root = view.root;
        for level in (0..LEVELS).rev() {
            count_work(
                &mut self.work.directory_reads,
                1,
                &mut self.work.counter_overflow,
            );
            let row = self.directories.value(root);
            let mut next = 0;
            for &child in &row.children {
                if child == 0 {
                    continue;
                }
                let totals = if level == 0 {
                    self.tile_totals(child)
                } else {
                    self.directories.value(child).totals
                };
                if offset < totals.bytes {
                    next = child;
                    break;
                }
                offset -= totals.bytes;
            }
            root = next;
        }
        for index in 0..self.tiles.value(root).count as usize {
            let event = self.record(root, index);
            let bytes = 12 + event.message().as_bytes().len() as u64;
            if offset < bytes {
                return Ok(Some((event, offset as usize)));
            }
            offset -= bytes;
        }
        Err(Error::InvalidState)
    }
    fn release_directory(&mut self, id: u32) {
        if self.directories.release(id) {
            self.retirement.push(Release::Directory { id, next: 0 });
        }
    }
    fn release_tile(&mut self, id: u32) {
        if self.tiles.release(id) {
            self.retirement.push(Release::Tile { id, next: 0 });
        }
    }
    fn replace_tile(&mut self, root: u32, coordinate: u64, tile: u32) -> u32 {
        let mut path = [0; LEVELS];
        let mut old = root;
        for level in (0..LEVELS).rev() {
            path[level] = old;
            old = if old == 0 {
                0
            } else {
                self.directories.value(old).children[((coordinate >> (level * 4)) & 15) as usize]
            };
        }
        let mut child = tile;
        for level in 0..LEVELS {
            let mut children = if path[level] == 0 {
                [0; RADIX]
            } else {
                self.directories.value(path[level]).children
            };
            let changed = ((coordinate >> (level * 4)) & 15) as usize;
            children[changed] = child;
            if children.iter().all(|&id| id == 0) {
                child = 0;
                continue;
            }
            let mut totals = Totals::default();
            let mut first = None;
            let mut last = 0;
            for (index, &id) in children.iter().enumerate() {
                if id == 0 {
                    continue;
                }
                if level == 0 {
                    let row = self.tiles.value(id);
                    if index != changed {
                        self.tiles.retain(id);
                    }
                    first.get_or_insert(row.coordinate);
                    last = row.coordinate;
                    totals = totals.then(self.tile_totals(id));
                } else {
                    let row = self.directories.value(id);
                    if index != changed {
                        self.directories.retain(id);
                    }
                    first.get_or_insert(row.first);
                    last = row.last;
                    totals = totals.then(row.totals);
                }
            }
            child = self.directories.insert(Directory {
                children,
                level: level as u8,
                totals,
                first: first.unwrap(),
                last,
            });
            count_work(
                &mut self.work.directory_writes,
                1,
                &mut self.work.counter_overflow,
            );
        }
        child
    }
    fn outside(&mut self, tile: u32, range: EditRange) -> usize {
        if tile == 0 {
            return 0;
        }
        let mut count = 0;
        for index in 0..self.tiles.value(tile).count as usize {
            let event = self.record(tile, index);
            count += usize::from(!range.contains(event.frame));
        }
        count
    }
    fn build_tile(
        &mut self,
        coordinate: u64,
        old: u32,
        range: EditRange,
        input: &[EventRecord],
        count: usize,
    ) -> u32 {
        if count == 0 {
            return 0;
        }
        let pages = count.div_ceil(RECORDS_PER_PAGE);
        let tile = self.tiles.insert(Tile {
            coordinate,
            count: count as u32,
            pages: pages as u16,
            bytes: 0,
        });
        for page in 0..pages {
            let id = self.pages.insert(Page {
                records: [[0; EventRecord::ENCODED_BYTES]; RECORDS_PER_PAGE],
            });
            self.tile_pages[(tile as usize - 1) * self.profile.pages_per_tile + page] = id;
            count_work(
                &mut self.work.record_writes,
                RECORDS_PER_PAGE as u64,
                &mut self.work.counter_overflow,
            );
        }
        let old_count = if old == 0 {
            0
        } else {
            self.tiles.value(old).count as usize
        };
        let mut previous = 0;
        let mut incoming = 0;
        let mut old_event = None;
        let mut new_event = None;
        let mut bytes = 0;
        for index in 0..count {
            while old_event.is_none() && previous < old_count {
                let event = self.record(old, previous);
                previous += 1;
                if !range.contains(event.frame) {
                    old_event = Some(event);
                }
            }
            while new_event.is_none() && incoming < input.len() {
                let event = input[incoming];
                incoming += 1;
                if event.frame.0 >> self.profile.tile_shift == coordinate {
                    new_event = Some(event);
                }
            }
            let event = match (old_event, new_event) {
                (Some(a), Some(b)) if (a.frame, a.order) <= (b.frame, b.order) => {
                    old_event = None;
                    a
                }
                (_, Some(b)) => {
                    new_event = None;
                    b
                }
                (Some(a), None) => {
                    old_event = None;
                    a
                }
                _ => unreachable!("preflighted tile count"),
            };
            let page = self.tile_page(tile, index / RECORDS_PER_PAGE);
            self.pages.row_mut(page).records[index % RECORDS_PER_PAGE] = event.encode();
            bytes += 12 + event.message().as_bytes().len() as u64;
            count_work(
                &mut self.work.record_writes,
                1,
                &mut self.work.counter_overflow,
            );
        }
        let mut row = self.tiles.value(tile);
        row.bytes = bytes;
        self.tiles.replace(tile, row);
        tile
    }
    pub fn edit(
        &mut self,
        version: Version,
        range: Range<Frame>,
        input: &[EventRecord],
    ) -> Result<Version, Error> {
        self.patch(version, range, input, true)
    }
    pub fn append(&mut self, version: Version, input: &[EventRecord]) -> Result<Version, Error> {
        let Some(first) = input.first() else {
            return self.retain(version);
        };
        let last = input.last().unwrap();
        let view = self.view(version)?;
        if view.root != 0 {
            let tile = self.tile_at(view.root, self.directories.value(view.root).last);
            let tail = self.record(tile, self.tiles.value(tile).count as usize - 1);
            if (first.frame, first.order) <= (tail.frame, tail.order) {
                return Err(Error::InvalidEvent);
            }
        }
        self.patch(
            version,
            first.frame..last.frame.checked_add(1)?,
            input,
            false,
        )
    }
    fn patch(
        &mut self,
        version: Version,
        range: Range<Frame>,
        input: &[EventRecord],
        erase: bool,
    ) -> Result<Version, Error> {
        let view = self.view(version)?;
        self.version_credit()?;
        if view.visible.is_some() {
            return Err(Error::Busy);
        }
        if range.start >= range.end
            || input.iter().any(|event| !range.contains(&event.frame))
            || input
                .windows(2)
                .any(|pair| (pair[0].frame, pair[0].order) >= (pair[1].frame, pair[1].order))
        {
            return Err(Error::InvalidEvent);
        }
        let first = range.start.0 >> self.profile.tile_shift;
        let last = (range.end.0 - 1) >> self.profile.tile_shift;
        let shift = self.profile.tile_shift;
        let distinct_input = || {
            input.iter().enumerate().filter_map(|(index, event)| {
                let coordinate = event.frame.0 >> shift;
                (index == 0 || input[index - 1].frame.0 >> shift != coordinate)
                    .then_some(coordinate)
            })
        };
        let touched = if erase {
            usize::try_from(last - first + 1).map_err(|_| Error::Capacity)?
        } else {
            distinct_input().count()
        };
        let coordinates = || {
            (first..=last)
                .take(if erase { touched } else { 0 })
                .chain(distinct_input().take(if erase { 0 } else { touched }))
        };
        if touched > self.profile.touched_tiles
            || self.versions.available() == 0
            || self.directories.available() < touched.saturating_mul(LEVELS)
        {
            return Err(Error::Capacity);
        }
        let mut needed_pages = 0usize;
        let mut needed_tiles = 0;
        let selection = EditRange {
            start: range.start,
            end: range.end,
            erase,
        };
        for coordinate in coordinates() {
            let old = self.tile_at(view.root, coordinate);
            let count = self.outside(old, selection)
                + input
                    .iter()
                    .filter(|event| event.frame.0 >> self.profile.tile_shift == coordinate)
                    .count();
            let pages = count.div_ceil(RECORDS_PER_PAGE);
            if pages > self.profile.pages_per_tile {
                return Err(Error::Capacity);
            }
            needed_pages = needed_pages.checked_add(pages).ok_or(Error::Capacity)?;
            needed_tiles += usize::from(count != 0);
        }
        if needed_pages > self.pages.available() || needed_tiles > self.tiles.available() {
            return Err(Error::Capacity);
        }
        let mut root = view.root;
        self.directories.retain(root);
        for coordinate in coordinates() {
            let old = self.tile_at(root, coordinate);
            let count = self.outside(old, selection)
                + input
                    .iter()
                    .filter(|event| event.frame.0 >> self.profile.tile_shift == coordinate)
                    .count();
            let tile = self.build_tile(coordinate, old, selection, input, count);
            let next = self.replace_tile(root, coordinate, tile);
            self.release_directory(root);
            root = next;
        }
        Ok(self.publish(View {
            root,
            visible: None,
        }))
    }
    pub fn maintain(&mut self, steps: usize) -> usize {
        let mut completed = 0;
        while completed < steps {
            let Some(job) = self.retirement.pop() else {
                break;
            };
            match job {
                Release::Version(id) => {
                    let view = self.versions.value(id);
                    self.release_directory(view.root);
                    let generation = &mut self.generations[id as usize - 1];
                    if let Some(next) = generation.checked_add(1) {
                        *generation = next;
                        self.versions.reclaim(id);
                    } else {
                        self.exhausted = true;
                    }
                }
                Release::Directory { id, next } => {
                    let row = self.directories.value(id);
                    let child = row.children[usize::from(next)];
                    if next as usize + 1 == RADIX {
                        self.directories.reclaim(id);
                    } else {
                        self.retirement
                            .push(Release::Directory { id, next: next + 1 });
                    }
                    if row.level == 0 {
                        self.release_tile(child);
                    } else {
                        self.release_directory(child);
                    }
                }
                Release::Tile { id, next } => {
                    let row = self.tiles.value(id);
                    let page = self.tile_page(id, usize::from(next));
                    if next + 1 == row.pages {
                        self.tiles.reclaim(id);
                    } else {
                        self.retirement.push(Release::Tile { id, next: next + 1 });
                    }
                    if self.pages.release(page) {
                        self.retirement.push(Release::Page(page));
                    }
                }
                Release::Page(id) => self.pages.reclaim(id),
            }
            completed += 1;
        }
        count_work(
            &mut self.work.retirement_steps,
            completed as u64,
            &mut self.work.counter_overflow,
        );
        completed
    }
    fn version_credit(&self) -> Result<(), Error> {
        if self.versions.available() == 0 || self.leases == self.profile.versions {
            return Err(if self.exhausted {
                Error::Exhausted
            } else {
                Error::Capacity
            });
        }
        Ok(())
    }
}
