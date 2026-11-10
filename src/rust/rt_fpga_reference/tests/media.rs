use rt_fpga_reference::{
    compiler::{Graph, Node, NodeId},
    fixed::Fixed,
    storage::{ChunkId, MediaStore, ViewId},
    Backend, Error,
};
use shoop_wasm_test_support::shoop_test;

fn samples(values: &[i32]) -> Vec<Fixed> {
    values.iter().copied().map(Fixed).collect()
}

fn read(store: &MediaStore, view: ViewId) -> Vec<Fixed> {
    let frames = store.frames(view).unwrap();
    let mut result = vec![Fixed::ZERO; frames];
    if frames > 0 {
        store
            .read_loop(view, 0, frames as u64, &mut result)
            .unwrap();
    }
    result
}

#[shoop_test]
fn arena_and_tables_grow_without_moving_ids_or_changing_existing_audio() {
    let mut store = MediaStore::new(3).unwrap();
    store.reserve_chunk_capacity(100).unwrap();
    assert_eq!(store.slot_count(), 0);
    let a = store.insert(&samples(&[1, 2, 3, 4])).unwrap();
    let ids = store.chunk_ids(a).unwrap().to_vec();
    assert_eq!(ids, [ChunkId(0), ChunkId(1)]);
    store.reserve_view_capacity(a, 1_000).unwrap();
    let b = store.insert(&vec![Fixed(77); 6_000]).unwrap();
    assert_eq!(store.slot_count(), 2_002);
    store.append(a, &samples(&[5, 6])).unwrap();
    assert_eq!(store.chunk_ids(a).unwrap(), ids);
    assert_eq!(read(&store, a), samples(&[1, 2, 3, 4, 5, 6]));
    assert_eq!(read(&store, b), vec![Fixed(77); 6_000]);
    let mut wrapped = [Fixed::ZERO; 11];
    store.read_loop(a, 4, 5, &mut wrapped).unwrap();
    assert_eq!(
        wrapped.as_slice(),
        samples(&[5, 1, 2, 3, 4, 5, 1, 2, 3, 4, 5])
    );
}

#[shoop_test]
fn clearing_and_removing_recycle_slots_in_deterministic_order_without_reviving_view_ids() {
    let mut store = MediaStore::new(2).unwrap();
    let a = store.insert(&samples(&[1, 2, 3, 4])).unwrap();
    let b = store.insert(&samples(&[5, 6, 7])).unwrap();
    store.remove(a).unwrap();
    assert_eq!(store.remove(a), Err(Error::InvalidView));
    assert_eq!(store.append(a, &samples(&[8])), Err(Error::InvalidView));
    let c = store.insert(&samples(&[11, 12, 13])).unwrap();
    assert_eq!(c, ViewId(2));
    assert_eq!(store.chunk_ids(c).unwrap(), [ChunkId(1), ChunkId(0)]);
    assert_eq!(store.slot_count(), 4);
    assert_eq!(read(&store, b), samples(&[5, 6, 7]));
    store.clear(c).unwrap();
    store.clear(c).unwrap();
    assert_eq!(store.free_chunk_count(), 2);
    let mut restored = MediaStore::from_snapshot(store.snapshot()).unwrap();
    for media in [&mut store, &mut restored] {
        media.append(c, &samples(&[20])).unwrap();
        assert_eq!(media.chunk_ids(c).unwrap(), [ChunkId(0)]);
        assert_eq!(read(media, c), samples(&[20]));
        let mut output = [Fixed(99); 2];
        assert_eq!(
            media.read_loop(c, 0, 2, &mut output),
            Err(Error::InvalidView)
        );
        assert_eq!(output, [Fixed(99); 2]);
    }
    assert_eq!(store.snapshot(), restored.snapshot());
}

#[shoop_test]
fn bounded_admission_is_atomic_and_reuses_released_space_before_growing() {
    let mut store = MediaStore::with_chunk_limit(4, 2).unwrap();
    let a = store.insert(&samples(&[1, 2, 3])).unwrap();
    let before = store.snapshot();
    assert_eq!(
        store.append(a, &samples(&[4, 5, 6, 7, 8, 9])),
        Err(Error::MemoryFull)
    );
    assert_eq!(store.snapshot(), before);
    store.append(a, &samples(&[4, 5, 6, 7, 8])).unwrap();
    assert_eq!(read(&store, a), samples(&[1, 2, 3, 4, 5, 6, 7, 8]));
    let before = store.snapshot();
    assert_eq!(store.insert(&samples(&[9])), Err(Error::MemoryFull));
    assert_eq!(store.snapshot(), before);
    let b = store.create_recording().unwrap();
    store.clear(a).unwrap();
    store
        .append(b, &samples(&[10, 11, 12, 13, 14, 15, 16, 17]))
        .unwrap();
    assert_eq!(store.chunk_ids(b).unwrap(), [ChunkId(1), ChunkId(0)]);
    assert_eq!(store.slot_count(), 2);
    assert_eq!(
        store.reserve_chunk_capacity(usize::MAX),
        Err(Error::MemoryFull)
    );

    let mut empty = MediaStore::with_chunk_limit(32, 0).unwrap();
    let view = empty.create_recording().unwrap();
    empty.append(view, &[]).unwrap();
    assert_eq!(empty.append(view, &samples(&[1])), Err(Error::MemoryFull));
    assert_eq!(empty.frames(view), Ok(0));
}

