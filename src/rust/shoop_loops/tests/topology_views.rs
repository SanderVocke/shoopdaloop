#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use shoop_loops::{program::Segment, *};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn node(slot: u32, kind: LoopKind, length: u64, sync: Option<u32>) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind,
        length,
        sync_source: sync.map(id),
    }
}
fn entry(target: u32, delay: u64, duration: u64, mode: Option<Mode>) -> Entry {
    Entry {
        target: id(target),
        delay,
        duration: Some(duration),
        repetitions: 1,
        mode,
    }
}
fn program(source: u32, entries: Vec<Entry>) -> ProgramDescription {
    ProgramDescription {
        source: id(source),
        nominal_unit_frames: 1,
        sections: vec![Section { entries }],
    }
}
fn segment(start: u64, end: u64, mode: Mode, ordinal: u32) -> Segment {
    Segment {
        start,
        end,
        mode,
        occurrence_start: start,
        ordinal,
    }
}

#[shoop_test]
fn borrowed_views_match_independent_sparse_segments_and_component_order() {
    let mut input = TopologyDescription {
        loops: vec![
            node(0, LoopKind::Primitive, 1, None),
            node(1, LoopKind::Primitive, 4, Some(2)),
            node(2, LoopKind::Primitive, 3, Some(1)),
            node(3, LoopKind::Composite, 0, Some(0)),
            node(4, LoopKind::Composite, 0, Some(0)),
        ],
        triggers: vec![],
        programs: vec![
            program(3, vec![entry(1, 2, 3, None)]),
            program(
                4,
                vec![
                    entry(1, 0, 6, Some(Mode::Playing)),
                    entry(1, 2, 2, Some(Mode::Recording)),
                    entry(2, 1, 4, Some(Mode::Stopped)),
                ],
            ),
        ],
    };
    let first = compile(Limits::default(), &input).unwrap();
    input.loops.reverse();
    input.programs.reverse();
    let reversed = compile(Limits::default(), &input).unwrap();
    assert_no_heap(|| {
        assert_eq!(first, reversed);
        let components = first.components();
        assert_eq!(components.len(), 4);
        assert!(!components.is_empty());
        assert_eq!(components.get(0), Some(&[id(0)][..]));
        assert_eq!(components.get(1), Some(&[id(3)][..]));
        assert_eq!(components.get(2), Some(&[id(4)][..]));
        assert_eq!(components.last(), Some(&[id(1), id(2)][..]));
        assert_eq!(components.get(4), None);
        assert_eq!(components.get(usize::MAX), None);
        let mut walk = components.into_iter();
        assert_eq!(walk.len(), 4);
        assert_eq!(walk.next(), Some(&[id(0)][..]));
        assert_eq!(walk.next_back(), Some(&[id(1), id(2)][..]));
        assert_eq!(walk.size_hint(), (2, Some(2)));
        assert_eq!(walk.clone().count(), 2);
        assert_eq!(walk.next(), Some(&[id(3)][..]));
        assert_eq!(walk.next_back(), Some(&[id(4)][..]));
        assert_eq!(walk.next(), None);
        assert_eq!(walk.next_back(), None);
        let regular = first.program(id(3)).unwrap();
        assert_eq!(regular.kind, ProgramKind::Regular);
        assert_eq!(regular.iterations, 5);
        assert_eq!(regular.nominal_frames(), Ok(5));
        assert_eq!(
            regular.target(id(1)).unwrap().segments(),
            &[segment(2, 5, Mode::Playing, 0)]
        );
        let explicit = first.program(id(4)).unwrap();
        assert_eq!(explicit.kind, ProgramKind::Explicit);
        assert_eq!(explicit.segment_count(), 4);
        let targets = explicit.targets();
        assert_eq!(targets.len(), 2);
        assert!(!targets.is_empty());
        let one = targets.get(0).unwrap();
        let two = targets.get(1).unwrap();
        assert_eq!(one.target, id(1));
        assert_eq!(two.target, id(2));
        let expected = [
            segment(0, 2, Mode::Playing, 0),
            segment(2, 4, Mode::Recording, 1),
            segment(4, 6, Mode::Playing, 0),
        ];
        assert_eq!(one.segments(), &expected);
        assert_eq!(two.segments(), &[segment(1, 5, Mode::Stopped, 2)]);
        for iteration in 0..8 {
            assert_eq!(
                one.at(iteration),
                expected
                    .iter()
                    .copied()
                    .find(|segment| segment.start <= iteration && iteration < segment.end)
            );
        }
        assert_eq!(one.at(u64::MAX), None);
        assert_eq!(explicit.target(id(0)), None);
        assert_eq!(targets.get(2), None);
        assert_eq!(targets.get(usize::MAX), None);
        let mut walk = targets.into_iter();
        assert_eq!(walk.size_hint(), (2, Some(2)));
        assert_eq!(walk.next_back(), Some(two));
        assert_eq!(walk.len(), 1);
        assert_eq!(walk.clone().count(), 1);
        assert_eq!(walk.next(), Some(one));
        assert_eq!(walk.next(), None);
        assert_eq!(walk.next_back(), None);
        assert_eq!(first.program(id(0)), None);
        assert_eq!(first.program(LoopId::from_parts(3, 2).unwrap()), None);
    });
    let empty = compile(Limits::default(), &TopologyDescription::default()).unwrap();
    assert_no_heap(|| {
        assert!(empty.components().is_empty());
        assert_eq!(empty.components().last(), None);
        assert_eq!(empty.components().iter().len(), 0);
        assert_eq!(empty.storage_bytes(), size_of::<PreparedTopology>());
    });
    let empty_program = compile(
        Limits::default(),
        &TopologyDescription {
            loops: vec![node(0, LoopKind::Composite, 0, None)],
            triggers: vec![],
            programs: vec![program(0, vec![])],
        },
    )
    .unwrap();
    assert_no_heap(|| {
        let program = empty_program.program(id(0)).unwrap();
        assert!(program.targets().is_empty());
        assert_eq!(program.targets().iter().len(), 0);
        assert_eq!(program.target(id(0)), None);
        assert_eq!(program.segment_count(), 0);
    });
}

