#[path = "support/composite.rs"]
mod composite;
#[path = "support/mono.rs"]
mod mono;
use rt_fpga_compiler::{compile, CompileContext};
use rt_fpga_compiler::{loading::*, *};
use shoop_wasm_test_support::shoop_test;

fn graph(ids: &[u32]) -> LogicalStateProgram {
    compile(
        &mono::session(
            &(ids
                .iter()
                .map(|&slot| mono::MonoLoop {
                    id: Identity {
                        slot,
                        generation: 1,
                    },
                    source: None,
                    input: true,
                    output: true,
                    gain: 1.0,
                    repeat_sync: false,
                })
                .collect::<Vec<_>>()),
        ),
        CompileContext {
            quantum: 16,
            limits: Default::default(),
        },
    )
    .unwrap()
}
fn limits() -> ProgramLimits {
    ProgramLimits {
        loop_slots: 4,
        state_values: 8,
        history_slots: 32,
        ..Default::default()
    }
}

#[shoop_test]
fn carry_over_has_no_initializers_and_old_slots_are_pinned_until_handoff() {
    let mut allocator = Allocator::new(limits());
    let a = allocator.stage(&graph(&[0, 1])).unwrap();
    assert_eq!(a.new_values().count(), 4);
    allocator.publish(a.revision).unwrap();
    allocator.complete(a.revision).unwrap();
    let b = allocator.stage(&graph(&[1, 2])).unwrap();
    let one = Identity {
        slot: 1,
        generation: 1,
    };
    assert_eq!(a.loop_id(one), b.loop_id(one));
    assert_eq!(
        b.state_address(0),
        a.state_address(graph(&[0, 1]).bindings[1].gain.unwrap())
    );
    assert_eq!(b.new_values().count(), 2);
    assert_eq!(b.new_loops().count(), 1);
    assert_ne!(
        b.loop_id(Identity {
            slot: 2,
            generation: 1
        })
        .unwrap()
        .slot,
        a.loop_id(Identity {
            slot: 0,
            generation: 1
        })
        .unwrap()
        .slot
    );
    assert_eq!(allocator.stage(&graph(&[3])).unwrap_err(), LoadError::Busy);
    assert_eq!(
        allocator.complete(a.revision),
        Err(LoadError::StaleRevision)
    );
    assert_eq!(allocator.current().unwrap().revision, a.revision);
    allocator.publish(b.revision).unwrap();
    allocator.complete(b.revision).unwrap();
    let c = allocator.stage(&graph(&[1, 3])).unwrap();
    let recycled = c
        .loop_id(Identity {
            slot: 3,
            generation: 1,
        })
        .unwrap();
    assert_eq!(
        recycled.slot,
        a.loop_id(Identity {
            slot: 0,
            generation: 1
        })
        .unwrap()
        .slot
    );
    assert!(
        recycled.generation
            > a.loop_id(Identity {
                slot: 0,
                generation: 1
            })
            .unwrap()
            .generation
    );
    assert_eq!((a.region, b.region, c.region), (0, 1, 0));
}

#[shoop_test]
fn failed_admission_and_cancel_preserve_current_and_relocation_is_sparse() {
    let mut allocator = Allocator::new(ProgramLimits {
        loop_slots: 3,
        state_values: 6,
        ..limits()
    });
    let a = allocator.stage(&graph(&[0, 1])).unwrap();
    allocator.publish(a.revision).unwrap();
    allocator.complete(a.revision).unwrap();
    assert!(matches!(
        allocator.stage(&graph(&[2, 3])),
        Err(LoadError::Compile(_))
    ));
    assert!(allocator.pending().is_none());
    let b = allocator.stage(&graph(&[1, 2])).unwrap();
    let bound = b.relocate(&graph(&[1, 2])).unwrap();
    bound
        .validate(ProgramLimits {
            loop_slots: 3,
            state_values: 6,
            ..limits()
        })
        .unwrap();
    assert_eq!(
        bound.bindings[0].gain,
        Some(a.state_address(graph(&[0, 1]).bindings[1].gain.unwrap()))
    );
    allocator.cancel(b.revision).unwrap();
    assert_eq!(allocator.revision(), a.revision);
    let c = allocator.stage(&graph(&[1, 2])).unwrap();
    assert!(c.revision > b.revision);
    assert!(c.loops[1].physical.generation > b.loops[1].physical.generation);
    allocator.cancel(c.revision).unwrap();
}

