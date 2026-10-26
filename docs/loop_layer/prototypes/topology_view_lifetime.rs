#![forbid(unsafe_code)]
use shoop_loop_engine::*;

fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn prepared() -> PreparedTopology {
    compile(
        Limits::default(),
        &TopologyDescription {
            loops: vec![
                LoopSpec {
                    id: id(0),
                    kind: LoopKind::Primitive,
                    length: 4,
                    sync_source: None,
                },
                LoopSpec {
                    id: id(1),
                    kind: LoopKind::Composite,
                    length: 0,
                    sync_source: None,
                },
            ],
            triggers: vec![],
            programs: vec![ProgramDescription {
                source: id(1),
                nominal_unit_frames: 1,
                sections: vec![Section {
                    entries: vec![Entry {
                        target: id(0),
                        delay: 2,
                        duration: Some(4),
                        repetitions: 1,
                        mode: None,
                    }],
                }],
            }],
        },
    )
    .unwrap()
}

fn main() {
    let mut resolver = Resolver::prepare(prepared()).unwrap();
    let candidate = prepared();
    #[cfg(compile_fail_program)]
    let held = resolver.topology().program(id(1)).unwrap();
    #[cfg(compile_fail_target)]
    let held = resolver
        .topology()
        .program(id(1))
        .unwrap()
        .target(id(0))
        .unwrap();
    #[cfg(compile_fail_component)]
    let held = resolver.topology().components().get(0).unwrap();
    #[cfg(not(any(compile_fail_program, compile_fail_target, compile_fail_component)))]
    {
        let program: shoop_loop_engine::program::CompiledProgram<'_> =
            resolver.topology().program(id(1)).unwrap();
        let targets: shoop_loop_engine::program::TargetTimelines<'_> = program.targets();
        let target: shoop_loop_engine::program::TargetTimeline<'_> = targets.get(0).unwrap();
        assert_eq!(target.target, id(0));
        assert_eq!(target.at(2).unwrap().end, 6);
        let components: shoop_loop_engine::topology::Components<'_> =
            resolver.topology().components();
        assert_eq!(components.get(0).unwrap(), &[id(1)]);
    }
    let retired = resolver.replace_topology(Epoch(0), candidate).unwrap();
    #[cfg(compile_fail_program)]
    assert_eq!(held.target(id(0)).unwrap().at(2).unwrap().end, 6);
    #[cfg(compile_fail_target)]
    assert_eq!(held.at(2).unwrap().end, 6);
    #[cfg(compile_fail_component)]
    assert_eq!(held.len(), 1);
    assert_eq!(resolver.epoch(), Epoch(1));
    assert_eq!(retired.program(id(1)).unwrap().iterations, 6);
    drop(retired);
}
