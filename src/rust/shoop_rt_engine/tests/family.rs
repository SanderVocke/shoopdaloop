mod support;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn upload(h: &mut Harness, id: u64, frames: u64, value: f32) {
    let id = id * 2 - 1;
    let bytes = (0..frames)
        .flat_map(|_| [value, 0.0])
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    h.done(Request::ContentBegin(ContentDescription {
        id,
        midi: false,
        frames,
        bytes: bytes.len() as u64,
    }));
    for (index, bytes) in bytes.chunks(256).enumerate() {
        h.done(Request::ContentWrite {
            id,
            offset: (index * 256) as u64,
            data: ContentChunk::new(bytes).unwrap(),
        });
    }
    h.done(Request::ContentFinish { id });
    h.done(Request::ContentBegin(ContentDescription {
        id: id + 1,
        midi: true,
        frames,
        bytes: 0,
    }));
    h.done(Request::ContentFinish { id: id + 1 });
}
fn prepare(
    h: &mut Harness,
    family: u64,
    target: LoopSlotId,
    source: LoopSlotId,
    old_frames: u64,
    revision: u64,
    frames: u64,
    audio: u64,
) {
    let outcome = h.apply(Request::ContentFamilyEdit {
        id: family,
        edit: ContentFamilyEdit {
            loop_id: target,
            sync_source: source,
            expected_revision: revision,
            expected_frames: old_frames,
            frames,
            audio: Some(audio * 2 - 1),
            midi: Some(audio * 2),
            replace: true,
            restart: target == source,
            fence_content: true,
        },
    });
    assert!(
        matches!(outcome, Outcome::ContentPrepared { revision: next } if next > revision),
        "{outcome:?}"
    );
}

fn baseline() -> Harness {
    let mut description = description();
    description.quantum = 32;
    description.max_frames = 16;
    description.ramp_frames = 0;
    description.smoothing_frames = 0;
    description.resource_pages = 64;
    let mut h = Harness::new(&description);
    let strip = strip(0, 1);
    let root = loop_id(0, 1);
    let child = loop_id(1, 1);
    h.done(Request::CreateStrip {
        strip,
        shape: shape(1, 1, true),
    });
    for loop_id in [root, child] {
        h.done(Request::CreateLoop { loop_id, strip });
    }
    h.done(Request::LoopSync {
        loop_id: child,
        source: Some(root),
    });
    h.done(route(strip, 0, 0, true));
    for (token, loop_id, frames, value) in [(1, root, 96, 1.0), (2, child, 384, 2.0)] {
        upload(&mut h, token, frames, value);
        h.done(Request::ContentApply {
            loop_id,
            frames,
            audio: Some(token * 2 - 1),
            midi: Some(token * 2),
        });
        h.done(Request::LoopTransition {
            loop_id,
            mode: LoopMode::Playing,
            delay: None,
        });
    }
    h.run(64, 16);
    assert_eq!(h.outputs[0].last(), Some(&3.0));
    h
}

#[shoop_test]
fn cancelled_and_incomplete_families_reclaim_staged_media_without_partial_changes() {
    let mut h = baseline();
    let root = loop_id(0, 1);
    let child = loop_id(1, 1);
    for round in 0..16 {
        h.done(Request::ContentFamilyBegin {
            id: round + 1,
            root,
            count: 2,
            sample_rate: description().sample_rate,
            quantum: 32,
        });
        upload(&mut h, 3 + round, 128, 0.25);
        prepare(&mut h, round + 1, root, root, 96, 1, 128, 3 + round);
        h.done(Request::ContentFamilyCancel { id: round + 1 });
        h.run(64, 16);
        assert_eq!(h.outputs[0].last(), Some(&3.0));
    }
    h.done(Request::ContentFamilyBegin {
        id: 17,
        root,
        count: 1,
        sample_rate: description().sample_rate,
        quantum: 32,
    });
    upload(&mut h, 19, 128, 0.25);
    prepare(&mut h, 17, root, root, 96, 1, 128, 19);
    assert!(matches!(
        h.apply(Request::ContentFamilyCommit { id: 17 }),
        Outcome::Refused(_)
    ));
    h.run(64, 16);
    assert_eq!(h.outputs[0].last(), Some(&3.0));
    assert!(matches!(
        h.last_loop(child),
        Some((
            _,
            Telemetry::Loop {
                length: 384,
                content_revision: 1,
                ..
            }
        ))
    ));
}

#[shoop_test]
fn controller_loss_discards_preparation_and_preserves_an_admitted_family_commit() {
    for committed in [false, true] {
        let mut h = baseline();
        let root = loop_id(0, 1);
        let child = loop_id(1, 1);
        h.done(Request::ContentFamilyBegin {
            id: 1,
            root,
            count: 2,
            sample_rate: description().sample_rate,
            quantum: 32,
        });
        upload(&mut h, 3, 128, 0.25);
        prepare(&mut h, 1, root, root, 96, 1, 128, 3);
        upload(&mut h, 4, 512, 0.5);
        prepare(&mut h, 1, child, root, 384, 1, 512, 4);
        if committed {
            h.send(Request::ContentFamilyCommit { id: 1 });
        }
        assert_no_heap(|| h.host.disconnect());
        h.pump();
        h.settle();
        assert_no_heap(|| h.host.connect()).unwrap();
        h.pump();
        h.settle();
        h.run(128, 16);
        assert_eq!(
            h.outputs[0].last(),
            Some(&if committed { 0.75 } else { 3.0 })
        );
        h.done(Request::ContentFamilyBegin {
            id: 2,
            root,
            count: 1,
            sample_rate: description().sample_rate,
            quantum: 32,
        });
        h.done(Request::ContentFamilyCancel { id: 2 });
    }
}