#[shoop_test]
fn changing_sync_operands_keeps_addresses_and_requires_explicit_plan_clear() {
    let mut allocator = Allocator::new(limits());
    let first = graph(&[0, 1]);
    let a = allocator.stage(&first).unwrap();
    allocator.publish(a.revision).unwrap();
    allocator.complete(a.revision).unwrap();
    let mut next = first.clone();
    next.loops[1].sync_source = Some(next.loops[0].id);
    let b = allocator.stage(&next).unwrap();
    assert_eq!(b.changed_sources(), &[next.loops[1].id]);
    assert_eq!(b.new_values().count(), 0);
    assert_eq!(b.new_loops().count(), 0);
    assert_eq!(b.loop_id(next.loops[1].id), a.loop_id(next.loops[1].id));
    let bound = b.relocate(&next).unwrap();
    assert_eq!(bound.loops[1].sync_source, Some(bound.loops[0].id));
}

#[shoop_test]
fn logical_ids_do_not_consume_physical_slots_and_failed_staging_keeps_ownership() {
    for (resource, pool) in [
        (
            Resource::Loops,
            ProgramLimits {
                loop_slots: 3,
                state_values: 8,
                ..limits()
            },
        ),
        (
            Resource::State,
            ProgramLimits {
                loop_slots: 4,
                state_values: 6,
                ..limits()
            },
        ),
    ] {
        let mut allocator = Allocator::new(pool);
        let first = graph(&[1000, 2000]);
        first.validate_logical(pool).unwrap();
        let a = allocator.stage(&first).unwrap();
        allocator.publish(a.revision).unwrap();
        allocator.complete(a.revision).unwrap();
        let replacement = graph(&[3000, 4000]);
        replacement.validate_logical(pool).unwrap();
        assert_eq!(
            allocator.stage(&replacement).unwrap_err(),
            LoadError::Compile(CompileError::Capacity(resource))
        );
        assert!(allocator.pending().is_none());
        assert_eq!(allocator.current().unwrap().revision, a.revision);
        let retained = allocator.stage(&first).unwrap();
        assert_eq!(retained.new_values().count(), 0);
        assert_eq!(retained.new_loops().count(), 0);
        allocator.cancel(retained.revision).unwrap();
        let smaller = allocator.stage(&graph(&[3000])).unwrap();
        allocator.publish(smaller.revision).unwrap();
        allocator.complete(smaller.revision).unwrap();
        allocator.stage(&replacement).unwrap();
    }
}

#[shoop_test]
fn staging_requires_explicit_state_ownership_without_inference() {
    let mut allocator = Allocator::new(limits());
    let mut invalid = graph(&[0]);
    invalid.state_bindings.clear();
    assert_eq!(
        allocator.stage(&invalid).unwrap_err(),
        LoadError::Compile(CompileError::InvalidProgram)
    );
    assert!(allocator.pending().is_none());
    let valid = allocator.stage(&graph(&[0])).unwrap();
    assert_eq!(valid.loops[0].physical.generation, 1);
}

#[shoop_test]
fn a_dropped_child_at_wrap_uses_one_optional_instruction_row() {
    let mut graph = composite::arbitration_graph();
    let context = CompileContext {
        quantum: 16,
        limits: ProgramLimits {
            loop_slots: 8,
            ..limits()
        },
    };
    let before = compile_composites(&graph, context, 16).unwrap();
    let replacement = graph.loops[0].id;
    let edited = graph.loops[1].composite.as_mut().unwrap();
    edited.version = 2;
    let dropped = edited.spans[0].target;
    edited.spans.to_mut()[0].target = replacement;
    let after = compile_composites(&graph, context, 16).unwrap();
    let used = (1..64)
        .find(|&instructions| {
            after
                .validate_logical(ProgramLimits {
                    instructions,
                    ..context.limits
                })
                .is_ok()
        })
        .unwrap();

    for extra in [0, 1] {
        let capacity = ProgramLimits {
            instructions: used + extra,
            ..context.limits
        };
        let mut allocator = Allocator::new(capacity);
        let first = allocator.stage(&before).unwrap();
        allocator.publish(first.revision).unwrap();
        allocator.complete(first.revision).unwrap();
        let next = allocator.stage(&after).unwrap();
        assert_eq!(next.composite_cuts()[0].allow_restart, extra == 1);
        assert_eq!(next.composite_cuts()[0].reconcile_ending, extra == 1);
        let bound = next.relocate(&after).unwrap();
        bound.validate(capacity).unwrap();
        let child = next.loop_id(dropped).unwrap();
        assert_eq!(
            bound
                .composite_parents(child)
                .filter(|(_, child)| child.spans.is_empty())
                .count(),
            extra as usize
        );
    }
}
