use shoop_loop_engine::*;

fn main() {
    let target = LoopId::from_parts(0, 1).unwrap();
    let description = TopologyDescription {
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
    };
    let mut resolver =
        Resolver::prepare(compile(Limits::default(), &description).unwrap()).unwrap();
    let candidate = compile(Limits::default(), &description).unwrap();
    #[cfg(hold_candidate)]
    let candidate_view = candidate.program(target).unwrap();
    let request = resolver
        .stage_program(Epoch(0), target, Sequence(1), Frame(0), candidate)
        .unwrap();
    #[cfg(hold_candidate)]
    assert_eq!(candidate_view.source, target);
    resolver.preview(Frame(1), &[]).unwrap();
    resolver.commit().unwrap();
    resolver.finish(Frame(1)).unwrap();
    resolver.preview(Frame(2), &[]).unwrap();
    let decisions = resolver.decisions().unwrap();
    #[cfg(hold_decision)]
    resolver.take_program_change(request).unwrap();
    #[cfg(hold_quiesce)]
    resolver.quiesce();
    assert_eq!(decisions[0].state.id, target);
    let result = resolver.take_program_change(request).unwrap().unwrap();
    let retired_view = result.topology.program(target).unwrap();
    #[cfg(hold_retired)]
    drop(result);
    assert_eq!(retired_view.source, target);
    #[cfg(not(hold_retired))]
    drop(result);
    resolver.quiesce();
    assert!(resolver.is_quiescent());
    assert_eq!(resolver.frame(), Frame(1));
    assert_eq!(resolver.preview(Frame(2), &[]), Err(Error::InvalidState));
}
