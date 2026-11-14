mod support;
use rt_runtime::midi::EventRecord;
use rt_runtime::midi_runtime::{Profile, Store, Version};
use rt_runtime::{Error, Frame};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn profile() -> Profile {
    Profile {
        pages: 128,
        tiles: 64,
        directories: 4096,
        versions: 64,
        tile_shift: 6,
        pages_per_tile: 8,
        touched_tiles: 4,
    }
}
fn event(frame: u64, order: u64, bytes: &[u8]) -> EventRecord {
    EventRecord::new(Frame(frame), order, bytes).unwrap()
}
fn read(store: &mut Store, version: Version, expected: &[EventRecord]) {
    assert_no_heap(|| {
        let mut after = None;
        let mut bytes = 0;
        for &row in expected {
            let actual = store.first(version, after).unwrap().unwrap();
            assert_eq!(actual, row);
            let len = 12 + row.message().as_bytes().len();
            for within in 0..len {
                assert_eq!(
                    store.record_at_byte(version, bytes + within as u64),
                    Ok(Some((row, within)))
                );
            }
            assert_eq!(
                store.byte_before(version, row.frame).unwrap(),
                expected
                    .iter()
                    .filter(|prior| prior.frame < row.frame)
                    .map(|prior| 12 + prior.message().as_bytes().len() as u64)
                    .sum::<u64>()
            );
            bytes += len as u64;
            after = Some((actual.frame, actual.order));
        }
        assert_eq!(store.first(version, after), Ok(None));
        let totals = store.totals(version).unwrap();
        assert_eq!(totals.events as usize, expected.len());
        assert_eq!(totals.bytes, bytes);
        assert_eq!(store.record_at_byte(version, bytes), Ok(None));
    });
}
fn drain(store: &mut Store) {
    assert_no_heap(|| {
        for _ in 0..100_000 {
            let before = store.work().retirement_steps;
            let used = store.maintain(1);
            assert!(used <= 1);
            assert_eq!(store.work().retirement_steps - before, used as u64);
            if used == 0 {
                return;
            }
        }
        panic!("bounded retirement must settle");
    });
}

#[shoop_test]
fn retained_block_versions_match_a_linear_edit_oracle_and_reclaim_every_pool() {
    let mut store = Store::prepare(profile()).unwrap();
    let baseline = store.usage();
    let empty = assert_no_heap(|| store.empty()).unwrap();
    let originals = [
        event(0, 0, &[0xc0, 7]),
        event(12, 0, &[0xb0, 7, 81]),
        event(64, 0, &[0x90, 60, 100]),
        event(64, 1, &[0x80, 60, 0]),
        event(100, 0, &[0xe0, 0, 64]),
    ];
    let first = assert_no_heap(|| store.edit(empty, Frame(0)..Frame(128), &originals)).unwrap();
    let replacement = [event(64, 0, &[0x90, 61, 90])];
    let mut oracle = originals.to_vec();
    oracle.retain(|row| !(64..100).contains(&row.frame.0));
    oracle.extend(replacement);
    oracle.sort_by_key(|row| (row.frame, row.order));
    let second = assert_no_heap(|| store.edit(first, Frame(64)..Frame(100), &replacement)).unwrap();
    read(&mut store, first, &originals);
    read(&mut store, second, &oracle);
    let clipped = assert_no_heap(|| store.clip(second, Frame(12)..Frame(100))).unwrap();
    let visible: Vec<_> = oracle
        .iter()
        .copied()
        .filter(|row| (12..100).contains(&row.frame.0))
        .collect();
    read(&mut store, clipped, &visible);
    assert_eq!(
        store.edit(clipped, Frame(0)..Frame(1), &[]),
        Err(Error::Busy)
    );
    assert_no_heap(|| {
        store.release(empty).unwrap();
        store.release(first).unwrap();
        store.release(second).unwrap();
    });
    drain(&mut store);
    read(&mut store, clipped, &visible);
    assert_no_heap(|| store.release(clipped)).unwrap();
    drain(&mut store);
    assert_eq!(store.usage(), baseline);
    assert_eq!(store.retain(first), Err(Error::InvalidId));
}