fn submit(resolver: &mut Resolver, target: u32, sequence: u64, transition: Transition) {
    assert_no_heap(|| {
        resolver.submit(Request {
            target: id(target),
            sequence: Sequence(sequence),
            at: resolver.frame(),
            timing: Timing::At,
            transition,
        })
    })
    .unwrap();
}
fn step(resolver: &mut Resolver, end: u64) {
    assert_no_heap(|| {
        let interval = resolver.preview(Frame(end), &[]).unwrap();
        assert_eq!(interval.end, Frame(end));
        resolver.commit().unwrap();
        resolver.finish(Frame(end)).unwrap();
    });
}
#[shoop_test]
fn unrelated_table_relocation_preserves_running_program_identity_and_owning_returns() {
    let mut input = TopologyDescription {
        loops: vec![
            node(0, LoopKind::Primitive, 3, None),
            node(1, LoopKind::Primitive, 2, None),
            node(3, LoopKind::Composite, 0, Some(0)),
        ],
        triggers: vec![],
        programs: vec![program(3, vec![entry(1, 0, 3, None)])],
    };
    let original = compile(Limits::default(), &input).unwrap();
    input.loops.push(node(2, LoopKind::Composite, 0, Some(0)));
    input
        .programs
        .push(program(2, vec![entry(1, 2, 5, Some(Mode::Recording))]));
    let candidate = compile(Limits::default(), &input).unwrap();
    assert_no_heap(|| assert_eq!(original.program(id(3)), candidate.program(id(3))));
    let mut resolver = Resolver::prepare(original).unwrap();
    let play = Transition::Play {
        position: 0,
        retrigger: false,
    };
    submit(&mut resolver, 0, 1, play);
    submit(&mut resolver, 3, 2, play);
    step(&mut resolver, 1);
    let before = resolver.state(id(3)).unwrap();
    assert_eq!(before.mode, Mode::Playing);
    let retired = assert_no_heap(|| resolver.replace_topology(Epoch(0), candidate)).unwrap();
    assert_eq!(resolver.state(id(3)).unwrap(), before);
    assert_eq!(resolver.epoch(), Epoch(1));
    assert_eq!(retired.nodes().len(), 3);
    assert_eq!(resolver.topology().nodes().len(), 4);
    assert_eq!(retired.program(id(3)), resolver.topology().program(id(3)));
    drop(retired);
    input.programs[0].sections[0].entries[0].duration = Some(4);
    let candidate = compile(Limits::default(), &input).unwrap();
    let (error, candidate) =
        assert_no_heap(|| resolver.replace_topology(Epoch(1), candidate)).unwrap_err();
    assert_eq!(error, Error::InvalidState);
    assert_eq!(resolver.state(id(3)).unwrap(), before);
    assert_eq!(candidate.program(id(3)).unwrap().iterations, 4);
    let (error, candidate) =
        assert_no_heap(|| resolver.replace_topology(Epoch(0), candidate)).unwrap_err();
    assert_eq!(error, Error::InvalidState);
    assert_eq!(resolver.epoch(), Epoch(1));
    submit(&mut resolver, 3, 3, Transition::Stop);
    step(&mut resolver, 2);
    let retired = assert_no_heap(|| resolver.replace_topology(Epoch(1), candidate)).unwrap();
    assert_eq!(resolver.epoch(), Epoch(2));
    assert_eq!(resolver.state(id(3)).unwrap().content_length, 4);
    assert_eq!(
        resolver.state(id(3)).unwrap().composite,
        Some(CompositeState::default())
    );
    assert_eq!(retired.program(id(3)).unwrap().iterations, 3);
    assert_eq!(resolver.retained_requests(), 3);
    drop(retired);
}

