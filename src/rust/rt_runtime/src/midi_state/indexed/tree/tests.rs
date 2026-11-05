use super::*;
use crate::{midi::Message, Frame};
use core::fmt::Debug;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Owned {
    point: Point,
    id: usize,
}
struct Counts([usize; 256]);
impl Lookup<Owned> for Counts {
    fn point(&self, value: Owned) -> Point {
        value.point
    }
    fn event(&self, _: Owned) -> Option<Event> {
        None
    }
}
impl Values<Owned> for Counts {
    fn retain(&mut self, value: Owned) {
        self.0[value.id] += 1;
    }
    fn release(&mut self, value: Owned) {
        assert!(self.0[value.id] > 0);
        self.0[value.id] -= 1;
    }
}
fn validate<V: Copy, S: Summary<V>>(
    pool: &Pool<V, S>,
    link: Link,
    low: Option<Point>,
    high: Option<Point>,
    lookup: &impl Lookup<V>,
) -> (u8, usize) {
    let Some(id) = link else {
        return (0, 0);
    };
    let node = pool.node(id);
    let point = lookup.point(node.value);
    assert!(low.is_none_or(|low| low < point));
    assert!(high.is_none_or(|high| point < high));
    let (left, a) = validate(pool, node.children[0], low, Some(point), lookup);
    let (right, b) = validate(pool, node.children[1], Some(point), high, lookup);
    assert!((i16::from(left) - i16::from(right)).abs() <= 1);
    assert_eq!(node.height, left.max(right) + 1);
    assert_eq!(node.count, a + b + 1);
    (node.height, node.count)
}
fn ownership(
    pool: &Pool<Owned, ()>,
    values: &Counts,
    roots: &[Option<Root>],
    incoming: &mut [usize],
) {
    incoming.fill(0);
    let mut expected = [0; 256];
    for entry in &pool.nodes {
        if let Some(node) = entry.node {
            expected[node.value.id] += 1;
            for id in node.children.into_iter().flatten() {
                incoming[slot(id)] += 1;
            }
        }
    }
    let mut count = 0;
    for root in roots.iter().flatten() {
        if let Some(id) = root.link {
            incoming[slot(id)] += 1;
            count += 1;
        }
    }
    assert_eq!(count, pool.root_references());
    for (entry, expected) in pool.nodes.iter().zip(incoming) {
        if let Some(node) = entry.node {
            assert_eq!(node.references as usize, *expected);
        }
    }
    assert_eq!(values.0, expected);
}

#[shoop_test]
fn prepared_ordered_edits_preserve_snapshots_balance_and_exact_value_references() {
    let mut pool = Pool::<Owned, ()>::prepare(4096).unwrap();
    let mut root = pool.empty();
    let mut snapshots = [None; 8];
    let mut expected = [None; 128];
    let mut saved = [[None; 128]; 8];
    let mut incoming = vec![0; 4096];
    let mut values = Counts([0; 256]);
    let origin = u64::from(u32::MAX) + 100;
    assert_no_heap(|| {
        let mut seed = 0x172b_12e0_d12a_8365u64;
        for step in 0..512 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let key = ((seed >> 32) & 127) as usize;
            let point = Point {
                frame: Frame(origin + key as u64),
                order: 0,
            };
            if seed & 3 != 0 {
                let id = key + ((seed >> 42) as usize & 1) * 128;
                root = pool.insert(root, Owned { point, id }, &mut values).unwrap();
                expected[key] = Some(id);
            } else {
                root = pool.remove(root, point, &mut values).unwrap();
                expected[key] = None;
            }
            if step % 32 == 0 {
                let index = step / 32 % snapshots.len();
                if let Some(old) = snapshots[index] {
                    pool.release(old).unwrap();
                }
                let low = if step % 64 == 0 { 0 } else { key / 2 };
                let high = if step % 96 == 0 { low } else { 128 - key / 2 };
                let retained = pool.retain(root).unwrap();
                snapshots[index] = Some(
                    pool.clip(
                        retained,
                        Cut::frame(Frame(origin + low as u64)),
                        Cut::frame(Frame(origin + high as u64)),
                        &mut values,
                    )
                    .unwrap(),
                );
                saved[index] = core::array::from_fn(|key| {
                    if (low..high).contains(&key) {
                        expected[key]
                    } else {
                        None
                    }
                });
            }
            assert!(pool.maintain(3, &mut values) <= 3);
            let mut roots = [None; 9];
            roots[0] = Some(root);
            roots[1..].copy_from_slice(&snapshots);
            ownership(&pool, &values, &roots, &mut incoming);
            assert_eq!(pool.len(root).unwrap(), expected.iter().flatten().count());
            validate(&pool, root.link, None, None, &values);
            for (snapshot, expected) in snapshots.iter().zip(&saved) {
                if let Some(snapshot) = snapshot {
                    validate(&pool, snapshot.link, None, None, &values);
                    for key in 0..128 {
                        let point = Point {
                            frame: Frame(origin + key as u64),
                            order: 0,
                        };
                        let last = expected[..=key].iter().rev().find_map(|value| *value);
                        assert_eq!(
                            pool.last(*snapshot, Cut::at(point), &values)
                                .unwrap()
                                .map(|value| value.id),
                            last
                        );
                        let first = expected[key..].iter().find_map(|value| *value);
                        assert_eq!(
                            pool.first(
                                *snapshot,
                                Cut::before(point),
                                Cut::frame(Frame(origin + 128)),
                                &values,
                            )
                            .unwrap()
                            .map(|value| value.id),
                            first
                        );
                    }
                }
            }
        }
        pool.release(root).unwrap();
        for snapshot in snapshots.into_iter().flatten() {
            pool.release(snapshot).unwrap();
        }
        while pool.maintain(1, &mut values) > 0 {}
        assert_eq!(pool.free_nodes(), 4096);
        assert_eq!(pool.root_references(), 0);
        assert_eq!(values.0, [0; 256]);
        assert!(!pool.work().counter_overflow);
    });
}

