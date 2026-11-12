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
fn unaligned_slices_survive_source_clear_and_slot_reuse_without_copying_payloads() {
    let mut store = MediaStore::with_chunk_limit(4, 3).unwrap();
    let original = samples(&[1, 2, 3, i32::MIN, i32::MAX, -17, 7, 8, 9, 10, 11, 12]);
    let a = store.insert(&original).unwrap();
    let before = store.snapshot();
    let b = store.slice(a, 3, 5).unwrap();
    assert_eq!(store.snapshot().samples, before.samples);
    assert_eq!(store.slot_count(), 3);
    assert_eq!(store.chunk_ids(b).unwrap(), [ChunkId(0), ChunkId(1)]);
    assert_eq!(
        store.snapshot().views[b.0]
            .as_ref()
            .unwrap()
            .range
            .unwrap()
            .offset,
        3
    );
    assert_eq!(store.chunk_references(ChunkId(0)), Ok(2));
    assert_eq!(store.chunk_references(ChunkId(1)), Ok(2));
    assert_eq!(store.chunk_references(ChunkId(2)), Ok(1));
    let expected = &original[3..8];
    assert_eq!(read(&store, b), expected);
    store.clear(a).unwrap();
    assert_eq!(store.free_chunk_count(), 1);
    let replacement = samples(&[91, 92, 93, 94]);
    store.append(a, &replacement).unwrap();
    assert_eq!(store.chunk_ids(a).unwrap(), [ChunkId(2)]);
    assert_eq!(read(&store, b), expected);
    let mut wrapped = [Fixed::ZERO; 13];
    store.read_loop(b, 4, 5, &mut wrapped).unwrap();
    for (index, sample) in wrapped.iter().enumerate() {
        assert_eq!(*sample, expected[(4 + index) % 5]);
    }
    let mut restored = MediaStore::from_snapshot(store.snapshot()).unwrap();
    for media in [&mut store, &mut restored] {
        media.clear(b).unwrap();
        assert_eq!(media.free_chunk_count(), 2);
        assert_eq!(media.append(b, &[]), Err(Error::InvalidState));
        media
            .append(a, &samples(&[100, 101, 102, 103, 104, 105, 106, 107]))
            .unwrap();
        assert_eq!(
            media.chunk_ids(a).unwrap(),
            [ChunkId(2), ChunkId(1), ChunkId(0)]
        );
        assert_eq!(media.slot_count(), 3);
        assert_eq!(media.free_chunk_count(), 0);
        assert_eq!(
            read(media, a),
            samples(&[91, 92, 93, 94, 100, 101, 102, 103, 104, 105, 106, 107])
        );
    }
    assert_eq!(store.snapshot(), restored.snapshot());
}

#[shoop_test]
fn nested_and_empty_slices_outlive_their_sources_and_preserve_partial_tails() {
    let mut store = MediaStore::new(4).unwrap();
    let a = store.insert(&samples(&[1, 2, 3])).unwrap();
    let b = store.slice(a, 1, 2).unwrap();
    let empty = store.slice(b, 2, 0).unwrap();
    let tail = store.slice(b, 1, 1).unwrap();
    store.append(a, &samples(&[4, 5, 6, 7, 8, 9])).unwrap();
    assert_eq!(store.chunk_references(ChunkId(0)), Ok(3));
    assert_eq!(read(&store, b), samples(&[2, 3]));
    assert_eq!(store.frames(empty), Ok(0));
    assert!(store.chunk_ids(empty).unwrap().is_empty());
    let before = store.snapshot();
    assert_eq!(store.append(b, &samples(&[7])), Err(Error::InvalidState));
    assert_eq!(
        store.reserve_view_capacity(b, 100),
        Err(Error::InvalidState)
    );
    for (start, frames) in [(3, 0), (2, 1), (0, 3), (u64::MAX, 1)] {
        assert_eq!(store.slice(b, start, frames), Err(Error::InvalidView));
    }
    assert_eq!(store.snapshot(), before);
    store.remove(a).unwrap();
    store.remove(b).unwrap();
    assert_eq!(read(&store, tail), samples(&[3]));
    assert_eq!(store.chunk_references(ChunkId(0)), Ok(1));
    let mut restored = MediaStore::from_snapshot(store.snapshot()).unwrap();
    assert_eq!(read(&restored, tail), samples(&[3]));
    restored.remove(tail).unwrap();
    assert_eq!(restored.free_chunk_count(), restored.slot_count());
    assert_eq!(restored.slice(a, 0, 0), Err(Error::InvalidView));
    assert_eq!(restored.slice(empty, 0, 0), Ok(ViewId(4)));
}