#[shoop_test]
fn flat_storage_scales_with_total_metadata_not_program_products_or_duration() {
    let mut unit_bytes = None;
    let mut unit_bound = None;
    for count in [1, 2, 4, 8, 16, 32] {
        let limits = Limits {
            loop_slots: count * 2,
            relations: count,
            program_entries: count,
            program_segments: count,
            pending_requests: 1,
            journal_events: 0,
        };
        let mut input = TopologyDescription::default();
        for index in 0..count as u32 {
            input.loops.push(node(index, LoopKind::Primitive, 1, None));
            input
                .loops
                .push(node(index + count as u32, LoopKind::Composite, 0, None));
            input.programs.push(program(
                index + count as u32,
                vec![entry(index, 1_000_000_000, 1, None)],
            ));
        }
        let prepared = compile(limits, &input).unwrap();
        let bytes = assert_no_heap(|| prepared.storage_bytes() - size_of::<PreparedTopology>());
        let bound = assert_no_heap(|| {
            limits.topology_storage_bytes().unwrap() - size_of::<PreparedTopology>()
        });
        assert_eq!(bytes, *unit_bytes.get_or_insert(bytes) * count);
        assert_eq!(bound, *unit_bound.get_or_insert(bound) * count);
        assert!(bytes <= bound);
        for program in &mut input.programs {
            program.sections[0].entries[0].delay = u64::MAX - 1;
        }
        let longest = compile(limits, &input).unwrap();
        assert_no_heap(|| {
            assert_eq!(longest.storage_bytes(), prepared.storage_bytes());
            for index in 0..count as u32 {
                let program = longest.program(id(index + count as u32)).unwrap();
                assert_eq!(program.nominal_frames(), Ok(u64::MAX));
                let target = program.target(id(index)).unwrap();
                assert_eq!(
                    target.segments(),
                    &[segment(u64::MAX - 1, u64::MAX, Mode::Playing, 0)]
                );
                assert_eq!(target.at(u64::MAX), None);
                assert!(target.at(u64::MAX - 1).is_some());
            }
        });
    }
    let combined = Limits {
        program_entries: i32::MAX as usize / size_of::<Entry>(),
        program_segments: i32::MAX as usize / size_of::<Segment>(),
        ..Limits::default()
    };
    assert_no_heap(|| {
        assert_eq!(combined.topology_storage_bytes(), Err(Error::Capacity));
        assert_eq!(combined.validate(), Err(Error::Capacity));
        assert_eq!(
            Limits {
                loop_slots: usize::MAX,
                ..Limits::default()
            }
            .topology_storage_bytes(),
            Err(Error::Capacity)
        );
    });
}
