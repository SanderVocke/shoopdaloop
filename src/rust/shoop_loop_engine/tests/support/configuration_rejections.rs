use super::*;

fn call(h: &mut Harness, operation: Operation) -> Reply {
    let sequence = h.op(h.engine.next_frame().0, operation);
    h.step(1);
    h.drain();
    h.reply(sequence)
}
fn unchanged(
    h: &mut Harness,
    roles: RoleStageId,
    topologies: [StageId; 2],
    graphs: [GraphStageId; 2],
    media: media::MediaId,
) {
    let at = h.engine.next_frame().0;
    let role = h.op(at, Operation::RoleStatus { id: roles });
    let ts = topologies.map(|id| h.op(at, Operation::TopologyStatus { id }));
    let gs = graphs.map(|id| h.op(at, Operation::GraphStatus { id }));
    let cut = h.op(at, Operation::State { target: id(1) });
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(role),
        Reply::RoleStatus {
            id: roles,
            status: RoleStatus::Ready
        }
    );
    for (sequence, id) in ts.into_iter().zip(topologies) {
        assert_eq!(
            h.reply(sequence),
            Reply::TopologyStatus {
                id,
                status: Status::Ready
            }
        );
    }
    for (sequence, id) in gs.into_iter().zip(graphs) {
        assert_eq!(
            h.reply(sequence),
            Reply::GraphStatus {
                id,
                status: GraphStageStatus::Ready {
                    validated_epoch: Epoch(1)
                }
            }
        );
    }
    let Reply::State(cut) = h.reply(cut) else {
        panic!("old cut")
    };
    assert_eq!(
        (
            cut.graph_epoch,
            cut.topology_epoch,
            cut.state.id,
            cut.state.mode,
            cut.playback
        ),
        (Epoch(1), Epoch(0), id(1), Mode::Stopped, Some(media))
    );
}
#[shoop_test]
fn configuration_refusals_and_final_resolver_failure_leave_all_three_owners_ready() {
    let mut h = prepared(true, 0, false, 64, false);
    let sources = source_pair(&mut h);
    let roles = begin(&mut h, 0, 1);
    let target = generation(1, 2);
    assert_eq!(
        call(
            &mut h,
            Operation::RoleAppend {
                id: roles,
                previous: Some(id(1)),
                initial: Some(initial(target, sources, true))
            }
        ),
        Reply::RoleStatus {
            id: roles,
            status: RoleStatus::Ready
        }
    );
    let mut wrong = changed_description(2, false);
    wrong
        .loops
        .iter_mut()
        .find(|n| n.id == target)
        .unwrap()
        .length = 3;
    let bad_topology = h.ready(0, &compile(limits(0), &wrong).unwrap());
    let topology = h.ready(
        1,
        &compile(limits(0), &changed_description(2, false)).unwrap(),
    );
    let plan = h
        .control
        .compile_graph(&GraphDescription {
            nodes: vec![node(0), node(1), node(2)],
            ..GraphDescription::default()
        })
        .unwrap();
    let bad_graph = h.graph_plan(0, 1, plan);
    let graph = h.graph(1, 1, true, false);
    let media = h
        .control
        .media()
        .iter()
        .find(|m| m.target == id(1))
        .unwrap()
        .media;
    let [scope, slot, generation] = roles.parts();
    let foreign_role = RoleStageId::from_parts([scope + 1, slot, generation]).unwrap();
    let [scope, slot, generation] = graph.parts();
    let foreign_graph = GraphStageId::from_parts(scope + 1, slot, generation).unwrap();
    let foreign_topology = StageId::from_parts(u32::MAX, 1).unwrap();
    for (operation, error) in [
        (
            configuration(foreign_role, topology, graph, 1, 0),
            Failure::Runtime(Error::InvalidId),
        ),
        (
            configuration(roles, foreign_topology, graph, 1, 0),
            Failure::Timeline(shoop_loop_engine::Error::InvalidId),
        ),
        (
            configuration(roles, topology, foreign_graph, 1, 0),
            Failure::Runtime(Error::InvalidId),
        ),
        (
            configuration(roles, topology, graph, 0, 0),
            Failure::Runtime(Error::StaleEpoch),
        ),
        (
            configuration(roles, topology, graph, 1, 1),
            Failure::Timeline(shoop_loop_engine::Error::InvalidState),
        ),
        (
            configuration(roles, bad_topology, graph, 1, 0),
            Failure::Runtime(Error::InvalidShape),
        ),
        (
            configuration(roles, topology, bad_graph, 1, 0),
            Failure::Runtime(Error::InvalidShape),
        ),
    ] {
        assert_eq!(call(&mut h, operation), Reply::Failed(error));
        unchanged(
            &mut h,
            roles,
            [bad_topology, topology],
            [bad_graph, graph],
            media,
        );
    }
    let at = h.engine.next_frame().0;
    h.op(at, play(2));
    h.op(at, stop(1));
    h.step(1);
    h.drain();
    assert_eq!(
        call(&mut h, configuration(roles, topology, graph, 1, 0)),
        Reply::Failed(Failure::Timeline(shoop_loop_engine::Error::InvalidState))
    );
    unchanged(
        &mut h,
        roles,
        [bad_topology, topology],
        [bad_graph, graph],
        media,
    );
    let at = h.engine.next_frame().0;
    h.op(at, stop(2));
    h.step(1);
    h.drain();
    let ticket = assert_no_heap(|| {
        h.control.submit(
            Lane::Normal,
            h.engine.next_frame(),
            configuration(roles, topology, graph, 1, 0),
        )
    })
    .unwrap_or_else(|e| panic!("{:?}", e.error));
    ticket.cancel();
    h.step(1);
    let completion = assert_no_heap(|| h.control.try_completion()).unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.outcome, TransportOutcome::Cancelled);
    assert!(assert_no_heap(|| h.control.try_execution()).is_none());
    let ticket = assert_no_heap(|| {
        h.control.submit(
            Lane::Recovery,
            h.engine.next_frame(),
            configuration(roles, topology, graph, 1, 0),
        )
    })
    .unwrap_or_else(|e| panic!("{:?}", e.error));
    h.step(1);
    let completion = assert_no_heap(|| h.control.try_completion()).unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(
        completion.outcome,
        TransportOutcome::Rejected(Error::InvalidState)
    );
    assert!(assert_no_heap(|| h.control.try_execution()).is_none());
    let unexpected = h.send(
        Lane::Normal,
        h.engine.next_frame().0,
        configuration(roles, topology, graph, 1, 0),
        Box::new([1]),
    );
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(unexpected),
        Reply::Failed(Failure::Runtime(Error::InvalidShape))
    );
    unchanged(
        &mut h,
        roles,
        [bad_topology, topology],
        [bad_graph, graph],
        media,
    );
    assert_eq!(
        call(&mut h, configuration(roles, topology, graph, 1, 0)),
        Reply::ConfigurationActivated {
            roles,
            topology,
            graph,
            graph_epoch: Epoch(2),
            topology_epoch: Epoch(1)
        }
    );
    assert_eq!(
        call(&mut h, Operation::RoleCancel { id: roles }),
        Reply::Failed(Failure::Runtime(Error::InvalidState))
    );
    assert_eq!(
        call(&mut h, configuration(roles, topology, graph, 2, 1)),
        Reply::Failed(Failure::Runtime(Error::InvalidState))
    );
    let Reply::State(cut) = call(&mut h, Operation::State { target }) else {
        panic!("new cut")
    };
    assert_eq!(
        (
            cut.state.id,
            cut.graph_epoch,
            cut.topology_epoch,
            cut.state.mode,
            cut.state.content_length
        ),
        (target, Epoch(2), Epoch(1), Mode::Stopped, 4)
    );
    assert_ne!(cut.playback, Some(media));
    h.finish();
}