#[shoop_test]
fn incremental_release_counts_shared_chunks_and_respects_slice_offsets() {
    let mut store = MediaStore::new(4).unwrap();
    let a = store
        .insert(&samples(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]))
        .unwrap();
    let b = store.slice(a, 3, 6).unwrap();
    for length in [5, 1, 0] {
        assert_eq!(store.reclaim_chunk(b).unwrap(), length == 0);
        assert_eq!(store.frames(b), Ok(length));
        assert_eq!(read(&store, b), samples(&[4, 5, 6, 7, 8][..length]));
        assert_eq!(store.free_chunk_count(), 0);
        MediaStore::from_snapshot(store.snapshot()).unwrap();
    }
    assert!(store.reclaim_chunk(b).unwrap());
    assert_eq!(store.append(b, &[Fixed(7)]), Err(Error::InvalidState));
    for length in [8, 4, 0] {
        let free = store.free_chunk_count();
        assert_eq!(store.reclaim_chunk(a).unwrap(), length == 0);
        assert_eq!(store.frames(a), Ok(length));
        assert_eq!(store.free_chunk_count(), free + 1);
    }
}

#[shoop_test]
fn bounded_capture_cannot_reuse_chunks_retained_by_slices_and_failures_are_atomic() {
    let mut store = MediaStore::with_chunk_limit(4, 1).unwrap();
    let a = store.insert(&samples(&[1, 2, 3])).unwrap();
    let b = store.slice(a, 1, 2).unwrap();
    store.append(a, &[Fixed(4)]).unwrap();
    let before = store.snapshot();
    assert_eq!(store.append(a, &[Fixed(5)]), Err(Error::MemoryFull));
    assert_eq!(store.snapshot(), before);
    assert_eq!(store.chunk_references(ChunkId(0)), Ok(2));
    store.clear(a).unwrap();
    let before = store.snapshot();
    assert_eq!(store.append(a, &[Fixed(9)]), Err(Error::MemoryFull));
    assert_eq!(store.snapshot(), before);
    assert_eq!(read(&store, b), samples(&[2, 3]));
    store.remove(b).unwrap();
    store.append(a, &[Fixed(9)]).unwrap();
    assert_eq!(read(&store, a), samples(&[9]));
    assert_eq!(store.chunk_references(ChunkId(0)), Ok(1));
}

#[shoop_test]
fn snapshot_restore_rejects_writable_aliases_and_slices_into_unpublished_tail_samples() {
    let mut store = MediaStore::new(4).unwrap();
    let a = store.insert(&samples(&[1, 2, 3])).unwrap();
    let b = store.slice(a, 1, 2).unwrap();
    let good = store.snapshot();
    let mut invalid = Vec::new();
    let mut writers = good.clone();
    let alias = writers.views[a.0].clone();
    writers.views.push(alias);
    invalid.push(writers);
    let mut future = good.clone();
    future.views[b.0]
        .as_mut()
        .unwrap()
        .range
        .as_mut()
        .unwrap()
        .offset = 2;
    invalid.push(future);
    let mut offset = good.clone();
    offset.views[b.0]
        .as_mut()
        .unwrap()
        .range
        .as_mut()
        .unwrap()
        .offset = 4;
    invalid.push(offset);
    let mut writable = good.clone();
    writable.views[b.0].as_mut().unwrap().range = None;
    invalid.push(writable);
    let mut free = good.clone();
    free.free_chunks.push(ChunkId(0));
    invalid.push(free);
    let mut duplicate = good;
    let view = duplicate.views[b.0].as_mut().unwrap();
    view.frames = 5;
    view.chunks.push(ChunkId(0));
    invalid.push(duplicate);
    for snapshot in invalid {
        assert!(MediaStore::from_snapshot(snapshot).is_err());
    }
}

