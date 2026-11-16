use rt_fpga_compiler::{graph::*, loading::*, *};
use shoop_wasm_test_support::shoop_test;

fn graph(ids: &[u32]) -> LogicalStateProgram {
    GraphDescription {
        quantum: 16,
        loops: ids
            .iter()
            .map(|&slot| LoopDescription {
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
            .collect(),
    }
    .compile()
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
    assert_eq!(b.state_address(0), a.state_address(1));
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
    assert_eq!(bound.bindings[0].gain, Some(a.state_address(1)));
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
