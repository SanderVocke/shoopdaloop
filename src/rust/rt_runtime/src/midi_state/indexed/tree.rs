//! Prepared persistent ordered trees with bounded path edits and deferred release.

use super::{Cut, Event, HoldSummary, Point, SostenutoSummary};
use crate::Error;
use alloc::{vec, vec::Vec};
use core::{
    cell::Cell,
    num::NonZeroU32,
    sync::atomic::{AtomicU32, Ordering},
};

static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);
type Link = Option<NonZeroU32>;

pub(crate) fn maximum_height(nodes: usize) -> usize {
    let (mut previous, mut minimum, mut height) = (0usize, 1usize, 0usize);
    while minimum <= nodes {
        let Some(next) = minimum.checked_add(previous).and_then(|n| n.checked_add(1)) else {
            return height + 1;
        };
        previous = minimum;
        minimum = next;
        height += 1;
    }
    height
}

pub(crate) trait Lookup<V> {
    fn point(&self, value: V) -> Point;
    fn event(&self, value: V) -> Option<Event>;
    fn packet_bytes(&self, _: V) -> usize {
        0
    }
    fn packet_pages(&self, _: V) -> (usize, Option<NonZeroU32>, Option<NonZeroU32>) {
        (0, None, None)
    }
}
impl Lookup<Event> for () {
    fn point(&self, value: Event) -> Point {
        value.point
    }
    fn event(&self, value: Event) -> Option<Event> {
        Some(value)
    }
}
pub(crate) trait Values<V>: Lookup<V> {
    fn retain(&mut self, value: V);
    fn release(&mut self, value: V);
}
impl Values<Event> for () {
    fn retain(&mut self, _: Event) {}
    fn release(&mut self, _: Event) {}
}
pub(crate) trait Summary<V>: Copy + Default {
    fn value(value: V, lookup: &impl Lookup<V>) -> Self;
    fn then(self, later: Self) -> Self;
}
impl<V> Summary<V> for () {
    fn value(_: V, _: &impl Lookup<V>) -> Self {}
    fn then(self, _: Self) -> Self {}
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Totals {
    pub(crate) events: usize,
    pub(crate) bytes: usize,
    pub(crate) pages: usize,
    first_page: Option<NonZeroU32>,
    last_page: Option<NonZeroU32>,
    pub(crate) overflow: bool,
}
impl<V: Copy> Summary<V> for Totals {
    fn value(value: V, lookup: &impl Lookup<V>) -> Self {
        let bytes = lookup.packet_bytes(value).checked_add(12);
        let (pages, first_page, last_page) = lookup.packet_pages(value);
        Self {
            pages,
            first_page,
            last_page,
            events: 1,
            bytes: bytes.unwrap_or(usize::MAX),
            overflow: bytes.is_none(),
        }
    }
    fn then(self, later: Self) -> Self {
        let events = self.events.checked_add(later.events);
        let bytes = self.bytes.checked_add(later.bytes);
        let shared = usize::from(self.last_page.is_some() && self.last_page == later.first_page);
        let pages = self
            .pages
            .checked_add(later.pages)
            .and_then(|n| n.checked_sub(shared));
        Self {
            pages: pages.unwrap_or(usize::MAX),
            first_page: self.first_page.or(later.first_page),
            last_page: later.last_page.or(self.last_page),
            events: events.unwrap_or(usize::MAX),
            bytes: bytes.unwrap_or(usize::MAX),
            overflow: self.overflow
                || later.overflow
                || events.is_none()
                || bytes.is_none()
                || pages.is_none(),
        }
    }
}
impl<V> Summary<V> for HoldSummary {
    fn value(value: V, lookup: &impl Lookup<V>) -> Self {
        lookup
            .event(value)
            .and_then(Event::hold)
            .map_or_else(Self::default, Self::update)
    }
    fn then(self, later: Self) -> Self {
        self.then(later)
    }
}
impl Summary<Event> for SostenutoSummary {
    fn value(value: Event, _: &impl Lookup<Event>) -> Self {
        value
            .sostenuto()
            .map_or_else(Self::default, |update| Self::update(update, value.point))
    }
    fn then(self, later: Self) -> Self {
        self.then(later)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SostenutoIndex<V: Copy>(pub(crate) SostenutoSummary<V>);
impl<V: Copy> Default for SostenutoIndex<V> {
    fn default() -> Self {
        Self(SostenutoSummary::default())
    }
}
impl<V: Copy> Summary<V> for SostenutoIndex<V> {
    fn value(value: V, lookup: &impl Lookup<V>) -> Self {
        Self(
            lookup
                .event(value)
                .and_then(Event::sostenuto)
                .map_or_else(SostenutoSummary::default, |update| {
                    SostenutoSummary::update(update, value)
                }),
        )
    }
    fn then(self, later: Self) -> Self {
        Self(self.0.then(later.0))
    }
}

/// Copying borrows this token; `retain` acquires an additional root reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Root {
    scope: u32,
    link: Link,
    generation: u64,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Work {
    pub(crate) visited: usize,
    pub(crate) copied: usize,
    pub(crate) rotations: usize,
    pub(crate) reclaimed: usize,
    pub(crate) counter_overflow: bool,
}
#[derive(Clone, Copy)]
struct Node<V, S> {
    value: V,
    children: [Link; 2],
    summary: S,
    count: usize,
    references: u32,
    height: u8,
}
#[derive(Clone, Copy)]
struct Slot<V, S> {
    generation: u64,
    node: Option<Node<V, S>>,
}
pub(crate) struct Pool<V, S> {
    scope: u32,
    nodes: Vec<Slot<V, S>>,
    free: Vec<NonZeroU32>,
    retired: Vec<NonZeroU32>,
    roots: usize,
    work: Cell<Work>,
}
fn slot(id: NonZeroU32) -> usize {
    (id.get() - 1) as usize
}

impl<V: Copy, S: Summary<V>> Pool<V, S> {
    pub(crate) fn prepare(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 {
            return Err(Error::InvalidConfig);
        }
        Self::prepared_bytes(capacity)?;
        let scope = NEXT_SCOPE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| Error::Exhausted)?;
        let mut nodes = vec![
            Slot {
                generation: 1,
                node: None
            };
            capacity
        ];
        for node in &mut nodes {
            *core::hint::black_box(node) = Slot {
                generation: 1,
                node: None,
            };
        }
        let mut retired = vec![NonZeroU32::new(1).unwrap(); capacity];
        for entry in &mut retired {
            *core::hint::black_box(entry) = NonZeroU32::new(1).unwrap();
        }
        retired.clear();
        Ok(Self {
            scope,
            nodes,
            free: (1..=capacity as u32)
                .rev()
                .map(|value| NonZeroU32::new(value).unwrap())
                .collect(),
            retired,
            roots: 0,
            work: Cell::new(Work::default()),
        })
    }
    pub(crate) fn empty(&self) -> Root {
        Root {
            scope: self.scope,
            link: None,
            generation: 0,
        }
    }
    pub(crate) fn has_retired(&self) -> bool {
        !self.retired.is_empty()
    }
    pub(crate) fn free_nodes(&self) -> usize {
        self.free.len()
    }
    pub(crate) fn prepared_bytes(capacity: usize) -> Result<usize, Error> {
        capacity
            .checked_mul(size_of::<Slot<V, S>>() + 2 * size_of::<NonZeroU32>())
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
    pub(crate) fn available_roots(&self) -> usize {
        self.nodes.len() - self.roots
    }
    pub(crate) fn root_references(&self) -> usize {
        self.roots
    }
    pub(crate) fn work(&self) -> Work {
        self.work.get()
    }
    pub(crate) fn reset_work(&self) -> Work {
        self.work.replace(Work::default())
    }
    fn add_work(&self, visited: usize, copied: usize, rotations: usize, reclaimed: usize) {
        let mut work = self.work.get();
        for (counter, amount) in [
            (&mut work.visited, visited),
            (&mut work.copied, copied),
            (&mut work.rotations, rotations),
            (&mut work.reclaimed, reclaimed),
        ] {
            match counter.checked_add(amount) {
                Some(value) => *counter = value,
                None => {
                    *counter = usize::MAX;
                    work.counter_overflow = true;
                }
            }
        }
        self.work.set(work);
    }
    fn check(&self, root: Root) -> Result<Link, Error> {
        if root.scope != self.scope {
            return Err(Error::InvalidId);
        }
        if let Some(id) = root.link {
            let entry = self.nodes.get(slot(id)).ok_or(Error::InvalidId)?;
            if entry.generation != root.generation
                || entry.node.is_none_or(|node| node.references == 0)
            {
                return Err(Error::InvalidId);
            }
        }
        Ok(root.link)
    }
    fn root(&self, link: Link) -> Root {
        Root {
            scope: self.scope,
            generation: link.map_or(0, |id| self.nodes[slot(id)].generation),
            link,
        }
    }
    fn node(&self, id: NonZeroU32) -> &Node<V, S> {
        self.add_work(1, 0, 0, 0);
        self.nodes[slot(id)].node.as_ref().expect("owned tree node")
    }
    fn retain_link(&mut self, link: Link) {
        if let Some(id) = link {
            let node = self.nodes[slot(id)].node.as_mut().expect("owned child");
            node.references = node
                .references
                .checked_add(1)
                .expect("prepared reference ceiling");
        }
    }
    pub(crate) fn retain(&mut self, root: Root) -> Result<Root, Error> {
        let link = self.check(root)?;
        if link.is_some() {
            if self.roots == self.nodes.len() {
                return Err(Error::Capacity);
            }
            self.retain_link(link);
            self.roots += 1;
        }
        Ok(root)
    }
    fn release_link(&mut self, link: Link) {
        if let Some(id) = link {
            let node = self.nodes[slot(id)].node.as_mut().expect("owned child");
            node.references -= 1;
            if node.references == 0 {
                self.retired.push(id);
            }
        }
    }
    pub(crate) fn release(&mut self, root: Root) -> Result<(), Error> {
        let link = self.check(root)?;
        if link.is_some() {
            self.roots -= 1;
            self.release_link(link);
        }
        Ok(())
    }
    fn free_slot(&mut self, id: NonZeroU32) {
        let entry = &mut self.nodes[slot(id)];
        entry.node = None;
        match entry.generation.checked_add(1) {
            Some(generation) => {
                entry.generation = generation;
                self.free.push(id);
            }
            None => (),
        }
        self.add_work(0, 0, 0, 1);
    }
    pub(crate) fn maintain(&mut self, budget: usize, values: &mut impl Values<V>) -> usize {
        let mut reclaimed = 0;
        while reclaimed < budget {
            let Some(id) = self.retired.pop() else {
                break;
            };
            let node = self.nodes[slot(id)].node.expect("retired node");
            for child in node.children {
                self.release_link(child);
            }
            values.release(node.value);
            self.free_slot(id);
            reclaimed += 1;
        }
        reclaimed
    }
    fn allocate(&mut self, node: Node<V, S>, values: &mut impl Values<V>) -> NonZeroU32 {
        let id = self.free.pop().expect("preflighted node credit");
        values.retain(node.value);
        self.nodes[slot(id)].node = Some(node);
        id
    }
    fn unique(&mut self, id: NonZeroU32, values: &mut impl Values<V>) -> NonZeroU32 {
        let mut node = *self.node(id);
        if node.references == 1 {
            return id;
        }
        self.nodes[slot(id)]
            .node
            .as_mut()
            .expect("owned path")
            .references -= 1;
        for child in node.children {
            self.retain_link(child);
        }
        node.references = 1;
        self.add_work(0, 1, 0, 0);
        self.allocate(node, values)
    }
    fn height(&self, link: Link) -> u8 {
        link.map_or(0, |id| self.node(id).height)
    }
    fn summary(&self, link: Link) -> S {
        link.map_or_else(S::default, |id| self.node(id).summary)
    }
    fn count(&self, link: Link) -> usize {
        link.map_or(0, |id| self.node(id).count)
    }
    fn recalculate(&mut self, id: NonZeroU32, lookup: &impl Lookup<V>) {
        let node = *self.node(id);
        let summary = self
            .summary(node.children[0])
            .then(S::value(node.value, lookup))
            .then(self.summary(node.children[1]));
        let count = 1 + self.count(node.children[0]) + self.count(node.children[1]);
        let height = 1 + self
            .height(node.children[0])
            .max(self.height(node.children[1]));
        let node = self.nodes[slot(id)].node.as_mut().expect("owned node");
        node.summary = summary;
        node.count = count;
        node.height = height;
    }
    fn balance(&self, id: NonZeroU32) -> i16 {
        let children = self.node(id).children;
        i16::from(self.height(children[0])) - i16::from(self.height(children[1]))
    }
    fn rotate(&mut self, id: NonZeroU32, side: usize, values: &mut impl Values<V>) -> NonZeroU32 {
        let child = self.unique(
            self.node(id).children[side].expect("rotation child"),
            values,
        );
        let middle = self.node(child).children[1 - side];
        self.nodes[slot(id)]
            .node
            .as_mut()
            .expect("rotation root")
            .children[side] = middle;
        self.nodes[slot(child)]
            .node
            .as_mut()
            .expect("rotation child")
            .children[1 - side] = Some(id);
        self.recalculate(id, values);
        self.recalculate(child, values);
        self.add_work(0, 0, 1, 0);
        child
    }
    fn rebalance(&mut self, id: NonZeroU32, values: &mut impl Values<V>) -> NonZeroU32 {
        self.recalculate(id, values);
        let balance = self.balance(id);
        if balance.abs() <= 1 {
            return id;
        }
        let side = usize::from(balance < 0);
        let child = self.node(id).children[side].expect("heavy child");
        if (side == 0 && self.balance(child) < 0) || (side == 1 && self.balance(child) > 0) {
            let child = self.unique(child, values);
            let child = self.rotate(child, 1 - side, values);
            self.nodes[slot(id)]
                .node
                .as_mut()
                .expect("rotation root")
                .children[side] = Some(child);
        }
        self.rotate(id, side, values)
    }
    fn insertion_credit(
        &self,
        mut link: Link,
        point: Point,
        lookup: &impl Lookup<V>,
        mut copying: bool,
    ) -> usize {
        let mut required = 0;
        while let Some(id) = link {
            let node = self.node(id);
            copying |= node.references > 1;
            required += usize::from(copying);
            if point == lookup.point(node.value) {
                return required;
            }
            link = node.children[usize::from(point > lookup.point(node.value))];
        }
        required + 1
    }
    pub(crate) fn insert_credit(
        &self,
        root: Root,
        point: Point,
        lookup: &impl Lookup<V>,
        shared: bool,
    ) -> Result<usize, Error> {
        Ok(self.insertion_credit(self.check(root)?, point, lookup, shared))
    }
    pub(crate) fn remove_credit(&self, root: Root) -> Result<usize, Error> {
        Ok(4 * (usize::from(self.height(self.check(root)?)) + 1) + 8)
    }
    pub(crate) fn clip_credit(
        &self,
        root: Root,
        after: Cut,
        cut: Cut,
        lookup: &impl Lookup<V>,
    ) -> Result<usize, Error> {
        let link = self.check(root)?;
        if after > cut {
            return Err(Error::InvalidEvent);
        }
        if link.is_none() || after == cut {
            return Ok(0);
        }
        let first = self
            .first(
                root,
                Cut::frame(crate::Frame(0)),
                Cut::at(Point {
                    frame: crate::Frame(u64::MAX),
                    order: u64::MAX,
                }),
                lookup,
            )?
            .unwrap();
        let last = self
            .last(
                root,
                Cut::at(Point {
                    frame: crate::Frame(u64::MAX),
                    order: u64::MAX,
                }),
                lookup,
            )?
            .unwrap();
        if !after.includes(lookup.point(first)) && cut.includes(lookup.point(last)) {
            return Ok(0);
        }
        Ok(8 * (usize::from(self.height(link)) + 1).pow(2) + 4)
    }
    fn join(
        &mut self,
        left: Link,
        value: V,
        right: Link,
        values: &mut impl Values<V>,
    ) -> NonZeroU32 {
        let a = self.height(left);
        let b = self.height(right);
        if a > b + 1 {
            let id = self.unique(left.unwrap(), values);
            let child = self.join(self.node(id).children[1], value, right, values);
            self.nodes[slot(id)].node.as_mut().unwrap().children[1] = Some(child);
            self.rebalance(id, values)
        } else if b > a + 1 {
            let id = self.unique(right.unwrap(), values);
            let child = self.join(left, value, self.node(id).children[0], values);
            self.nodes[slot(id)].node.as_mut().unwrap().children[0] = Some(child);
            self.rebalance(id, values)
        } else {
            let id = self.allocate(
                Node {
                    value,
                    children: [left, right],
                    summary: S::default(),
                    count: 0,
                    references: 1,
                    height: 1,
                },
                values,
            );
            self.recalculate(id, values);
            id
        }
    }
    fn trim_lower(&mut self, link: Link, after: Cut, values: &mut impl Values<V>) -> Link {
        let node = *self.node(link?);
        if after.includes(values.point(node.value)) {
            self.trim_lower(node.children[1], after, values)
        } else {
            let left = self.trim_lower(node.children[0], after, values);
            self.retain_link(node.children[1]);
            Some(self.join(left, node.value, node.children[1], values))
        }
    }
    fn trim_upper(&mut self, link: Link, cut: Cut, values: &mut impl Values<V>) -> Link {
        let node = *self.node(link?);
        if !cut.includes(values.point(node.value)) {
            self.trim_upper(node.children[0], cut, values)
        } else {
            let right = self.trim_upper(node.children[1], cut, values);
            self.retain_link(node.children[0]);
            Some(self.join(node.children[0], node.value, right, values))
        }
    }
    pub(crate) fn clip(
        &mut self,
        root: Root,
        after: Cut,
        cut: Cut,
        values: &mut impl Values<V>,
    ) -> Result<Root, Error> {
        let credit = self.clip_credit(root, after, cut, values)?;
        if after == cut {
            self.release(root)?;
            return Ok(self.empty());
        }
        if credit == 0 {
            return Ok(root);
        }
        if self.free_nodes() < credit {
            return Err(Error::Capacity);
        }
        let old = self.check(root)?;
        let lower = self.trim_lower(old, after, values);
        let clipped = self.trim_upper(lower, cut, values);
        self.release_link(lower);
        self.release(root)?;
        if clipped.is_some() {
            self.roots += 1;
        }
        Ok(self.root(clipped))
    }
    pub(crate) fn insert(
        &mut self,
        root: Root,
        value: V,
        values: &mut impl Values<V>,
    ) -> Result<Root, Error> {
        let link = self.check(root)?;
        if self.insertion_credit(link, values.point(value), values, false) > self.free.len()
            || (link.is_none() && self.roots == self.nodes.len())
        {
            return Err(Error::Capacity);
        }
        if link.is_none() {
            self.roots += 1;
        }
        let changed = self.insert_at(link, value, values);
        Ok(self.root(Some(changed)))
    }
    fn insert_at(&mut self, link: Link, value: V, values: &mut impl Values<V>) -> NonZeroU32 {
        let Some(id) = link else {
            return self.allocate(
                Node {
                    value,
                    children: [None; 2],
                    summary: S::value(value, values),
                    count: 1,
                    references: 1,
                    height: 1,
                },
                values,
            );
        };
        let id = self.unique(id, values);
        let old = *self.node(id);
        match values.point(value).cmp(&values.point(old.value)) {
            core::cmp::Ordering::Equal => {
                values.retain(value);
                values.release(old.value);
                self.nodes[slot(id)]
                    .node
                    .as_mut()
                    .expect("owned node")
                    .value = value;
            }
            order => {
                let side = usize::from(order.is_gt());
                let child = self.insert_at(old.children[side], value, values);
                self.nodes[slot(id)]
                    .node
                    .as_mut()
                    .expect("owned path")
                    .children[side] = Some(child);
            }
        }
        self.rebalance(id, values)
    }
    pub(crate) fn remove(
        &mut self,
        root: Root,
        point: Point,
        values: &mut impl Values<V>,
    ) -> Result<Root, Error> {
        let link = self.check(root)?;
        if self
            .last(root, Cut::at(point), values)?
            .is_none_or(|value| values.point(value) != point)
        {
            return Ok(root);
        }
        if self.free.len() < 4 * (usize::from(self.height(link)) + 1) + 8 {
            return Err(Error::Capacity);
        }
        let changed = self.remove_at(link, point, values);
        if link.is_some() && changed.is_none() {
            self.roots -= 1;
        }
        Ok(self.root(changed))
    }
    fn detach(&mut self, id: NonZeroU32) -> (V, Link) {
        let node = *self.node(id);
        debug_assert!(node.children[0].is_none());
        self.free_slot(id);
        (node.value, node.children[1])
    }
    fn pop_first(&mut self, id: NonZeroU32, values: &mut impl Values<V>) -> (V, Link) {
        let id = self.unique(id, values);
        let node = *self.node(id);
        if let Some(left) = node.children[0] {
            let (value, left) = self.pop_first(left, values);
            self.nodes[slot(id)]
                .node
                .as_mut()
                .expect("successor path")
                .children[0] = left;
            (value, Some(self.rebalance(id, values)))
        } else {
            self.detach(id)
        }
    }
    fn remove_at(&mut self, link: Link, point: Point, values: &mut impl Values<V>) -> Link {
        let id = self.unique(link?, values);
        let node = *self.node(id);
        match point.cmp(&values.point(node.value)) {
            core::cmp::Ordering::Equal => match node.children {
                [None, right] => {
                    self.free_slot(id);
                    values.release(node.value);
                    return right;
                }
                [left, None] => {
                    self.free_slot(id);
                    values.release(node.value);
                    return left;
                }
                [Some(_), Some(right)] => {
                    let (successor, right) = self.pop_first(right, values);
                    values.release(node.value);
                    let node = self.nodes[slot(id)].node.as_mut().expect("owned successor");
                    node.value = successor;
                    node.children[1] = right;
                }
            },
            order => {
                let side = usize::from(order.is_gt());
                let child = self.remove_at(node.children[side], point, values);
                self.nodes[slot(id)]
                    .node
                    .as_mut()
                    .expect("owned path")
                    .children[side] = child;
            }
        }
        Some(self.rebalance(id, values))
    }
    pub(crate) fn len(&self, root: Root) -> Result<usize, Error> {
        Ok(self.count(self.check(root)?))
    }
    pub(crate) fn record_number(&self, root: Root, mut ordinal: usize) -> Result<Option<V>, Error> {
        let mut link = self.check(root)?;
        while let Some(id) = link {
            let node = self.node(id);
            let left = self.count(node.children[0]);
            if ordinal < left {
                link = node.children[0];
            } else if ordinal == left {
                return Ok(Some(node.value));
            } else {
                ordinal -= left + 1;
                link = node.children[1];
            }
        }
        Ok(None)
    }
    pub(crate) fn tree_height(&self, root: Root) -> Result<usize, Error> {
        Ok(usize::from(self.height(self.check(root)?)))
    }
    pub(crate) fn last(
        &self,
        root: Root,
        cut: Cut,
        lookup: &impl Lookup<V>,
    ) -> Result<Option<V>, Error> {
        let mut link = self.check(root)?;
        let mut result = None;
        while let Some(id) = link {
            let node = self.node(id);
            if cut.includes(lookup.point(node.value)) {
                result = Some(node.value);
                link = node.children[1];
            } else {
                link = node.children[0];
            }
        }
        Ok(result)
    }
    pub(crate) fn first(
        &self,
        root: Root,
        after: Cut,
        cut: Cut,
        lookup: &impl Lookup<V>,
    ) -> Result<Option<V>, Error> {
        let mut link = self.check(root)?;
        let mut result = None;
        while let Some(id) = link {
            let node = self.node(id);
            if after.includes(lookup.point(node.value)) {
                link = node.children[1];
            } else {
                result = Some(node.value);
                link = node.children[0];
            }
        }
        Ok(result.filter(|value| cut.includes(lookup.point(*value))))
    }
    pub(crate) fn interval(
        &self,
        root: Root,
        after: Cut,
        cut: Cut,
        lookup: &impl Lookup<V>,
    ) -> Result<S, Error> {
        let link = self.check(root)?;
        Ok(if after >= cut {
            S::default()
        } else {
            self.interval_at(link, after, cut, lookup)
        })
    }
    fn prefix(&self, link: Link, cut: Cut, lookup: &impl Lookup<V>) -> S {
        let Some(id) = link else {
            return S::default();
        };
        let node = self.node(id);
        if cut.includes(lookup.point(node.value)) {
            self.summary(node.children[0])
                .then(S::value(node.value, lookup))
                .then(self.prefix(node.children[1], cut, lookup))
        } else {
            self.prefix(node.children[0], cut, lookup)
        }
    }
    fn suffix(&self, link: Link, after: Cut, lookup: &impl Lookup<V>) -> S {
        let Some(id) = link else {
            return S::default();
        };
        let node = self.node(id);
        if after.includes(lookup.point(node.value)) {
            self.suffix(node.children[1], after, lookup)
        } else {
            self.suffix(node.children[0], after, lookup)
                .then(S::value(node.value, lookup))
                .then(self.summary(node.children[1]))
        }
    }
    fn interval_at(&self, link: Link, after: Cut, cut: Cut, lookup: &impl Lookup<V>) -> S {
        let Some(id) = link else {
            return S::default();
        };
        let node = self.node(id);
        if after.includes(lookup.point(node.value)) {
            self.interval_at(node.children[1], after, cut, lookup)
        } else if !cut.includes(lookup.point(node.value)) {
            self.interval_at(node.children[0], after, cut, lookup)
        } else {
            self.suffix(node.children[0], after, lookup)
                .then(S::value(node.value, lookup))
                .then(self.prefix(node.children[1], cut, lookup))
        }
    }
}

#[cfg(test)]
mod tests;

impl<V: Copy> Pool<V, Totals> {
    pub(crate) fn record_at(
        &self,
        root: Root,
        mut offset: usize,
        lookup: &impl Lookup<V>,
    ) -> Result<Option<(V, usize)>, Error> {
        let mut link = self.check(root)?;
        while let Some(id) = link {
            let node = self.node(id);
            let left = self.summary(node.children[0]);
            if left.overflow {
                return Err(Error::Capacity);
            }
            if offset < left.bytes {
                link = node.children[0];
                continue;
            }
            offset -= left.bytes;
            let bytes = lookup
                .packet_bytes(node.value)
                .checked_add(12)
                .ok_or(Error::Capacity)?;
            if offset < bytes {
                return Ok(Some((node.value, offset)));
            }
            offset -= bytes;
            link = node.children[1];
        }
        Ok(None)
    }
}
