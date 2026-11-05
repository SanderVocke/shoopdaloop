//! Shared versions of the complete ordered packet and semantic indexes.

use super::{tree::*, Cut, Event, Key, Point, Timeline};
use crate::{midi_state::interval::*, Error};
use alloc::{vec, vec::Vec};
use core::{
    cell::Cell,
    num::NonZeroU32,
    sync::atomic::{AtomicU32, Ordering},
};

const LEAF: usize = 128;
const KEYS: usize = 8321;
const LEAVES: usize = KEYS.div_ceil(LEAF);
const HOLD: usize = 8288;
const SOSTENUTO: usize = 8304;
static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);

impl Key {
    fn slot(self) -> usize {
        let note = |base, channel, key| base + usize::from(channel) * 128 + usize::from(key);
        match self {
            Self::NoteOn(ch, key) => note(0, ch, key),
            Self::NoteOff(ch, key) => note(2048, ch, key),
            Self::KeyPressure(ch, key) => note(4096, ch, key),
            Self::Controller(ch, cc) => note(6144, ch, cc),
            Self::Program(ch) => 8192 + usize::from(ch),
            Self::Pressure(ch) => 8208 + usize::from(ch),
            Self::Pitch(ch) => 8224 + usize::from(ch),
            Self::ChannelOff(ch) => 8240 + usize::from(ch),
            Self::SoundOff(ch) => 8256 + usize::from(ch),
            Self::ResetControllers(ch) => 8272 + usize::from(ch),
            Self::Hold(ch) => HOLD + usize::from(ch),
            Self::Sostenuto(ch) => SOSTENUTO + usize::from(ch),
            Self::Reset => 8320,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Config {
    pub(crate) leases: usize,
    pub(crate) versions: usize,
    pub(crate) category_pages: usize,
    pub(crate) packet_nodes: usize,
    pub(crate) scalar_nodes: usize,
    pub(crate) hold_nodes: usize,
    pub(crate) sostenuto_nodes: usize,
}
impl Config {
    pub(crate) fn prepared_bytes<V: Copy>(self) -> Result<usize, Error> {
        if self.leases == 0
            || self.leases > u32::MAX as usize
            || self.versions == 0
            || self.versions > self.leases
            || self.category_pages == 0
            || [
                self.packet_nodes,
                self.scalar_nodes,
                self.hold_nodes,
                self.sostenuto_nodes,
            ]
            .contains(&0)
        {
            return Err(Error::InvalidConfig);
        }
        let mut bytes = self
            .versions
            .checked_mul(size_of::<TableSlot>() + 2 * size_of::<NonZeroU32>())
            .and_then(|bytes| {
                self.category_pages
                    .checked_mul(size_of::<CategoryPage>() + 2 * size_of::<NonZeroU32>())
                    .and_then(|pages| bytes.checked_add(pages))
            })
            .ok_or(Error::Capacity)?;
        for more in [
            Pool::<V, Totals>::prepared_bytes(self.packet_nodes)?,
            Pool::<V, ()>::prepared_bytes(self.scalar_nodes)?,
            Pool::<V, HoldSummary>::prepared_bytes(self.hold_nodes)?,
            Pool::<V, SostenutoIndex<V>>::prepared_bytes(self.sostenuto_nodes)?,
        ] {
            bytes = bytes.checked_add(more).ok_or(Error::Capacity)?;
        }
        if bytes > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        Ok(bytes)
    }
}
/// A copied token borrows a version. Only `retain` acquires another lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Version {
    scope: u32,
    slot: NonZeroU32,
    generation: u64,
}
#[derive(Clone, Copy)]
struct Table {
    references: u32,
    packets: Root,
    pages: [Option<NonZeroU32>; LEAVES],
}
#[derive(Clone, Copy)]
struct TableSlot {
    generation: u64,
    table: Option<Table>,
}
#[derive(Clone, Copy)]
struct CategoryPage {
    references: u32,
    base: u16,
    roots: [Root; LEAF],
}
#[derive(Clone, Copy)]
enum Kind {
    Packet,
    Scalar,
    Hold,
    Sostenuto,
}
impl Kind {
    fn index(self) -> usize {
        match self {
            Self::Packet => 0,
            Self::Scalar => 1,
            Self::Hold => 2,
            Self::Sostenuto => 3,
        }
    }
    fn category(slot: usize) -> Self {
        match slot {
            HOLD..SOSTENUTO => Self::Hold,
            SOSTENUTO..8320 => Self::Sostenuto,
            _ => Self::Scalar,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Work {
    pub(crate) table_copies: usize,
    pub(crate) category_copies: usize,
    pub(crate) root_retains: usize,
    pub(crate) retired_tables: usize,
    pub(crate) retired_categories: usize,
    pub(crate) counter_overflow: bool,
}
impl Work {
    fn add(
        &mut self,
        tables: usize,
        categories: usize,
        roots: usize,
        retired_tables: usize,
        retired_categories: usize,
    ) {
        for (counter, amount) in [
            (&mut self.table_copies, tables),
            (&mut self.category_copies, categories),
            (&mut self.root_retains, roots),
            (&mut self.retired_tables, retired_tables),
            (&mut self.retired_categories, retired_categories),
        ] {
            match counter.checked_add(amount) {
                Some(value) => *counter = value,
                None => {
                    *counter = usize::MAX;
                    self.counter_overflow = true;
                }
            }
        }
    }
}
pub(crate) struct Store<V: Copy> {
    scope: u32,
    config: Config,
    leases: usize,
    tables: Vec<TableSlot>,
    free_tables: Vec<NonZeroU32>,
    retired_tables: Vec<NonZeroU32>,
    pages: Vec<Option<CategoryPage>>,
    free_pages: Vec<NonZeroU32>,
    retired_pages: Vec<NonZeroU32>,
    packets: Pool<V, Totals>,
    scalars: Pool<V, ()>,
    holds: Pool<V, HoldSummary>,
    sostenuto: Pool<V, SostenutoIndex<V>>,
    cursor: usize,
    work: Cell<Work>,
}
fn slot(id: NonZeroU32) -> usize {
    (id.get() - 1) as usize
}
fn free_ids(count: usize) -> Vec<NonZeroU32> {
    (1..=count as u32)
        .rev()
        .map(|id| NonZeroU32::new(id).unwrap())
        .collect()
}
fn retirement_queue(count: usize) -> Vec<NonZeroU32> {
    let mut queue = vec![NonZeroU32::new(1).unwrap(); count];
    for value in &mut queue {
        *core::hint::black_box(value) = NonZeroU32::new(1).unwrap();
    }
    queue.clear();
    queue
}
impl<V: Copy> Store<V> {
    pub(crate) fn prepare(config: Config) -> Result<Self, Error> {
        config.prepared_bytes::<V>()?;
        let scope = NEXT_SCOPE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |scope| {
                scope.checked_add(1)
            })
            .map_err(|_| Error::Exhausted)?;
        let mut tables = vec![
            TableSlot {
                generation: 1,
                table: None
            };
            config.versions
        ];
        let mut pages = vec![None; config.category_pages];
        for table in &mut tables {
            *core::hint::black_box(table) = TableSlot {
                generation: 1,
                table: None,
            };
        }
        for page in &mut pages {
            *core::hint::black_box(page) = None;
        }
        Ok(Self {
            scope,
            config,
            leases: 0,
            tables,
            pages,
            free_tables: free_ids(config.versions),
            free_pages: free_ids(config.category_pages),
            retired_tables: retirement_queue(config.versions),
            retired_pages: retirement_queue(config.category_pages),
            packets: Pool::prepare(config.packet_nodes)?,
            scalars: Pool::prepare(config.scalar_nodes)?,
            holds: Pool::prepare(config.hold_nodes)?,
            sostenuto: Pool::prepare(config.sostenuto_nodes)?,
            cursor: 0,
            work: Cell::new(Work::default()),
        })
    }
    fn table(&self, version: Version) -> Result<&Table, Error> {
        let entry = self
            .tables
            .get(slot(version.slot))
            .ok_or(Error::InvalidId)?;
        if version.scope != self.scope || version.generation != entry.generation {
            return Err(Error::InvalidId);
        }
        entry
            .table
            .as_ref()
            .filter(|table| table.references > 0)
            .ok_or(Error::InvalidId)
    }
    fn version(&self, id: NonZeroU32) -> Version {
        Version {
            scope: self.scope,
            slot: id,
            generation: self.tables[slot(id)].generation,
        }
    }
    pub(crate) fn empty(&mut self) -> Result<Version, Error> {
        if self.leases == self.config.leases {
            return Err(Error::Capacity);
        }
        let id = self.free_tables.pop().ok_or(Error::Capacity)?;
        self.tables[slot(id)].table = Some(Table {
            references: 1,
            packets: self.packets.empty(),
            pages: [None; LEAVES],
        });
        self.leases += 1;
        Ok(self.version(id))
    }
    pub(crate) fn retain(&mut self, version: Version) -> Result<Version, Error> {
        self.table(version)?;
        if self.leases == self.config.leases {
            return Err(Error::Capacity);
        }
        self.tables[slot(version.slot)]
            .table
            .as_mut()
            .unwrap()
            .references += 1;
        self.leases += 1;
        Ok(version)
    }
    pub(crate) fn release(&mut self, version: Version) -> Result<(), Error> {
        self.table(version)?;
        let table = self.tables[slot(version.slot)].table.as_mut().unwrap();
        table.references -= 1;
        self.leases -= 1;
        if table.references == 0 {
            self.retired_tables.push(version.slot);
        }
        Ok(())
    }
    fn empty_root(&self, kind: Kind) -> Root {
        match kind {
            Kind::Packet => self.packets.empty(),
            Kind::Scalar => self.scalars.empty(),
            Kind::Hold => self.holds.empty(),
            Kind::Sostenuto => self.sostenuto.empty(),
        }
    }
    fn category_root(&self, table: &Table, key: usize) -> Root {
        table.pages[key / LEAF].map_or_else(
            || self.empty_root(Kind::category(key)),
            |page| self.pages[slot(page)].unwrap().roots[key % LEAF],
        )
    }
    fn root_len(&self, kind: Kind, root: Root) -> usize {
        match kind {
            Kind::Packet => self.packets.len(root),
            Kind::Scalar => self.scalars.len(root),
            Kind::Hold => self.holds.len(root),
            Kind::Sostenuto => self.sostenuto.len(root),
        }
        .expect("owned category root")
    }
    fn retain_root(&mut self, kind: Kind, root: Root) {
        match kind {
            Kind::Packet => self.packets.retain(root),
            Kind::Scalar => self.scalars.retain(root),
            Kind::Hold => self.holds.retain(root),
            Kind::Sostenuto => self.sostenuto.retain(root),
        }
        .expect("preflighted root credit");
        let mut work = self.work.get();
        work.add(0, 0, 1, 0, 0);
        self.work.set(work);
    }
    fn release_root(&mut self, kind: Kind, root: Root) {
        match kind {
            Kind::Packet => self.packets.release(root),
            Kind::Scalar => self.scalars.release(root),
            Kind::Hold => self.holds.release(root),
            Kind::Sostenuto => self.sostenuto.release(root),
        }
        .expect("owned category root");
    }
    fn preflight(
        &self,
        version: Version,
        keys: [Option<Key>; 3],
        point: Point,
        inserting: bool,
        values: &impl Lookup<V>,
    ) -> Result<(), Error> {
        let table = self.table(version)?;
        let shared = table.references > 1;
        if shared && self.free_tables.is_empty() {
            return Err(Error::Capacity);
        }
        let mut touched = [usize::MAX; 3];
        let mut pages = 0;
        let mut roots = [0; 4];
        let mut nodes = [0; 4];
        if shared && self.root_len(Kind::Packet, table.packets) > 0 {
            roots[0] += 1;
        }
        for key in keys.into_iter().flatten() {
            let key = key.slot();
            let page = key / LEAF;
            if !touched.contains(&page) {
                touched[pages] = page;
                pages += 1;
                let old = table.pages[page].map(|id| self.pages[slot(id)].unwrap());
                if old.is_none_or(|old| shared || old.references > 1) {
                    if let Some(old) = old {
                        for (offset, root) in old.roots.into_iter().enumerate() {
                            let kind = Kind::category(page * LEAF + offset);
                            roots[kind.index()] += usize::from(self.root_len(kind, root) > 0);
                        }
                    }
                } else {
                    pages -= 1;
                    touched[pages] = usize::MAX;
                }
            }
        }
        if pages > self.free_pages.len() {
            return Err(Error::Capacity);
        }
        for (kind, root, force_shared) in core::iter::once((Kind::Packet, table.packets, shared))
            .chain(keys.into_iter().flatten().map(|key| {
                let key = key.slot();
                let force_shared = shared
                    || table.pages[key / LEAF]
                        .is_some_and(|id| self.pages[slot(id)].unwrap().references > 1);
                (
                    Kind::category(key),
                    self.category_root(table, key),
                    force_shared,
                )
            }))
        {
            let i = kind.index();
            if inserting {
                nodes[i] += match kind {
                    Kind::Packet => self
                        .packets
                        .insert_credit(root, point, values, force_shared),
                    Kind::Scalar => self
                        .scalars
                        .insert_credit(root, point, values, force_shared),
                    Kind::Hold => self.holds.insert_credit(root, point, values, force_shared),
                    Kind::Sostenuto => {
                        self.sostenuto
                            .insert_credit(root, point, values, force_shared)
                    }
                }?;
                roots[i] += usize::from(self.root_len(kind, root) == 0);
            } else {
                nodes[i] += match kind {
                    Kind::Packet => self.packets.remove_credit(root),
                    Kind::Scalar => self.scalars.remove_credit(root),
                    Kind::Hold => self.holds.remove_credit(root),
                    Kind::Sostenuto => self.sostenuto.remove_credit(root),
                }?;
            }
        }
        let available_nodes = [
            self.packets.free_nodes(),
            self.scalars.free_nodes(),
            self.holds.free_nodes(),
            self.sostenuto.free_nodes(),
        ];
        let available_roots = [
            self.packets.available_roots(),
            self.scalars.available_roots(),
            self.holds.available_roots(),
            self.sostenuto.available_roots(),
        ];
        if (0..4).any(|i| nodes[i] > available_nodes[i] || roots[i] > available_roots[i]) {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    fn unique_table(&mut self, version: Version) -> Version {
        let mut table = *self.table(version).unwrap();
        if table.references == 1 {
            return version;
        }
        self.tables[slot(version.slot)]
            .table
            .as_mut()
            .unwrap()
            .references -= 1;
        self.retain_root(Kind::Packet, table.packets);
        for page in table.pages.into_iter().flatten() {
            self.pages[slot(page)].as_mut().unwrap().references += 1;
        }
        table.references = 1;
        let id = self.free_tables.pop().expect("preflighted version credit");
        self.tables[slot(id)].table = Some(table);
        let mut work = self.work.get();
        work.add(1, 0, 0, 0, 0);
        self.work.set(work);
        self.version(id)
    }
    fn unique_page(&mut self, version: Version, page: usize) -> NonZeroU32 {
        let old = self.table(version).unwrap().pages[page];
        if let Some(id) = old {
            if self.pages[slot(id)].unwrap().references == 1 {
                return id;
            }
        }
        let roots = if let Some(id) = old {
            let roots = self.pages[slot(id)].unwrap().roots;
            for (offset, root) in roots.into_iter().enumerate() {
                self.retain_root(Kind::category(page * LEAF + offset), root);
            }
            self.pages[slot(id)].as_mut().unwrap().references -= 1;
            let mut work = self.work.get();
            work.add(0, 1, 0, 0, 0);
            self.work.set(work);
            roots
        } else {
            core::array::from_fn(|offset| self.empty_root(Kind::category(page * LEAF + offset)))
        };
        let id = self.free_pages.pop().expect("preflighted category credit");
        self.pages[slot(id)] = Some(CategoryPage {
            references: 1,
            base: (page * LEAF) as u16,
            roots,
        });
        self.tables[slot(version.slot)]
            .table
            .as_mut()
            .unwrap()
            .pages[page] = Some(id);
        id
    }
    fn edit_root(
        &mut self,
        kind: Kind,
        root: Root,
        value: V,
        inserting: bool,
        values: &mut impl Values<V>,
    ) -> Root {
        let point = values.point(value);
        match (kind, inserting) {
            (Kind::Packet, true) => self.packets.insert(root, value, values),
            (Kind::Scalar, true) => self.scalars.insert(root, value, values),
            (Kind::Hold, true) => self.holds.insert(root, value, values),
            (Kind::Sostenuto, true) => self.sostenuto.insert(root, value, values),
            (Kind::Packet, false) => self.packets.remove(root, point, values),
            (Kind::Scalar, false) => self.scalars.remove(root, point, values),
            (Kind::Hold, false) => self.holds.remove(root, point, values),
            (Kind::Sostenuto, false) => self.sostenuto.remove(root, point, values),
        }
        .expect("preflighted complete index mutation")
    }
    fn edit(
        &mut self,
        version: Version,
        value: V,
        inserting: bool,
        values: &mut impl Values<V>,
    ) -> Result<Version, Error> {
        let keys = values.event(value).map_or([None; 3], Event::keys);
        self.preflight(version, keys, values.point(value), inserting, values)?;
        let version = self.unique_table(version);
        let root = self.table(version).unwrap().packets;
        let root = self.edit_root(Kind::Packet, root, value, inserting, values);
        self.tables[slot(version.slot)]
            .table
            .as_mut()
            .unwrap()
            .packets = root;
        for key in keys.into_iter().flatten() {
            let key = key.slot();
            let page = self.unique_page(version, key / LEAF);
            let root = self.pages[slot(page)].unwrap().roots[key % LEAF];
            let root = self.edit_root(Kind::category(key), root, value, inserting, values);
            self.pages[slot(page)].as_mut().unwrap().roots[key % LEAF] = root;
        }
        Ok(version)
    }
    /// Values must keep their point, packet classification and payload immutable
    /// while any tree owns them. A refused edit consumes no version lease.
    pub(crate) fn insert(
        &mut self,
        version: Version,
        value: V,
        values: &mut impl Values<V>,
    ) -> Result<Version, Error> {
        let point = values.point(value);
        if self
            .last(version, Cut::at(point), values)?
            .is_some_and(|old| values.point(old) == point)
        {
            return Err(Error::InvalidEvent);
        }
        self.edit(version, value, true, values)
    }
    pub(crate) fn remove(
        &mut self,
        version: Version,
        point: Point,
        values: &mut impl Values<V>,
    ) -> Result<Version, Error> {
        match self.last(version, Cut::at(point), values)? {
            Some(value) if values.point(value) == point => self.edit(version, value, false, values),
            _ => Ok(version),
        }
    }
    fn preflight_records(
        &self,
        version: Version,
        removals: &[V],
        insertions: &[V],
        values: &impl Lookup<V>,
    ) -> Result<(), Error>
    where
        V: Eq,
    {
        let table = self.table(version)?;
        for records in [removals, insertions] {
            if records
                .windows(2)
                .any(|pair| values.point(pair[0]) >= values.point(pair[1]))
            {
                return Err(Error::InvalidEvent);
            }
        }
        for &value in removals {
            if self.last(version, Cut::at(values.point(value)), values)? != Some(value) {
                return Err(Error::InvalidEvent);
            }
        }
        for &value in insertions {
            let point = values.point(value);
            if self
                .last(version, Cut::at(point), values)?
                .is_some_and(|old| values.point(old) == point)
                && removals
                    .binary_search_by_key(&point, |old| values.point(*old))
                    .is_err()
            {
                return Err(Error::InvalidEvent);
            }
        }
        let shared = table.references > 1;
        if shared && self.free_tables.is_empty() {
            return Err(Error::Capacity);
        }
        let upper_count = self
            .len(version)?
            .checked_add(insertions.len())
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Capacity)?;
        let height = 2 * upper_count.ilog2() as usize + 2;
        let mut nodes = [0usize; 4];
        let mut roots = [0usize; 4];
        let mut affected = [false; LEAVES];
        for (records, inserting) in [(removals, false), (insertions, true)] {
            for &value in records {
                let credit = if inserting {
                    height + 1
                } else {
                    4 * (height + 1) + 8
                };
                nodes[0] = nodes[0].checked_add(credit).ok_or(Error::Capacity)?;
                roots[0] += usize::from(inserting);
                for key in values
                    .event(value)
                    .map_or([None; 3], Event::keys)
                    .into_iter()
                    .flatten()
                {
                    let key = key.slot();
                    let kind = Kind::category(key).index();
                    affected[key / LEAF] = true;
                    nodes[kind] = nodes[kind].checked_add(credit).ok_or(Error::Capacity)?;
                    roots[kind] += usize::from(inserting);
                }
            }
        }
        roots[0] += usize::from(shared && self.root_len(Kind::Packet, table.packets) > 0);
        let mut pages = 0;
        for (page, affected) in affected.into_iter().enumerate() {
            if !affected {
                continue;
            }
            let old = table.pages[page].map(|id| self.pages[slot(id)].unwrap());
            if old.is_none_or(|old| shared || old.references > 1) {
                pages += 1;
                if let Some(old) = old {
                    for (offset, root) in old.roots.into_iter().enumerate() {
                        let kind = Kind::category(page * LEAF + offset);
                        roots[kind.index()] += usize::from(self.root_len(kind, root) > 0);
                    }
                }
            }
        }
        let available_nodes = [
            self.packets.free_nodes(),
            self.scalars.free_nodes(),
            self.holds.free_nodes(),
            self.sostenuto.free_nodes(),
        ];
        let available_roots = [
            self.packets.available_roots(),
            self.scalars.available_roots(),
            self.holds.available_roots(),
            self.sostenuto.available_roots(),
        ];
        if pages > self.free_pages.len()
            || (0..4).any(|i| nodes[i] > available_nodes[i] || roots[i] > available_roots[i])
        {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    /// Both lists are strictly ordered. Preflight reserves the worst path work for
    /// the whole edit; no intermediate version escapes and refusal preserves it.
    pub(crate) fn replace_records(
        &mut self,
        mut version: Version,
        removals: &[V],
        insertions: &[V],
        values: &mut impl Values<V>,
    ) -> Result<Version, Error>
    where
        V: Eq,
    {
        self.preflight_records(version, removals, insertions, values)?;
        for (records, inserting) in [(removals, false), (insertions, true)] {
            for &value in records {
                version = self
                    .edit(version, value, inserting, values)
                    .expect("preflighted whole record batch");
            }
        }
        Ok(version)
    }
    pub(crate) fn totals(
        &self,
        version: Version,
        after: Cut,
        cut: Cut,
        values: &impl Lookup<V>,
    ) -> Result<Totals, Error> {
        let totals = self
            .packets
            .interval(self.table(version)?.packets, after, cut, values)?;
        if totals.overflow {
            return Err(Error::Capacity);
        }
        Ok(totals)
    }
    pub(crate) fn last(
        &self,
        version: Version,
        cut: Cut,
        values: &impl Lookup<V>,
    ) -> Result<Option<V>, Error> {
        self.packets.last(self.table(version)?.packets, cut, values)
    }
    pub(crate) fn first(
        &self,
        version: Version,
        after: Cut,
        cut: Cut,
        values: &impl Lookup<V>,
    ) -> Result<Option<V>, Error> {
        self.packets
            .first(self.table(version)?.packets, after, cut, values)
    }
    pub(crate) fn len(&self, version: Version) -> Result<usize, Error> {
        self.packets.len(self.table(version)?.packets)
    }
    pub(crate) fn leases(&self) -> usize {
        self.leases
    }
    pub(crate) fn timeline<'a, L: Lookup<V>>(
        &'a self,
        version: Version,
        values: &'a L,
    ) -> Result<Reader<'a, V, L>, Error> {
        Ok(Reader {
            store: self,
            table: self.table(version)?,
            values,
        })
    }
    pub(crate) fn reset_work(&self) -> Work {
        self.packets.reset_work();
        self.scalars.reset_work();
        self.holds.reset_work();
        self.sostenuto.reset_work();
        self.work.replace(Work::default())
    }
    pub(crate) fn work(&self) -> (Work, [super::tree::Work; 4]) {
        (
            self.work.get(),
            [
                self.packets.work(),
                self.scalars.work(),
                self.holds.work(),
                self.sostenuto.work(),
            ],
        )
    }
    pub(crate) fn maintain(&mut self, budget: usize, values: &mut impl Values<V>) -> usize {
        let mut reclaimed = 0;
        while reclaimed < budget {
            let mut progress = false;
            for _ in 0..6 {
                let next = self.cursor;
                self.cursor = (self.cursor + 1) % 6;
                match next {
                    0 => {
                        if let Some(id) = self.retired_tables.pop() {
                            let table = self.tables[slot(id)].table.take().unwrap();
                            self.release_root(Kind::Packet, table.packets);
                            for page in table.pages.into_iter().flatten() {
                                let entry = self.pages[slot(page)].as_mut().unwrap();
                                entry.references -= 1;
                                if entry.references == 0 {
                                    self.retired_pages.push(page);
                                }
                            }
                            if let Some(generation) =
                                self.tables[slot(id)].generation.checked_add(1)
                            {
                                self.tables[slot(id)].generation = generation;
                                self.free_tables.push(id);
                            }
                            let mut work = self.work.get();
                            work.add(0, 0, 0, 1, 0);
                            self.work.set(work);
                            progress = true;
                        }
                    }
                    1 => {
                        if let Some(id) = self.retired_pages.pop() {
                            let page = self.pages[slot(id)].take().unwrap();
                            for (offset, root) in page.roots.into_iter().enumerate() {
                                self.release_root(
                                    Kind::category(usize::from(page.base) + offset),
                                    root,
                                );
                            }
                            self.free_pages.push(id);
                            let mut work = self.work.get();
                            work.add(0, 0, 0, 0, 1);
                            self.work.set(work);
                            progress = true;
                        }
                    }
                    2 => progress = self.packets.maintain(1, values) > 0,
                    3 => progress = self.scalars.maintain(1, values) > 0,
                    4 => progress = self.holds.maintain(1, values) > 0,
                    _ => progress = self.sostenuto.maintain(1, values) > 0,
                }
                if progress {
                    break;
                }
            }
            if !progress {
                break;
            }
            reclaimed += 1;
        }
        reclaimed
    }
}

pub(crate) struct Reader<'a, V: Copy, L> {
    store: &'a Store<V>,
    table: &'a Table,
    values: &'a L,
}
impl<V: Copy, L: Lookup<V>> Timeline for Reader<'_, V, L> {
    fn last(&self, key: Key, cut: Cut) -> Option<Event> {
        let root = self.store.category_root(self.table, key.slot());
        let value = match Kind::category(key.slot()) {
            Kind::Hold => self.store.holds.last(root, cut, self.values),
            Kind::Sostenuto => self.store.sostenuto.last(root, cut, self.values),
            _ => self.store.scalars.last(root, cut, self.values),
        }
        .expect("owned query root")?;
        self.values.event(value)
    }
    fn first(&self, key: Key, after: Cut, cut: Cut) -> Option<Event> {
        let root = self.store.category_root(self.table, key.slot());
        let value = match Kind::category(key.slot()) {
            Kind::Hold => self.store.holds.first(root, after, cut, self.values),
            Kind::Sostenuto => self.store.sostenuto.first(root, after, cut, self.values),
            _ => self.store.scalars.first(root, after, cut, self.values),
        }
        .expect("owned query root")?;
        self.values.event(value)
    }
    fn holds(&self, ch: u8, after: Cut, cut: Cut) -> HoldSummary {
        self.store
            .holds
            .interval(
                self.store.category_root(self.table, Key::Hold(ch).slot()),
                after,
                cut,
                self.values,
            )
            .expect("owned hold root")
    }
    fn sostenuto(&self, ch: u8, after: Cut, cut: Cut) -> SostenutoSummary {
        self.store
            .sostenuto
            .interval(
                self.store
                    .category_root(self.table, Key::Sostenuto(ch).slot()),
                after,
                cut,
                self.values,
            )
            .expect("owned sostenuto root")
            .0
            .map(|value| self.values.point(value))
    }
}

#[cfg(test)]
mod tests;
