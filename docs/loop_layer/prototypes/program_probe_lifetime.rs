use shoop_loop_engine::*;

fn main() {
    let target = LoopId::from_parts(0, 1).unwrap();
    let topology = compile(
        Limits::default(),
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: target,
                kind: LoopKind::Composite,
                length: 0,
                sync_source: None,
            }],
            programs: vec![ProgramDescription {
                source: target,
                nominal_unit_frames: 1,
                sections: vec![],
            }],
            triggers: vec![],
        },
    )
    .unwrap();
    let mut resolver = Resolver::prepare(topology).unwrap();
    resolver.preview(Frame(1), &[]).unwrap();
    let decisions = resolver.decisions().unwrap();
    #[cfg(hold_decision)]
    resolver.probe_program_boundary(target, &[]).unwrap();
    assert_eq!(decisions[0].state.id, target);
    let cut = resolver.probe_program_boundary(target, &[]).unwrap();
    assert_eq!(cut.frame, Frame(0));
    assert!(!cut.enters_zero());
    assert_eq!(resolver.commit(), Err(Error::InvalidState));
}
