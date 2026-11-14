use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Selection {
    None,
    All,
    Edge,
}

#[derive(Clone, Copy)]
struct Walk {
    source: u32,
    next: usize,
    children: [u32; RADIX],
}

impl Store {
    pub fn is_normalized(&self, version: Version) -> Result<bool, Error> {
        Ok(self.view(version)?.visible.is_none())
    }
    fn select_coordinates(&self, first: u64, last: u64, start: u64, end: u64) -> Selection {
        if start >= end {
            return Selection::None;
        }
        let low = start >> self.profile.tile_shift;
        let high = (end - 1) >> self.profile.tile_shift;
        if last < low || first > high {
            return Selection::None;
        }
        let mask = (1u64 << self.profile.tile_shift) - 1;
        if (first > low || start & mask == 0) && (last < high || end & mask == 0) {
            Selection::All
        } else {
            Selection::Edge
        }
    }
    fn visible_count(&mut self, tile: u32, start: u64, end: u64) -> usize {
        let mut count = 0;
        for index in 0..self.tiles.value(tile).count as usize {
            let frame = self.record(tile, index).frame.0;
            count += usize::from(start <= frame && frame < end);
        }
        count
    }
    fn copy_visible_tile(&mut self, tile: u32, start: u64, end: u64) -> u32 {
        let old = self.tiles.value(tile);
        let count = self.visible_count(tile, start, end);
        if count == 0 {
            return 0;
        }
        if count == old.count as usize {
            self.tiles.retain(tile);
            return tile;
        }
        let pages = count.div_ceil(RECORDS_PER_PAGE);
        let next = self.tiles.insert(Tile {
            coordinate: old.coordinate,
            count: count as u32,
            pages: pages as u16,
            bytes: 0,
        });
        for page in 0..pages {
            let id = self.pages.insert(Page {
                records: [[0; EventRecord::ENCODED_BYTES]; RECORDS_PER_PAGE],
            });
            self.tile_pages[(next as usize - 1) * self.profile.pages_per_tile + page] = id;
            count_work(
                &mut self.work.record_writes,
                RECORDS_PER_PAGE as u64,
                &mut self.work.counter_overflow,
            );
        }
        let mut target = 0;
        let mut bytes = 0;
        for index in 0..old.count as usize {
            let event = self.record(tile, index);
            if start <= event.frame.0 && event.frame.0 < end {
                let page = self.tile_page(next, target / RECORDS_PER_PAGE);
                self.pages.row_mut(page).records[target % RECORDS_PER_PAGE] = event.encode();
                count_work(
                    &mut self.work.record_writes,
                    1,
                    &mut self.work.counter_overflow,
                );
                target += 1;
                bytes += 12 + event.message().as_bytes().len() as u64;
            }
        }
        let mut row = self.tiles.value(next);
        row.bytes = bytes;
        self.tiles.replace(next, row);
        next
    }
    fn make_directory(&mut self, level: u8, children: [u32; RADIX]) -> u32 {
        let mut totals = Totals::default();
        let mut first = None;
        let mut last = 0;
        for &child in &children {
            if child == 0 {
                continue;
            }
            if level == 0 {
                let row = self.tiles.value(child);
                totals = totals.then(self.tile_totals(child));
                first.get_or_insert(row.coordinate);
                last = row.coordinate;
            } else {
                let row = self.directories.value(child);
                totals = totals.then(row.totals);
                first.get_or_insert(row.first);
                last = row.last;
            }
        }
        let Some(first) = first else {
            return 0;
        };
        count_work(
            &mut self.work.directory_writes,
            1,
            &mut self.work.counter_overflow,
        );
        self.directories.insert(Directory {
            children,
            level,
            totals,
            first,
            last,
        })
    }
    pub fn normalize(&mut self, version: Version) -> Result<Version, Error> {
        let view = self.view(version)?;
        let Some((start, end)) = view.visible else {
            return self.retain(version);
        };
        self.version_credit()?;
        if view.root == 0 || start == end {
            return self.empty();
        }
        let root = self.directories.value(view.root);
        match self.select_coordinates(root.first, root.last, start, end) {
            Selection::None => return self.empty(),
            Selection::All => {
                self.directories.retain(view.root);
                return Ok(self.publish(View {
                    root: view.root,
                    visible: None,
                }));
            }
            Selection::Edge => {}
        }
        if self.directories.available() < 2 * LEVELS {
            return Err(Error::Capacity);
        }
        let first = start >> self.profile.tile_shift;
        let last = (end - 1) >> self.profile.tile_shift;
        let mut needed_pages = 0;
        let mut needed_tiles = 0;
        for coordinate in [Some(first), (last != first).then_some(last)]
            .into_iter()
            .flatten()
        {
            let tile = self.tile_at(view.root, coordinate);
            if tile == 0 {
                continue;
            }
            let count = self.visible_count(tile, start, end);
            if count != 0 && count != self.tiles.value(tile).count as usize {
                needed_tiles += 1;
                needed_pages += count.div_ceil(RECORDS_PER_PAGE);
            }
        }
        if self.pages.available() < needed_pages || self.tiles.available() < needed_tiles {
            return Err(Error::Capacity);
        }
        let mut stack = [Walk {
            source: 0,
            next: 0,
            children: [0; RADIX],
        }; LEVELS];
        stack[0].source = view.root;
        let mut depth = 0;
        let root = loop {
            let row = self.directories.value(stack[depth].source);
            count_work(
                &mut self.work.directory_reads,
                1,
                &mut self.work.counter_overflow,
            );
            if stack[depth].next == RADIX {
                let child = self.make_directory(row.level, stack[depth].children);
                if depth == 0 {
                    break child;
                }
                depth -= 1;
                stack[depth].children[stack[depth].next - 1] = child;
                continue;
            }
            let index = stack[depth].next;
            stack[depth].next += 1;
            let child = row.children[index];
            if child == 0 {
                continue;
            }
            if row.level == 0 {
                let coordinate = self.tiles.value(child).coordinate;
                match self.select_coordinates(coordinate, coordinate, start, end) {
                    Selection::None => {}
                    Selection::All => {
                        self.tiles.retain(child);
                        stack[depth].children[index] = child;
                    }
                    Selection::Edge => {
                        stack[depth].children[index] = self.copy_visible_tile(child, start, end);
                    }
                }
            } else {
                let next = self.directories.value(child);
                match self.select_coordinates(next.first, next.last, start, end) {
                    Selection::None => {}
                    Selection::All => {
                        self.directories.retain(child);
                        stack[depth].children[index] = child;
                    }
                    Selection::Edge => {
                        depth += 1;
                        stack[depth] = Walk {
                            source: child,
                            next: 0,
                            children: [0; RADIX],
                        };
                    }
                }
            }
        };
        Ok(self.publish(View {
            root,
            visible: None,
        }))
    }
}