#[shoop_test]
fn imported_snapshots_reject_invalid_ownership_addresses_and_geometry() {
    let mut store = MediaStore::new(2).unwrap();
    let a = store.insert(&samples(&[1, 2, 3])).unwrap();
    let b = store.insert(&samples(&[4])).unwrap();
    store.remove(b).unwrap();
    let good = store.snapshot();
    assert_eq!(
        MediaStore::from_snapshot(good.clone()).unwrap().snapshot(),
        good
    );
    let mut invalid = Vec::new();
    let mut duplicate = good.clone();
    duplicate.views[a.0].as_mut().unwrap().chunks[1] = ChunkId(0);
    invalid.push(duplicate);
    let mut alias = good.clone();
    alias.free_chunks[0] = ChunkId(0);
    invalid.push(alias);
    let mut outside = good.clone();
    outside.free_chunks[0] = ChunkId(3);
    invalid.push(outside);
    let mut leaked = good.clone();
    leaked.free_chunks.clear();
    invalid.push(leaked);
    let mut length = good.clone();
    length.views[a.0].as_mut().unwrap().frames = 2;
    invalid.push(length);
    let mut fractional = good.clone();
    fractional.samples.pop();
    invalid.push(fractional);
    let mut limit = good.clone();
    limit.chunk_limit = Some(2);
    invalid.push(limit);
    let mut zero = good;
    zero.chunk_frames = 0;
    invalid.push(zero);
    for snapshot in invalid {
        assert!(MediaStore::from_snapshot(snapshot).is_err());
    }
}

#[shoop_test]
fn the_backend_records_and_plays_recycled_chunks_through_the_same_program() {
    let mut media = MediaStore::with_chunk_limit(3, 2).unwrap();
    let view = media.create_recording().unwrap();
    let graph = Graph {
        quantum: 4,
        nodes: vec![
            Node::Input { port: 0 },
            Node::Gain {
                input: NodeId(0),
                gain: Fixed(1 << 23),
            },
            Node::Capture {
                input: NodeId(1),
                view,
                recording: true,
            },
            Node::Playback {
                view,
                length: 4,
                position: 0,
                playing: false,
            },
            Node::Output {
                input: NodeId(3),
                port: 0,
            },
        ],
    };
    let mut backend = Backend::new(&graph, media).unwrap();
    let program = backend.program.clone();
    let address = backend.program.instructions.as_ptr();
    let mut output = [Fixed::ZERO; 4];
    let first = samples(&[1, -3, i32::MAX, i32::MIN]);
    backend.process(&[&first], &mut [&mut output]).unwrap();
    backend.set_enabled(NodeId(2), false).unwrap();
    backend.set_enabled(NodeId(3), true).unwrap();
    backend.process(&[&first], &mut [&mut output]).unwrap();
    assert_eq!(
        output.as_slice(),
        samples(&[0, -1, i32::MAX / 2, i32::MIN / 2])
    );
    assert!(backend.queue.is_empty());
    backend.media.clear(view).unwrap();
    backend.set_enabled(NodeId(2), true).unwrap();
    backend.set_enabled(NodeId(3), false).unwrap();
    let second = samples(&[10, 20, 30, 40]);
    backend.process(&[&second], &mut [&mut output]).unwrap();
    backend.set_enabled(NodeId(2), false).unwrap();
    backend.set_enabled(NodeId(3), true).unwrap();
    backend.process(&[&second], &mut [&mut output]).unwrap();
    assert_eq!(output.as_slice(), samples(&[5, 10, 15, 20]));
    assert_eq!(
        backend.media.chunk_ids(view).unwrap(),
        [ChunkId(1), ChunkId(0)]
    );
    assert_eq!(backend.media.slot_count(), 2);
    assert_eq!(backend.program, program);
    assert_eq!(backend.program.instructions.as_ptr(), address);
}

#[shoop_test]
fn seeded_growth_reclamation_and_wrapped_reads_match_independent_sample_vectors() {
    for chunk_frames in [1, 3, 32] {
        let mut store = MediaStore::new(chunk_frames).unwrap();
        let mut expected: Vec<Option<Vec<Fixed>>> = Vec::new();
        let mut seed = 0x1234_5678u32;
        for step in 0..1_000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if expected.is_empty() || seed % 7 == 0 {
                assert_eq!(store.create_recording().unwrap(), ViewId(expected.len()));
                expected.push(Some(Vec::new()));
            }
            let index = seed as usize % expected.len();
            let id = ViewId(index);
            let Some(model) = &mut expected[index] else {
                continue;
            };
            match seed % 5 {
                0 => {
                    store.clear(id).unwrap();
                    model.clear();
                }
                1 => {
                    store.remove(id).unwrap();
                    expected[index] = None;
                    continue;
                }
                _ => {
                    let input = vec![Fixed(seed as i32); (seed >> 16) as usize % 19];
                    store.append(id, &input).unwrap();
                    model.extend(input);
                }
            }
            assert_eq!(store.frames(id), Ok(model.len()));
            if !model.is_empty() {
                let length = 1 + seed as usize % model.len();
                let position = (seed >> 8) as usize % length;
                let mut output = vec![Fixed::ZERO; 45];
                store
                    .read_loop(id, position as u64, length as u64, &mut output)
                    .unwrap();
                for (offset, sample) in output.iter().enumerate() {
                    assert_eq!(*sample, model[(position + offset) % length]);
                }
            }
            if step % 37 == 0 {
                MediaStore::from_snapshot(store.snapshot()).unwrap();
            }
        }
    }
}
