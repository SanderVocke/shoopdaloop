use super::*;

#[shoop_test]
fn paired_cancellation_released_generations_and_intervening_graphs_never_publish_the_program() {
    for case in 0..6 {
        let mut h = live(false, 0, false);
        let topology = h.ready(0, &compile(limits(0), &description(true, false)).unwrap());
        let graph = if case == 4 {
            let plan = h
                .control
                .compile_graph(&GraphDescription {
                    nodes: vec![node(0), node(1)],
                    ..GraphDescription::default()
                })
                .unwrap();
            h.graph_plan(0, 1, plan)
        } else {
            h.graph(0, 1, true, false)
        };
        h.op(3, play(0));
        h.op(3, play(2));
        let requested_graph = if case == 5 {
            let [scope, slot, generation] = graph.parts();
            GraphStageId::from_parts(scope + 1, slot, generation).unwrap()
        } else {
            graph
        };
        let change = h.op(4, paired(topology, requested_graph));
        h.step(2);
        h.drain();
        let mut current_graph = graph;
        match case {
            0 => {
                h.op(5, Operation::Cancel { sequence: change });
                h.step(1);
            }
            1 => {
                h.send(
                    Lane::Recovery,
                    5,
                    Operation::TopologyCancel { id: topology },
                    Box::new([]),
                );
                h.step(1);
            }
            2 => {
                assert_no_heap(|| h.control.release_graph(graph))
                    .unwrap_or_else(|e| panic!("{:?}", e.error));
                current_graph = h.graph(0, 1, true, true);
                assert_eq!(current_graph.parts()[2], graph.parts()[2] + 1);
            }
            3 => {
                h.op(5, switch(graph, 1, 0));
                h.step(1);
            }
            _ => h.step(1),
        }
        h.drain();
        let gs = h.op(6, Operation::GraphStatus { id: current_graph });
        let ts = h.op(6, Operation::TopologyStatus { id: topology });
        let state = h.op(7, Operation::State { target: id(2) });
        h.op(9, stop(0));
        h.op(9, stop(2));
        h.step(4);
        h.drain();
        if case < 4 {
            let Reply::ProgramGraphChanged {
                id: actual,
                graph: actual_graph,
                graph_epoch,
                topology_epoch,
                completion,
            } = h.reply(change)
            else {
                panic!("terminal")
            };
            assert_eq!(
                (actual, actual_graph, graph_epoch, topology_epoch),
                (
                    topology,
                    graph,
                    Epoch(if case == 3 { 2 } else { 1 }),
                    Epoch(0)
                )
            );
            assert_eq!(
                completion.outcome,
                if case < 2 {
                    Outcome::Cancelled { frame: Frame(5) }
                } else {
                    Outcome::Failed {
                        frame: Frame(5),
                        reason: shoop_loops::Error::InvalidState,
                    }
                }
            );
        } else {
            assert_eq!(
                h.reply(change),
                Reply::Failed(Failure::Runtime(if case == 4 {
                    Error::InvalidShape
                } else {
                    Error::InvalidId
                }))
            );
        }
        assert_eq!(
            h.reply(gs),
            Reply::GraphStatus {
                id: current_graph,
                status: if case == 3 {
                    GraphStageStatus::Taken
                } else {
                    GraphStageStatus::Ready {
                        validated_epoch: Epoch(1),
                    }
                }
            }
        );
        assert_eq!(
            h.reply(ts),
            Reply::TopologyStatus {
                id: topology,
                status: if case == 1 {
                    Status::Idle
                } else {
                    Status::Ready
                }
            }
        );
        let Reply::State(cut) = h.reply(state) else {
            panic!("state")
        };
        assert_eq!(
            (
                cut.topology_epoch,
                cut.graph_epoch,
                cut.state.content_length,
                cut.state.composite.unwrap().iteration,
                cut.state.composite.unwrap().passes
            ),
            (Epoch(0), Epoch(if case == 3 { 2 } else { 1 }), 4, 0, 1)
        );
        h.finish();
    }
}

