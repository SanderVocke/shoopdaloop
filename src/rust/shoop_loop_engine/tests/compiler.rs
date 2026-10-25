#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;

use shoop_loop_engine::*;
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn primitive(slot: u32, length: u64) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind: LoopKind::Primitive,
        length,
        sync_source: None,
    }
}
fn composite(slot: u32) -> LoopSpec {
    LoopSpec {
        kind: LoopKind::Composite,
        ..primitive(slot, 0)
    }
}
fn entry(
    target: u32,
    delay: u64,
    duration: Option<u64>,
    repetitions: u32,
    mode: Option<Mode>,
) -> Entry {
    Entry {
        target: id(target),
        delay,
        duration,
        repetitions,
        mode,
    }
}
fn program(source: u32, sections: Vec<Vec<Entry>>) -> ProgramDescription {
    ProgramDescription {
        source: id(source),
        nominal_unit_frames: 4,
        sections: sections
            .into_iter()
            .map(|entries| Section { entries })
            .collect(),
    }
}
fn description(loops: Vec<LoopSpec>, programs: Vec<ProgramDescription>) -> TopologyDescription {
    TopologyDescription {
        loops,
        programs,
        triggers: Vec::new(),
    }
}

#[shoop_test]
fn distinct_checked_identities_and_generation_exhaustion() {
    assert_eq!(LoopId::from_parts(0, 0), Err(Error::InvalidId));
    assert_eq!(RequestId::from_parts(0, 0), Err(Error::InvalidId));
    let next = assert_no_heap(|| id(7).checked_next_generation()).unwrap();
    assert_eq!((next.slot(), next.generation()), (7, 2));
    assert_eq!(
        LoopId::from_parts(7, u32::MAX)
            .unwrap()
            .checked_next_generation(),
        Err(Error::Exhausted)
    );
}

#[shoop_test]
fn capacities_are_checked_without_allocation_and_journal_can_be_disabled() {
    assert_eq!(
        assert_no_heap(|| Limits {
            loop_slots: 0,
            ..Limits::default()
        }
        .validate()),
        Err(Error::InvalidConfig)
    );
    assert_eq!(
        assert_no_heap(|| Limits {
            program_segments: usize::MAX,
            ..Limits::default()
        }
        .validate()),
        Err(Error::Capacity)
    );
    assert_eq!(
        assert_no_heap(|| Limits {
            loop_slots: i32::MAX as usize,
            ..Limits::default()
        }
        .validate()),
        Err(Error::Capacity)
    );
    assert!(assert_no_heap(|| Limits {
        journal_events: 0,
        ..Limits::default()
    }
    .validate())
    .is_ok());
}

#[shoop_test]
fn empty_topology_and_zero_length_primitive_are_preparable() {
    let empty = compile(Limits::default(), &TopologyDescription::default()).unwrap();
    assert!(empty.nodes().is_empty());
    assert!(empty.components().is_empty());
    let single = compile(
        Limits::default(),
        &description(vec![primitive(0, 0)], vec![]),
    )
    .unwrap();
    assert_eq!(single.node(id(0)).unwrap().length, 0);
}

#[shoop_test]
fn missing_stale_duplicate_and_out_of_capacity_identities_are_rejected() {
    let mut input = description(vec![primitive(0, 4), primitive(1, 4)], vec![]);
    input.loops[1].sync_source = Some(LoopId::from_parts(0, 2).unwrap());
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::InvalidId
    );
    input.loops[1] = LoopSpec {
        id: LoopId::from_parts(0, 2).unwrap(),
        ..primitive(1, 4)
    };
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::DuplicateId
    );
    input.loops[1] = primitive(64, 4);
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::Capacity
    );
}

#[shoop_test]
fn mutual_sync_is_one_component_but_state_changing_feedback_is_invalid() {
    let mut input = description(vec![primitive(0, 4), primitive(1, 7)], vec![]);
    input.loops[0].sync_source = Some(id(1));
    input.loops[1].sync_source = Some(id(0));
    let prepared = compile(Limits::default(), &input).unwrap();
    assert_eq!(prepared.components().len(), 1);
    assert_eq!(&*prepared.components()[0], &[id(0), id(1)]);
    input.triggers.push(Trigger {
        source: id(0),
        target: id(1),
        mode: Mode::Playing,
    });
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::InstantaneousDependencyCycle
    );
}