#[shoop_test]
fn consecutive_views_and_incremental_retirement_preserve_other_recordings() {
    let mut store = MediaStore::new(3).unwrap();
    let before = store.snapshot();
    assert_eq!(store.create_recordings(0), Err(Error::InvalidShape));
    assert_eq!(store.snapshot(), before);
    let first = store.create_recordings(2).unwrap();
    let second = ViewId(first.0 + 1);
    store.append(first, &samples(&[1, 2, 3, 4])).unwrap();
    store.append(second, &samples(&[5, 6])).unwrap();
    assert!(!store.reclaim_chunk(first).unwrap());
    assert_eq!(read(&store, first), samples(&[1, 2, 3]));
    assert_eq!(read(&store, second), samples(&[5, 6]));
    assert_eq!(store.free_chunk_count(), 1);
    assert!(store.reclaim_chunk(first).unwrap());
    assert!(store.reclaim_chunk(first).unwrap());
    assert_eq!(store.free_chunk_count(), 2);
    store.remove(first).unwrap();
    assert_eq!(store.reclaim_chunk(first), Err(Error::InvalidView));
    let next = store.create_recordings(2).unwrap();
    assert_eq!(next, ViewId(second.0 + 1));
    store.append(next, &samples(&[7, 8, 9, 10])).unwrap();
    assert_eq!(store.chunk_ids(next).unwrap(), [ChunkId(0), ChunkId(1)]);
    assert_eq!(store.slot_count(), 3);
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
    let mut media = MediaStore::with_chunk_limit(2, 2).unwrap();
    let view = media.create_recording().unwrap();
    let graph = Graph {
        chunk_frames: 2,
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
                length: 2,
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
fn seeded_slices_and_source_reuse_match_independent_immutable_sample_ranges() {
    for chunk_frames in [1, 3, 32] {
        let mut store = MediaStore::new(chunk_frames).unwrap();
        let mut expected: Vec<Option<Vec<Fixed>>> = Vec::new();
        let mut read_only = Vec::new();
        let mut seed = 0xcafe_babeu32;
        for step in 0..2_000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if expected.is_empty() || seed % 11 == 0 {
                assert_eq!(store.create_recording().unwrap(), ViewId(expected.len()));
                expected.push(Some(Vec::new()));
                read_only.push(false);
            }
            let index = (seed >> 8) as usize % expected.len();
            let id = ViewId(index);
            let Some(model) = expected[index].as_ref() else {
                continue;
            };
            match seed % 6 {
                0 | 1 => {
                    let start = (seed >> 16) as usize % (model.len() + 1);
                    let length = (seed >> 3) as usize % (model.len() - start + 1);
                    let range = model[start..start + length].to_vec();
                    let next = store.slice(id, start as u64, length as u64).unwrap();
                    assert_eq!(next, ViewId(expected.len()));
                    expected.push(Some(range));
                    read_only.push(true);
                }
                2 => {
                    store.clear(id).unwrap();
                    expected[index].as_mut().unwrap().clear();
                }
                3 => {
                    store.remove(id).unwrap();
                    expected[index] = None;
                }
                _ => {
                    let input = vec![Fixed(seed as i32); (seed >> 16) as usize % 19];
                    if read_only[index] {
                        assert_eq!(store.append(id, &input), Err(Error::InvalidState));
                    } else {
                        store.append(id, &input).unwrap();
                        expected[index].as_mut().unwrap().extend(input);
                    }
                }
            }
            for (index, model) in expected.iter().enumerate() {
                if let Some(model) = model {
                    assert_eq!(read(&store, ViewId(index)), *model);
                }
            }
            if step % 37 == 0 {
                store = MediaStore::from_snapshot(store.snapshot()).unwrap();
            }
        }
        for (index, model) in expected.iter().enumerate() {
            if model.is_some() {
                store.remove(ViewId(index)).unwrap();
            }
        }
        assert_eq!(store.free_chunk_count(), store.slot_count());
        for slot in 0..store.slot_count() {
            assert_eq!(store.chunk_references(ChunkId(slot)), Ok(0));
        }
    }
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

#[shoop_test]
fn allocation_blocks_share_ranges_and_reclaim_once_per_block() {
    let mut media = MediaStore::with_block_chunks(4, 32).unwrap();
    let a = media
        .insert(&samples(&(0..140).collect::<Vec<_>>()))
        .unwrap();
    assert_eq!(media.slot_count(), 64);
    let b = media.slice(a, 123, 10).unwrap();
    let c = media.slice(b, 2, 6).unwrap();
    assert_eq!(read(&media, b), samples(&(123..133).collect::<Vec<_>>()));
    assert_eq!(read(&media, c), samples(&(125..131).collect::<Vec<_>>()));
    assert_eq!(media.chunk_references(ChunkId(0)), Ok(3));
    assert_eq!(media.chunk_references(ChunkId(32)), Ok(3));
    let restored = MediaStore::from_snapshot(media.snapshot()).unwrap();
    assert_eq!(restored.snapshot(), media.snapshot());
    media.clear(a).unwrap();
    assert_eq!(media.free_chunk_count(), 0);
    media.remove(b).unwrap();
    assert!(!media.reclaim_chunk(c).unwrap());
    assert_eq!(media.frames(c), Ok(3));
    assert_eq!(media.free_chunk_count(), 32);
    assert!(media.reclaim_chunk(c).unwrap());
    assert_eq!(media.free_chunk_count(), 64);
    let d = media.insert(&samples(&[901, 902, 903])).unwrap();
    assert_eq!(media.chunk_ids(d).unwrap(), [ChunkId(0)]);
    assert_eq!(read(&media, d), samples(&[901, 902, 903]));
}

#[shoop_test]
fn blocked_append_keeps_grabbed_prefix_and_refuses_growth_before_mutation() {
    let mut media = MediaStore::with_block_chunks(4, 32).unwrap();
    let a = media.insert(&samples(&[1, 2, 3])).unwrap();
    let b = media.slice(a, 1, 2).unwrap();
    media
        .append(a, &samples(&(4..=128).collect::<Vec<_>>()))
        .unwrap();
    assert_eq!(media.slot_count(), 32);
    assert_eq!(media.chunk_references(ChunkId(31)), Ok(2));
    assert_eq!(read(&media, b), samples(&[2, 3]));
    let mut image = media.snapshot();
    image.chunk_limit = Some(32);
    let mut limited = MediaStore::from_snapshot(image).unwrap();
    let before = limited.snapshot();
    assert_eq!(limited.append(a, &samples(&[129])), Err(Error::MemoryFull));
    assert_eq!(limited.snapshot(), before);
    media.append(a, &samples(&[129])).unwrap();
    assert_eq!(media.slot_count(), 64);
    media.clear(a).unwrap();
    assert_eq!(read(&media, b), samples(&[2, 3]));
    assert_eq!(media.free_chunk_count(), 32);
    let restored = MediaStore::from_snapshot(media.snapshot()).unwrap();
    assert_eq!(restored.snapshot(), media.snapshot());
}

#[shoop_test]
fn blocked_snapshots_reject_misaligned_tables_and_incomplete_free_blocks() {
    assert!(matches!(
        MediaStore::with_block_chunks(4, 3),
        Err(Error::InvalidShape)
    ));
    let mut media = MediaStore::with_block_chunks(4, 32).unwrap();
    media
        .insert(&samples(&(0..129).collect::<Vec<_>>()))
        .unwrap();
    let image = media.snapshot();
    let mut invalid = image.clone();
    invalid.views[0].as_mut().unwrap().chunks[1] = ChunkId(33);
    assert!(matches!(
        MediaStore::from_snapshot(invalid),
        Err(Error::InvalidState)
    ));
    let mut invalid = image.clone();
    invalid.views.push(image.views[0].clone());
    assert!(matches!(
        MediaStore::from_snapshot(invalid),
        Err(Error::InvalidState)
    ));
    let mut invalid = image;
    invalid.free_chunks = (0..32).map(ChunkId).collect();
    assert!(matches!(
        MediaStore::from_snapshot(invalid),
        Err(Error::InvalidState)
    ));
    media.clear(ViewId(0)).unwrap();
    let image = media.snapshot();
    let mut invalid = image.clone();
    invalid.free_chunks.remove(0);
    assert!(matches!(
        MediaStore::from_snapshot(invalid),
        Err(Error::InvalidState)
    ));
    let mut invalid = image;
    invalid.free_chunks.drain(..32);
    assert!(matches!(
        MediaStore::from_snapshot(invalid),
        Err(Error::InvalidState)
    ));
}