#[shoop_test]
fn paired_retirement_unread_results_recovery_disconnect_and_render_fault_preserve_ownership() {
    let mut h = live(false, 0, false);
    let topology = h.ready(0, &compile(limits(0), &description(true, false)).unwrap());
    let graph = h.graph(0, 1, true, false);
    h.op(3, play(0));
    h.op(3, play(2));
    let change = h.op(4, paired(topology, graph));
    let mut pending = Vec::new();
    for _ in 0..15 {
        pending.push(h.op(
            4,
            Operation::Transition {
                target: id(1),
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 1000,
                },
                transition: Transition::Stop,
            },
        ));
    }
    h.op(
        8,
        Operation::Transition {
            target: id(1),
            timing: Timing::AfterTicks {
                source: id(0),
                skip: 1000,
            },
            transition: Transition::Stop,
        },
    );
    let clock = h.op(130, Operation::State { target: id(0) });
    h.op(132, stop(0));
    h.op(132, stop(2));
    h.step(4);
    h.step(0);
    assert_eq!(h.engine.diagnostics().epoch, Epoch(1));
    while h.engine.next_frame().0 < 129 {
        let n = (129 - h.engine.next_frame().0).min(32) as usize;
        h.step(n);
    }
    let cancelled = h.send(
        Lane::Recovery,
        129,
        Operation::Cancel {
            sequence: pending[0],
        },
        Box::new([]),
    );
    h.step(4);
    h.drain();
    let Reply::ProgramGraphChanged {
        completion,
        graph_epoch,
        topology_epoch,
        ..
    } = h.reply(change)
    else {
        panic!("paired")
    };
    assert_eq!(
        (completion.outcome, graph_epoch, topology_epoch),
        (Outcome::Applied { frame: Frame(7) }, Epoch(2), Epoch(1))
    );
    assert_eq!(
        h.reply(cancelled),
        Reply::Cancelled {
            sequence: pending[0],
            delivered: true
        }
    );
    let Reply::State(cut) = h.reply(clock) else {
        panic!("clock")
    };
    assert_eq!(
        (
            cut.frame,
            cut.state.cycles,
            cut.state.last_pulse,
            cut.graph_epoch,
            cut.topology_epoch
        ),
        (Frame(130), 127, Some(Frame(130)), Epoch(2), Epoch(1))
    );
    h.finish();
    for phase in 0..3 {
        let media = phase == 2;
        let mut h = Harness::options(media, 0, false, staging(), 8, media.then_some(Frame(7)));
        h.engine.prepare_graph_staging(2).unwrap();
        let topology = h.ready(0, &compile(limits(0), &description(true, false)).unwrap());
        let graph = h.graph(0, 1, false, false);
        h.op(3, play(0));
        h.op(3, play(2));
        let change = h.op(4, paired(topology, graph));
        let query = h.op(7, Operation::State { target: id(2) });
        h.step(if phase == 1 { 5 } else { 2 });
        h.drain();
        if phase == 2 {
            h.step(2);
            let [left, right] = &mut h.output;
            h.midi[0].clear();
            assert_eq!(
                assert_no_heap(|| h.engine.process(
                    AudioBlock {
                        frame: Frame(7),
                        frames: 1,
                        inputs: &[],
                        outputs: &mut [&mut left[..1], &mut right[..1]]
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut h.midi
                    }
                )),
                Err(Error::InvalidState)
            );
            assert_eq!(h.engine.next_frame(), Frame(8));
            h.drain();
        }
        if phase != 0 {
            let Reply::ProgramGraphChanged {
                completion,
                graph_epoch,
                topology_epoch,
                ..
            } = h.reply(change)
            else {
                panic!("paired")
            };
            assert_eq!(
                (completion.outcome, graph_epoch, topology_epoch),
                (Outcome::Applied { frame: Frame(7) }, Epoch(2), Epoch(1))
            );
            let Reply::State(cut) = h.reply(query) else {
                panic!("cut")
            };
            assert_eq!(
                (
                    cut.frame,
                    cut.graph_epoch,
                    cut.topology_epoch,
                    cut.state.content_length
                ),
                (Frame(7), Epoch(2), Epoch(1), 3)
            );
        }
        let Harness {
            control,
            mut engine,
            imports,
            ..
        } = h;
        drop(control);
        if phase == 0 {
            assert_no_heap(|| {
                engine.process(
                    AudioBlock {
                        frame: Frame(5),
                        frames: 3,
                        inputs: &[],
                        outputs: &mut [],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
            })
            .unwrap();
        }
        assert_eq!(engine.diagnostics().epoch, Epoch(2));
        let mut stopped = engine.into_stopped();
        assert_no_heap(|| {
            stopped.cleanup().unwrap();
            stopped.cleanup().unwrap();
            for resource in &imports {
                stopped.resources().remove(*resource).unwrap();
            }
            for _ in 0..128 {
                stopped.resources().begin_callback();
                stopped.resources().reclaim();
            }
            assert_eq!(stopped.media_usage().references, 0);
            assert_eq!(stopped.resource_usage().free_pages, 128);
            assert_eq!(stopped.resource_usage().free_versions, 16);
            assert_eq!(stopped.resource_usage().free_snapshots, 16);
        });
    }
}