#[shoop_test]
fn self_sync_is_valid_self_action_is_not() {
    let mut input = description(vec![primitive(0, 4)], vec![]);
    input.loops[0].sync_source = Some(id(0));
    assert_eq!(
        compile(Limits::default(), &input)
            .unwrap()
            .components()
            .len(),
        1
    );
    input.triggers.push(Trigger {
        source: id(0),
        target: id(0),
        mode: Mode::Stopped,
    });
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::InstantaneousDependencyCycle
    );
}

#[shoop_test]
fn compiler_orders_action_sources_before_targets_independent_of_ids() {
    let mut input = description(
        vec![primitive(0, 4), primitive(1, 4), primitive(2, 4)],
        vec![],
    );
    input.triggers = vec![
        Trigger {
            source: id(2),
            target: id(1),
            mode: Mode::Playing,
        },
        Trigger {
            source: id(1),
            target: id(0),
            mode: Mode::Playing,
        },
    ];
    let expected = compile(Limits::default(), &input).unwrap();
    assert_eq!(
        expected
            .components()
            .iter()
            .map(|component| component[0])
            .collect::<Vec<_>>(),
        vec![id(2), id(1), id(0)]
    );
    input.loops.reverse();
    input.triggers.reverse();
    assert_eq!(compile(Limits::default(), &input).unwrap(), expected);
}

#[shoop_test]
fn deep_finite_action_chain_has_no_wave_limit_or_recursive_stack_walk() {
    let count = 2048;
    let input = TopologyDescription {
        loops: (0..count).map(|slot| primitive(slot, 1)).collect(),
        triggers: (1..count)
            .map(|slot| Trigger {
                source: id(slot - 1),
                target: id(slot),
                mode: Mode::Playing,
            })
            .collect(),
        programs: vec![],
    };
    let limits = Limits {
        loop_slots: count as usize,
        relations: count as usize,
        ..Limits::default()
    };
    let prepared = compile(limits, &input).unwrap();
    assert_eq!(prepared.components().len(), count as usize);
    assert_eq!(prepared.components().last().unwrap()[0], id(count - 1));
}

#[shoop_test]
fn sequential_parallel_delayed_and_repeated_entries_compile_sparsely() {
    let input = description(
        vec![primitive(0, 5), primitive(1, 4), composite(2)],
        vec![program(
            2,
            vec![
                vec![entry(0, 0, None, 2, None), entry(1, 1, Some(1), 1, None)],
                vec![entry(1, 2, Some(3), 1, None)],
            ],
        )],
    );
    let prepared = compile(Limits::default(), &input).unwrap();
    let p = prepared.program(id(2)).unwrap();
    assert_eq!(p.kind, ProgramKind::Regular);
    assert_eq!(p.iterations, 9);
    assert_eq!(p.nominal_frames().unwrap(), 36);
    assert_eq!(p.segment_count(), 3);
    let a = p.target(id(0)).unwrap();
    assert_eq!(a.at(0).unwrap().end, 4);
    assert!(a.at(4).is_none());
    let b = p.target(id(1)).unwrap();
    assert!(b.at(0).is_none());
    assert_eq!(b.at(1).unwrap().end, 2);
    assert!(b.at(5).is_none());
    assert_eq!(b.at(6).unwrap().end, 9);
    assert!(b.at(9).is_none());
}

#[shoop_test]
fn sparse_duration_does_not_require_per_tick_storage_and_lookup_is_heap_free() {
    let input = description(
        vec![primitive(0, 4), composite(1)],
        vec![program(
            1,
            vec![vec![entry(0, 0, Some(1_000_000_000), 1, None)]],
        )],
    );
    let prepared = compile(
        Limits {
            program_segments: 1,
            ..Limits::default()
        },
        &input,
    )
    .unwrap();
    let p = prepared.program(id(1)).unwrap();
    assert_eq!(p.segment_count(), 1);
    assert_eq!(p.nominal_frames().unwrap(), 4_000_000_000);
    assert_no_heap(|| {
        for tick in [0, 1, 17, 999_999_999] {
            assert_eq!(
                p.target(id(0)).unwrap().at(tick).unwrap().mode,
                Mode::Playing
            );
        }
        assert!(p.target(id(0)).unwrap().at(1_000_000_000).is_none());
        assert_eq!(prepared.node(id(0)).unwrap().length, 4);
    });
}