#[shoop_test]
fn family_prepares_without_changing_music_and_publishes_one_cut_beside_recording() {
    for quantum in [16, 32, 64] {
        let mut description = description();
        description.quantum = quantum;
        description.max_frames = 16;
        description.ramp_frames = 0;
        description.smoothing_frames = 0;
        let mut h = Harness::new(&description);
        let q = quantum as u64;
        let strip = strip(0, 1);
        let root = loop_id(0, 1);
        let child = loop_id(1, 1);
        let unrelated = loop_id(2, 1);
        h.done(Request::CreateStrip {
            strip,
            shape: shape(1, 1, true),
        });
        for loop_id in [root, child, unrelated] {
            h.done(Request::CreateLoop { loop_id, strip });
        }
        h.done(Request::LoopSync {
            loop_id: child,
            source: Some(root),
        });
        h.done(route(strip, 0, 0, true));
        upload(&mut h, 1, 3 * q, 1.0);
        h.done(Request::ContentApply {
            loop_id: root,
            frames: 3 * q,
            audio: Some(1),
            midi: Some(2),
        });
        upload(&mut h, 2, 12 * q, 2.0);
        h.done(Request::ContentApply {
            loop_id: child,
            frames: 12 * q,
            audio: Some(3),
            midi: Some(4),
        });
        for loop_id in [root, child] {
            h.done(Request::LoopTransition {
                loop_id,
                mode: LoopMode::Playing,
                delay: None,
            });
        }
        h.run(2 * quantum, 16);
        assert_eq!(h.outputs[0].last(), Some(&3.0));
        h.done(Request::ContentFamilyBegin {
            id: 1,
            root,
            count: 2,
            sample_rate: description.sample_rate,
            quantum: quantum as u32,
        });
        upload(&mut h, 3, 4 * q, 0.25);
        prepare(&mut h, 1, root, root, 3 * q, 1, 4 * q, 3);
        upload(&mut h, 4, 16 * q, 0.5);
        prepare(&mut h, 1, child, root, 12 * q, 1, 16 * q, 4);
        assert_eq!(h.outputs[0].last(), Some(&3.0));
        h.done(Request::LoopGain {
            loop_id: root,
            gain: 0.75,
        });
        h.done(Request::LoopTransition {
            loop_id: unrelated,
            mode: LoopMode::Recording,
            delay: None,
        });
        h.run(2 * quantum, 16);
        let first = h.outputs[0].len();
        h.done(Request::ContentFamilyCommit { id: 1 });
        h.run(2 * quantum, 16);
        assert!(
            h.outputs[0][first..]
                .iter()
                .all(|sample| *sample == 2.75 || *sample == 0.6875),
            "partial family at Q={quantum}"
        );
        assert_eq!(h.outputs[0].last(), Some(&0.6875));
        assert!(matches!(
            h.last_loop(unrelated),
            Some((
                _,
                Telemetry::Loop {
                    mode: LoopMode::Recording,
                    ..
                }
            ))
        ));
        assert!(
            matches!(h.last_loop(root), Some((_, Telemetry::Loop { length, content_revision: 2, .. })) if length == 4 * q)
        );
        assert!(
            matches!(h.last_loop(child), Some((_, Telemetry::Loop { length, content_revision: 2, .. })) if length == 16 * q)
        );
        h.done(Request::LoopTransition {
            loop_id: unrelated,
            mode: LoopMode::Stopped,
            delay: None,
        });

        h.done(Request::ContentFamilyBegin {
            id: 2,
            root,
            count: 2,
            sample_rate: description.sample_rate,
            quantum: quantum as u32,
        });
        upload(&mut h, 5, 5 * q, 0.125);
        prepare(&mut h, 2, root, root, 4 * q, 2, 5 * q, 5);
        upload(&mut h, 6, 20 * q, 0.75);
        prepare(&mut h, 2, child, root, 16 * q, 2, 20 * q, 6);
        upload(&mut h, 7, 4 * q, 0.375);
        h.done(Request::ContentApply {
            loop_id: root,
            frames: 4 * q,
            audio: Some(13),
            midi: Some(14),
        });
        h.run(2 * quantum, 16);
        let first = h.outputs[0].len();
        assert_eq!(
            h.apply(Request::ContentFamilyCommit { id: 2 }),
            Outcome::Refused(Refusal::Busy)
        );
        h.run(2 * quantum, 16);
        assert!(h.outputs[0][first..]
            .iter()
            .all(|sample| *sample == 0.78125));
        assert!(
            matches!(h.last_loop(child), Some((_, Telemetry::Loop { length, content_revision: 2, .. })) if length == 16 * q)
        );
        h.done(Request::ContentFamilyBegin {
            id: 3,
            root,
            count: 1,
            sample_rate: description.sample_rate,
            quantum: quantum as u32,
        });
        h.done(Request::ContentFamilyCancel { id: 3 });
    }
}