#[shoop_test]
fn dense_tiles_cross_pages_and_keep_equal_frame_originals_in_order() {
    let mut store = Store::prepare(profile()).unwrap();
    let empty = store.empty().unwrap();
    let originals: Vec<_> = (0..150)
        .map(|order| event(3, order, &[0x90, (order % 128) as u8, 1]))
        .collect();
    let version = assert_no_heap(|| store.edit(empty, Frame(0)..Frame(64), &originals)).unwrap();
    read(&mut store, version, &originals);
    assert_eq!(store.totals(version).unwrap().pages, 5);
    let replacement = [event(4, 0, &[0xff])];
    let edited = assert_no_heap(|| store.edit(version, Frame(3)..Frame(5), &replacement)).unwrap();
    read(&mut store, edited, &replacement);
    read(&mut store, version, &originals);
}

#[shoop_test]
fn capacity_refusal_preserves_content_and_foreign_or_stale_versions_are_rejected() {
    let mut constrained = profile();
    constrained.pages = 2;
    let mut store = Store::prepare(constrained).unwrap();
    let empty = store.empty().unwrap();
    let original = [event(1, 0, &[0xb0, 10, 17])];
    let version = store.edit(empty, Frame(0)..Frame(64), &original).unwrap();
    let before = store.usage();
    let too_many: Vec<_> = (0..33).map(|n| event(2, n, &[0x90, 60, 1])).collect();
    assert_eq!(
        assert_no_heap(|| store.edit(version, Frame(0)..Frame(64), &too_many)),
        Err(Error::Capacity)
    );
    assert_eq!(store.usage(), before);
    read(&mut store, version, &original);
    let mut foreign = Store::prepare(profile()).unwrap();
    assert_eq!(foreign.first(version, None), Err(Error::InvalidId));
    assert_no_heap(|| store.release(empty)).unwrap();
    assert_eq!(store.release(empty), Err(Error::InvalidId));
}

#[shoop_test]
fn directory_depth_and_work_do_not_grow_with_recording_duration() {
    let mut store = Store::prepare(profile()).unwrap();
    let empty = store.empty().unwrap();
    let frame = (1u64 << 55) + 1;
    let records = [event(frame, 0, &[0xd7, 42])];
    let version =
        assert_no_heap(|| store.edit(empty, Frame(frame)..Frame(frame + 1), &records)).unwrap();
    store.reset_work();
    assert_eq!(store.first(version, None), Ok(Some(records[0])));
    assert_eq!(store.work().directory_reads, 16);
    assert_eq!(store.work().record_reads, 1);
    read(&mut store, version, &records);
}

#[shoop_test]
fn normalized_grabs_extend_without_revealing_hidden_originals() {
    let mut store = Store::prepare(profile()).unwrap();
    let empty = store.empty().unwrap();
    let originals = [
        event(0, 0, &[0xc0, 3]),
        event(16, 0, &[0x90, 60, 100]),
        event(63, 0, &[0x80, 60, 0]),
        event(64, 0, &[0xc0, 12]),
        event(128, 0, &[0xc0, 13]),
    ];
    let version = store.edit(empty, Frame(0)..Frame(192), &originals).unwrap();
    let clip = store.clip(version, Frame(16)..Frame(64)).unwrap();
    store.reset_work();
    let normalized = assert_no_heap(|| store.normalize(clip)).unwrap();
    assert!(store.is_normalized(normalized).unwrap());
    assert!(store.work().directory_writes <= 32);
    let inserted = [event(129, 0, &[0x90, 62, 90])];
    let extended = assert_no_heap(|| store.append(normalized, &inserted)).unwrap();
    read(
        &mut store,
        extended,
        &[originals[1], originals[2], inserted[0]],
    );
    read(&mut store, version, &originals);
    let aligned = store.clip(version, Frame(64)..Frame(128)).unwrap();
    let aligned_normalized = assert_no_heap(|| store.normalize(aligned)).unwrap();
    read(&mut store, aligned_normalized, &originals[3..4]);
    store.release(aligned).unwrap();
    store.release(aligned_normalized).unwrap();
    let owners = [empty, version, clip, normalized, extended];
    assert_no_heap(|| {
        for owner in owners {
            store.release(owner).unwrap();
        }
    });
    drain(&mut store);
    assert_eq!(store.usage().pages, 0);
    assert_eq!(store.usage().directories, 0);
}