fn summary_case<S: Summary<Event> + Eq + Debug>(controller: u8) {
    let origin = u64::from(u32::MAX) + 10;
    let mut events: Vec<_> = (0..128)
        .map(|order| {
            let cc = if order == 0 {
                controller
            } else {
                [64, 69, 66, 120, 121][order as usize % 5]
            };
            Event::from_message(
                Point {
                    frame: Frame(origin + order / 4),
                    order,
                },
                Message::parse(&[0xb0, cc, if order % 3 == 0 { 127 } else { 0 }], 3).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let mut pool = Pool::<Event, S>::prepare(4096).unwrap();
    let mut root = pool.empty();
    assert_no_heap(|| {
        for index in 0..128 {
            root = pool
                .insert(root, events[(index * 37) % 128], &mut ())
                .unwrap();
        }
        let height = pool.tree_height(root).unwrap();
        for begin in 0..32 {
            for end in begin..=32 {
                let after = Cut::frame(Frame(origin + begin));
                let cut = Cut::frame(Frame(origin + end));
                let expected = events
                    .iter()
                    .copied()
                    .filter(|event| !after.includes(event.point) && cut.includes(event.point))
                    .fold(S::default(), |summary, value| {
                        summary.then(S::value(value, &()))
                    });
                pool.reset_work();
                assert_eq!(pool.interval(root, after, cut, &()).unwrap(), expected);
                assert!(pool.work().visited <= 8 * (height + 1));
            }
        }
        let old = pool.retain(root).unwrap();
        let first = events[0];
        events[0] = Event::from_message(
            first.point,
            Message::parse(&[0xb0, controller, 0], 3).unwrap(),
        )
        .unwrap();
        root = pool.insert(root, events[0], &mut ()).unwrap();
        assert_eq!(
            pool.last(old, Cut::at(first.point), &()).unwrap(),
            Some(first)
        );
        assert_eq!(
            pool.last(root, Cut::at(first.point), &()).unwrap(),
            Some(events[0])
        );
        root = pool.remove(root, events[5].point, &mut ()).unwrap();
        let after = Cut::frame(Frame(origin));
        let cut = Cut::frame(Frame(origin + 32));
        let expected = events
            .iter()
            .copied()
            .enumerate()
            .filter(|(index, _)| *index != 5)
            .fold(S::default(), |summary, (_, value)| {
                summary.then(S::value(value, &()))
            });
        assert_eq!(pool.interval(root, after, cut, &()).unwrap(), expected);
        assert_eq!(pool.len(root).unwrap(), 127);
        assert_eq!(pool.len(old).unwrap(), 128);
        pool.release(root).unwrap();
        pool.release(old).unwrap();
        while pool.maintain(3, &mut ()) > 0 {}
        assert_eq!(pool.free_nodes(), 4096);
    });
}

#[shoop_test]
fn interval_tree_queries_and_prefix_edits_keep_logarithmic_work_and_immutable_roots() {
    summary_case::<HoldSummary>(64);
    summary_case::<SostenutoSummary>(66);
}

#[shoop_test]
fn metadata_pressure_refuses_before_mutation_and_root_release_needs_no_workspace() {
    let mut pool = Pool::<Event, ()>::prepare(64).unwrap();
    let other = Pool::<Event, ()>::prepare(64).unwrap();
    let event = Event::from_message(
        Point {
            frame: Frame(1),
            order: 0,
        },
        Message::parse(&[0x90, 60, 90], 3).unwrap(),
    )
    .unwrap();
    let mut root = pool.empty();
    assert_no_heap(|| {
        root = pool.insert(root, event, &mut ()).unwrap();
        let snapshot = pool.retain(root).unwrap();
        let held = core::mem::take(&mut pool.free);
        assert_eq!(pool.insert(root, event, &mut ()), Err(Error::Capacity));
        assert_eq!(
            pool.remove(root, event.point, &mut ()),
            Err(Error::Capacity)
        );
        assert_eq!(pool.retain(other.empty()), Err(Error::InvalidId));
        assert_eq!(pool.len(root), Ok(1));
        assert_eq!(pool.len(snapshot), Ok(1));
        pool.release(root).unwrap();
        pool.release(snapshot).unwrap();
        assert_eq!(pool.len(root), Err(Error::InvalidId));
        pool.free = held;
        assert_eq!(pool.maintain(1, &mut ()), 1);
        assert_eq!(pool.free_nodes(), 64);
        assert_eq!(pool.root_references(), 0);
    });
}

struct Records {
    events: Vec<Event>,
    references: Vec<usize>,
}
impl Lookup<NonZeroU32> for Records {
    fn point(&self, value: NonZeroU32) -> Point {
        self.events[slot(value)].point
    }
    fn event(&self, value: NonZeroU32) -> Option<Event> {
        Some(self.events[slot(value)])
    }
}
impl Values<NonZeroU32> for Records {
    fn retain(&mut self, value: NonZeroU32) {
        self.references[slot(value)] += 1;
    }
    fn release(&mut self, value: NonZeroU32) {
        let references = &mut self.references[slot(value)];
        assert!(*references > 0);
        *references -= 1;
    }
}

#[shoop_test]
fn compact_sostenuto_markers_retain_records_and_resolve_only_the_query_result() {
    assert!(size_of::<SostenutoIndex<NonZeroU32>>() <= 32);
    let mut records = Records {
        events: Vec::new(),
        references: vec![0; 65],
    };
    for order in 0..64 {
        let cc = [66, 120, 121][order as usize % 3];
        records.events.push(
            Event::from_message(
                Point {
                    frame: Frame(order / 4),
                    order,
                },
                Message::parse(&[0xb0, cc, if order % 4 == 0 { 0 } else { 127 }], 3).unwrap(),
            )
            .unwrap(),
        );
    }
    records.events.push(
        Event::from_message(
            records.events[0].point,
            Message::parse(&[0xb0, 66, 127], 3).unwrap(),
        )
        .unwrap(),
    );
    let mut pool = Pool::<NonZeroU32, SostenutoIndex<NonZeroU32>>::prepare(1024).unwrap();
    let mut root = pool.empty();
    assert_no_heap(|| {
        for index in 0..64 {
            root = pool
                .insert(root, NonZeroU32::new(index + 1).unwrap(), &mut records)
                .unwrap();
        }
        let old = pool.retain(root).unwrap();
        root = pool
            .insert(root, NonZeroU32::new(65).unwrap(), &mut records)
            .unwrap();
        for (root, changed) in [(old, false), (root, true)] {
            let after = Cut::frame(Frame(0));
            let cut = Cut::frame(Frame(16));
            let compact = pool.interval(root, after, cut, &records).unwrap().0;
            let resolved = compact.map(|record| records.point(record));
            let expected = (0..64).fold(SostenutoSummary::default(), |summary, index| {
                let event = records.events[if index == 0 && changed { 64 } else { index }];
                summary.then(SostenutoSummary::update(
                    event.sostenuto().unwrap(),
                    event.point,
                ))
            });
            assert_eq!(resolved, expected);
        }
        pool.release(root).unwrap();
        pool.release(old).unwrap();
        while pool.maintain(1, &mut records) > 0 {}
        assert_eq!(pool.free_nodes(), 1024);
        assert!(records.references.iter().all(|&references| references == 0));
    });
}
