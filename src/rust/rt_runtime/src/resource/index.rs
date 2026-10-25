//! Append-maintained page index. Every index node lives in its data-page slot.
use super::*;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_STORE: AtomicU32 = AtomicU32::new(1);
pub(super) fn new_scope() -> Result<u32, Error> {
    allocate_scope(&NEXT_STORE)
}
fn allocate_scope(counter: &AtomicU32) -> Result<u32, Error> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| Error::Exhausted)
}
type Link = Option<NonZeroU32>;
fn link(page: usize) -> NonZeroU32 {
    NonZeroU32::new(page as u32 + 1).expect("prepared page bound")
}
fn position(link: NonZeroU32) -> usize {
    link.get() as usize - 1
}

#[derive(Clone, Copy, Default)]
pub(super) struct PageIndex {
    left: Link,
    right: Link,
    parent: Link,
    ordinal: u32,
    height: u8,
}
#[derive(Clone, Copy, Default)]
pub(super) struct VersionIndex {
    root: Link,
    pages: u32,
}

/// Borrowed capability over one existing snapshot lease, not an additional lease.
/// It is checked against its store instance and lease generation on every seek.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadIndex {
    scope: u32,
    snapshot: SnapshotId,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IndexWork {
    pub background_steps: usize,
    pub render_steps: usize,
    pub counter_overflow: bool,
}
impl ResourceStore {
    pub fn index_work(&self) -> IndexWork {
        self.index_work
    }
    fn index_step(&mut self, class: WorkClass) {
        let counter = match class {
            WorkClass::Background => &mut self.index_work.background_steps,
            WorkClass::Render => &mut self.index_work.render_steps,
        };
        match counter.checked_add(1) {
            Some(next) => *counter = next,
            None => {
                *counter = usize::MAX;
                self.index_work.counter_overflow = true;
            }
        }
    }
    fn height(&self, node: Link) -> u8 {
        node.map_or(0, |node| self.pages[position(node)].index.height)
    }
    fn update_height(&mut self, node: usize) {
        let tree = self.pages[node].index;
        self.pages[node].index.height = 1 + self.height(tree.left).max(self.height(tree.right));
    }
    fn rotate_left(&mut self, version: &mut Version, node: usize) -> usize {
        let parent = self.pages[node].index.parent;
        let right = position(self.pages[node].index.right.expect("right-heavy subtree"));
        let middle = self.pages[right].index.left;
        self.pages[node].index.right = middle;
        if let Some(middle) = middle {
            self.pages[position(middle)].index.parent = Some(link(node));
        }
        self.pages[right].index.left = Some(link(node));
        self.pages[node].index.parent = Some(link(right));
        self.pages[right].index.parent = parent;
        if let Some(parent) = parent {
            let parent = &mut self.pages[position(parent)].index;
            if parent.left == Some(link(node)) {
                parent.left = Some(link(right));
            } else {
                assert_eq!(parent.right, Some(link(node)));
                parent.right = Some(link(right));
            }
        } else {
            version.index.root = Some(link(right));
        }
        self.update_height(node);
        self.update_height(right);
        right
    }
    pub(super) fn index_append(&mut self, version: &mut Version, page: usize, class: WorkClass) {
        self.index_step(class);
        self.pages[page].index = PageIndex {
            parent: version.tail.map(link),
            ordinal: version.index.pages,
            height: 1,
            ..PageIndex::default()
        };
        version.index.pages += 1;
        if let Some(tail) = version.tail {
            self.pages[tail].index.right = Some(link(page));
        } else {
            version.index.root = Some(link(page));
        }
        let mut parent = self.pages[page].index.parent;
        while let Some(node) = parent {
            self.index_step(class);
            let node = position(node);
            self.update_height(node);
            let tree = self.pages[node].index;
            // Keys only append at the rightmost leaf, so no double/left rotations arise.
            let root = if self.height(tree.right) > self.height(tree.left) + 1 {
                self.index_step(class);
                self.rotate_left(version, node)
            } else {
                node
            };
            parent = self.pages[root].index.parent;
        }
    }
    pub(super) fn index_lease(&self, index: ReadIndex) -> Result<Lease, Error> {
        if index.scope != self.scope {
            return Err(Error::InvalidId);
        }
        get(
            &self.snapshots,
            index.snapshot.slot,
            index.snapshot.generation,
        )
    }
    /// Constant metadata work; the page index was maintained while content arrived.
    /// The supplied legacy snapshot ID must belong to this store.
    pub fn read_index(&self, snapshot: SnapshotId) -> Result<ReadIndex, Error> {
        let lease = get(&self.snapshots, snapshot.slot, snapshot.generation)?;
        get(&self.versions, lease.version.slot, lease.version.generation)?;
        Ok(ReadIndex {
            scope: self.scope,
            snapshot,
        })
    }
    /// Byte positioning in logarithmic page-index steps, without scanning content.
    pub fn cursor_at(&mut self, index: ReadIndex, offset: usize) -> Result<Cursor, Error> {
        self.cursor_at_with(index, offset, WorkClass::Background)
    }
    pub(super) fn cursor_at_with(
        &mut self,
        index: ReadIndex,
        offset: usize,
        class: WorkClass,
    ) -> Result<Cursor, Error> {
        self.note_operation(class);
        if index.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let lease = get(
            &self.snapshots,
            index.snapshot.slot,
            index.snapshot.generation,
        )?;
        if offset > lease.snapshot.description.bytes {
            return Err(Error::InvalidOffset);
        }
        let version = get(&self.versions, lease.version.slot, lease.version.generation)?;
        let mut cursor = Cursor {
            scope: self.scope,
            snapshot: index.snapshot,
            position: offset,
            page: None,
        };
        if offset == lease.snapshot.description.bytes {
            return Ok(cursor);
        }
        let ordinal = (offset / self.config.limits().page_bytes) as u32;
        let mut node = version.index.root;
        while let Some(current) = node {
            self.index_step(class);
            let page = position(current);
            let tree = self.pages[page].index;
            match ordinal.cmp(&tree.ordinal) {
                std::cmp::Ordering::Less => node = tree.left,
                std::cmp::Ordering::Greater => node = tree.right,
                std::cmp::Ordering::Equal => {
                    cursor.page = Some(page);
                    return Ok(cursor);
                }
            }
        }
        Err(Error::InvalidState)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    fn check(store: &ResourceStore, node: Link, parent: Link, seen: &mut Vec<usize>) -> u8 {
        let Some(node) = node else {
            return 0;
        };
        let page = position(node);
        let tree = store.pages[page].index;
        assert_eq!(tree.parent, parent);
        let left = check(store, tree.left, Some(node), seen);
        assert_eq!(tree.ordinal as usize, seen.len());
        seen.push(page);
        let right = check(store, tree.right, Some(node), seen);
        assert!(left.abs_diff(right) <= 1);
        assert_eq!(tree.height, 1 + left.max(right));
        tree.height
    }
    #[shoop_test]
    fn append_index_is_balanced_and_matches_the_independent_page_chain() {
        let config = crate::Config {
            resource_slots: 2,
            resource_pages: 1024,
            page_bytes: 4,
            budgets: crate::Budgets {
                transfer_bytes: 4,
                validation_steps: 1,
                ..crate::Budgets::default()
            },
            ..crate::Config::default()
        }
        .prepare()
        .unwrap();
        let mut store = ResourceStore::new(config).unwrap();
        let resource = store.create().unwrap();
        let upload = store
            .begin_upload(
                resource,
                Revision(0),
                Description {
                    format: Format::Audio { channels: 1 },
                    frames: 1024,
                    bytes: 4096,
                },
            )
            .unwrap();
        for ordinal in 0..1024 {
            store.begin_callback();
            store
                .write_upload(upload, ordinal * 4, &(ordinal as f32).to_le_bytes())
                .unwrap();
            let version = get(&store.versions, upload.slot, upload.generation).unwrap();
            let mut seen = Vec::new();
            check(&store, version.index.root, None, &mut seen);
            assert_eq!(seen.len(), ordinal + 1);
            let mut next = version.head;
            for page in seen {
                assert_eq!(next, Some(page));
                next = store.pages[page].next;
            }
            assert_eq!(next, None);
        }
    }
    #[shoop_test]
    fn scope_and_index_counter_exhaustion_never_wrap() {
        let counter = AtomicU32::new(u32::MAX - 1);
        assert_eq!(allocate_scope(&counter), Ok(u32::MAX - 1));
        assert_eq!(allocate_scope(&counter), Err(Error::Exhausted));
        assert_eq!(allocate_scope(&counter), Err(Error::Exhausted));
        let mut store = ResourceStore::new(crate::Config::default().prepare().unwrap()).unwrap();
        store.index_work.background_steps = usize::MAX;
        store.index_work.render_steps = usize::MAX;
        store.index_step(WorkClass::Background);
        store.index_step(WorkClass::Render);
        assert_eq!(
            store.index_work,
            IndexWork {
                background_steps: usize::MAX,
                render_steps: usize::MAX,
                counter_overflow: true
            }
        );
    }
}