#[shoop_test]
fn batch_capacity_is_preflighted_without_reusing_retirement_credits_or_publishing_a_prefix() {
    for media_slots in [3, 4] {
        let mut h = Harness::with_roles(
            false,
            0,
            false,
            staging(),
            media_slots,
            None,
            RoleSettings {
                candidates: 2,
                sources: 0,
                source_packets_per_callback: 0,
            },
            false,
        );
        h.engine.prepare_graph_staging(2).unwrap();
        let roles = begin(&mut h, 0, 2);
        for slot in 0..2 {
            let reply = call(
                &mut h,
                Operation::RoleAppend {
                    id: roles,
                    previous: Some(id(slot)),
                    initial: Some(timing_initial(
                        generation(slot, 2),
                        if slot == 0 { 1 } else { 4 },
                    )),
                },
            );
            assert_eq!(
                reply,
                Reply::RoleStatus {
                    id: roles,
                    status: if slot == 0 {
                        RoleStatus::Receiving {
                            received: 1,
                            expected: 2,
                        }
                    } else {
                        RoleStatus::Ready
                    }
                }
            );
        }
        let mut description = changed_description(2, false);
        description.loops[0].id = generation(0, 2);
        for node in &mut description.loops {
            if node.sync_source == Some(id(0)) {
                node.sync_source = Some(generation(0, 2));
            }
        }
        let topology = h.ready(0, &compile(limits(0), &description).unwrap());
        let graph = h.graph(0, 1, true, false);
        let result = call(&mut h, configuration(roles, topology, graph, 1, 0));
        assert_eq!(
            result,
            if media_slots == 3 {
                Reply::Failed(Failure::Runtime(Error::Capacity))
            } else {
                Reply::ConfigurationActivated {
                    roles,
                    topology,
                    graph,
                    graph_epoch: Epoch(2),
                    topology_epoch: Epoch(1),
                }
            }
        );
        for slot in 0..2 {
            let owner = if media_slots == 3 {
                id(slot)
            } else {
                generation(slot, 2)
            };
            let Reply::State(cut) = call(&mut h, Operation::State { target: owner }) else {
                panic!("batch cut")
            };
            assert_eq!(
                (cut.state.id, cut.graph_epoch, cut.topology_epoch),
                (
                    owner,
                    Epoch(if media_slots == 3 { 1 } else { 2 }),
                    Epoch(if media_slots == 3 { 0 } else { 1 })
                )
            );
        }
        assert_eq!(
            call(&mut h, Operation::RoleStatus { id: roles }),
            Reply::RoleStatus {
                id: roles,
                status: if media_slots == 3 {
                    RoleStatus::Ready
                } else {
                    RoleStatus::Taken
                }
            }
        );
        h.finish();
    }
}