#[shoop_test]
fn overlapping_explicit_entries_use_stable_ordinal_and_half_open_ranges() {
    let input = description(
        vec![primitive(0, 4), composite(1)],
        vec![program(
            1,
            vec![vec![
                entry(0, 0, Some(5), 1, Some(Mode::Playing)),
                entry(0, 2, Some(1), 1, Some(Mode::Stopped)),
            ]],
        )],
    );
    let prepared = compile(Limits::default(), &input).unwrap();
    let p = prepared.program(id(1)).unwrap();
    assert_eq!(p.kind, ProgramKind::Explicit);
    let target = p.target(id(0)).unwrap();
    assert_eq!(target.at(1).unwrap().mode, Mode::Playing);
    assert_eq!(target.at(2).unwrap().mode, Mode::Stopped);
    assert_eq!(target.at(3).unwrap().mode, Mode::Playing);
    assert_eq!(target.at(3).unwrap().occurrence_start, 3);
    assert!(target.at(5).is_none());
}

#[shoop_test]
fn nested_program_duration_and_order_are_derived_child_first() {
    let mut input = description(
        vec![primitive(0, 8), composite(1), composite(2)],
        vec![
            program(2, vec![vec![entry(1, 0, None, 1, None)]]),
            program(1, vec![vec![entry(0, 0, Some(3), 1, None)]]),
        ],
    );
    let prepared = compile(Limits::default(), &input).unwrap();
    assert_eq!(
        prepared.program(id(1)).unwrap().nominal_frames().unwrap(),
        12
    );
    assert_eq!(
        prepared.program(id(2)).unwrap().nominal_frames().unwrap(),
        12
    );
    assert_eq!(
        prepared
            .components()
            .iter()
            .map(|component| component[0])
            .collect::<Vec<_>>(),
        vec![id(2), id(1), id(0)]
    );
    input.loops.reverse();
    input.programs.reverse();
    assert_eq!(compile(Limits::default(), &input).unwrap(), prepared);
}

#[shoop_test]
fn empty_program_and_empty_child_have_distinct_durations() {
    let input = description(
        vec![primitive(0, 0), composite(1), composite(2)],
        vec![
            program(1, vec![]),
            program(2, vec![vec![entry(0, 0, None, 1, None)]]),
        ],
    );
    let prepared = compile(Limits::default(), &input).unwrap();
    assert_eq!(prepared.program(id(1)).unwrap().iterations, 0);
    assert_eq!(prepared.program(id(2)).unwrap().iterations, 1);
}

#[shoop_test]
fn recursive_composites_and_combined_sync_action_cycles_are_rejected() {
    let input = description(
        vec![composite(0), composite(1)],
        vec![
            program(0, vec![vec![entry(1, 0, Some(1), 1, None)]]),
            program(1, vec![vec![entry(0, 0, Some(1), 1, None)]]),
        ],
    );
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::RecursiveProgram
    );
    let mut input = description(
        vec![primitive(0, 4), composite(1)],
        vec![program(1, vec![vec![entry(0, 0, Some(1), 1, None)]])],
    );
    input.loops[1].sync_source = Some(id(0));
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::InstantaneousDependencyCycle
    );
}

#[shoop_test]
fn storage_refusal_checks_total_entries_segments_and_relations() {
    let input = description(
        vec![primitive(0, 4), primitive(1, 4), composite(2)],
        vec![program(
            2,
            vec![vec![
                entry(0, 0, Some(1), 1, None),
                entry(1, 0, Some(1), 1, None),
            ]],
        )],
    );
    for limits in [
        Limits {
            program_entries: 1,
            ..Limits::default()
        },
        Limits {
            program_segments: 1,
            ..Limits::default()
        },
        Limits {
            relations: 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(compile(limits, &input).unwrap_err(), Error::Capacity);
    }
    let mut input = input;
    input.programs[0].sections[0].entries[0].repetitions = u32::MAX;
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::Capacity
    );
}

#[shoop_test]
fn invalid_program_modes_durations_units_and_arithmetic_are_explicit() {
    let base = description(
        vec![primitive(0, 4), composite(1)],
        vec![program(1, vec![vec![entry(0, 0, Some(1), 1, None)]])],
    );
    let mut input = base.clone();
    input.programs[0].sections[0].entries[0].duration = Some(0);
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::InvalidProgram
    );
    input = base.clone();
    input.programs[0].nominal_unit_frames = 0;
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::InvalidProgram
    );
    input = base.clone();
    input.programs[0].sections[0]
        .entries
        .push(entry(0, 0, Some(1), 1, Some(Mode::Recording)));
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::InvalidProgram
    );
    input = base;
    input.programs[0].sections[0].entries[0].duration = Some(u64::MAX);
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::Exhausted
    );
}

#[shoop_test]
fn heap_guards_are_live_in_optimized_and_wasm_tests() {
    let (bytes, counts) =
        support::measure(|| vec![std::hint::black_box(7u8); std::hint::black_box(32)]);
    assert!(counts.allocations > 0);
    let (_, counts) = support::measure(|| drop(bytes));
    assert!(counts.deallocations > 0);
}

fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[shoop_test]
fn seeded_components_match_an_independent_reachability_reference() {
    let mut seed = 0x18ad_5c74_d032_fbb1;
    let mut accepted = 0;
    let mut rejected = 0;
    for _ in 0..4096 {
        let n = (random(&mut seed) % 6 + 1) as usize;
        let mut input = description(
            (0..n as u32).map(|slot| primitive(slot, 4)).collect(),
            vec![],
        );
        let mut reachable = vec![vec![false; n]; n];
        for target in 0..n {
            reachable[target][target] = true;
            if random(&mut seed) % 4 == 0 {
                let source = (random(&mut seed) % n as u64) as usize;
                input.loops[target].sync_source = Some(id(source as u32));
                reachable[source][target] = true;
            }
        }
        for (source, row) in reachable.iter_mut().enumerate() {
            for (target, edge) in row.iter_mut().enumerate() {
                if random(&mut seed) % 11 == 0 {
                    input.triggers.push(Trigger {
                        source: id(source as u32),
                        target: id(target as u32),
                        mode: Mode::Playing,
                    });
                    *edge = true;
                }
            }
        }
        for via in 0..n {
            for from in 0..n {
                for to in 0..n {
                    reachable[from][to] |= reachable[from][via] && reachable[via][to];
                }
            }
        }
        let invalid = input
            .triggers
            .iter()
            .any(|edge| reachable[edge.target.slot() as usize][edge.source.slot() as usize]);
        let result = compile(Limits::default(), &input);
        if invalid {
            assert_eq!(result.unwrap_err(), Error::InstantaneousDependencyCycle);
            rejected += 1;
        } else {
            accepted += 1;
            let prepared = result.unwrap();
            let mut group = vec![usize::MAX; n];
            for (index, members) in prepared.components().iter().enumerate() {
                for member in members.iter() {
                    group[member.slot() as usize] = index;
                }
            }
            for from in 0..n {
                for to in 0..n {
                    assert_eq!(
                        group[from] == group[to],
                        reachable[from][to] && reachable[to][from]
                    );
                    if reachable[from][to] {
                        assert!(group[from] <= group[to]);
                    }
                }
            }
            input.loops.reverse();
            input.triggers.reverse();
            assert_eq!(compile(Limits::default(), &input).unwrap(), prepared);
        }
    }
    assert!(accepted > 500 && rejected > 500);
}

#[shoop_test]
fn seeded_sparse_program_lookup_matches_a_dense_scalar_table() {
    let mut seed = 0x84a7_31c9_034e_09db;
    for _ in 0..1024 {
        let mut sections = Vec::new();
        let mut expected: Vec<[Option<Mode>; 3]> = Vec::new();
        let mut base = 0usize;
        for _ in 0..(random(&mut seed) % 4 + 1) {
            let mut entries = Vec::new();
            let mut section_end = base;
            for _ in 0..(random(&mut seed) % 4 + 1) {
                let target = (random(&mut seed) % 3) as usize;
                let delay = (random(&mut seed) % 5) as usize;
                let duration = (random(&mut seed) % 5 + 1) as usize;
                let repeats = (random(&mut seed) % 3 + 1) as usize;
                let mode = [Mode::Playing, Mode::Stopped, Mode::Recording]
                    [(random(&mut seed) % 3) as usize];
                let end = base + delay + duration * repeats;
                if expected.len() < end {
                    expected.resize(end, [None; 3]);
                }
                for sample in &mut expected[base + delay..end] {
                    sample[target] = Some(mode);
                }
                section_end = section_end.max(end);
                entries.push(entry(
                    target as u32,
                    delay as u64,
                    Some(duration as u64),
                    repeats as u32,
                    Some(mode),
                ));
            }
            sections.push(entries);
            base = section_end;
        }
        let input = description(
            vec![
                primitive(0, 4),
                primitive(1, 4),
                primitive(2, 4),
                composite(3),
            ],
            vec![program(3, sections)],
        );
        let prepared = compile(Limits::default(), &input).unwrap();
        let program = prepared.program(id(3)).unwrap();
        assert_eq!(program.iterations, expected.len() as u64);
        for (iteration, row) in expected.iter().enumerate() {
            for (target, &mode) in row.iter().enumerate() {
                assert_eq!(
                    program
                        .target(id(target as u32))
                        .and_then(|timeline| timeline.at(iteration as u64))
                        .map(|segment| segment.mode),
                    mode
                );
            }
        }
    }
}
