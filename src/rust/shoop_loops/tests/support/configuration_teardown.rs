use super::*;

#[shoop_test]
fn disconnect_and_fault_cleanup_recover_source_receivers_partial_batches_and_published_capture_claims(
) {
    for phase in 0..8 {
        let mut h = Harness::with_node_pairs(
            true,
            0,
            false,
            staging(),
            8,
            if phase == 7 { Some(Frame(7)) } else { None },
            NodePairSettings {
                candidates: 2,
                sources: 8,
                source_packets_per_callback: 64,
            },
            false,
        );
        h.engine.prepare_graph_staging(2).unwrap();
        let target = generation(1, 2);
        let mut published = false;
        if phase == 0 {
            h.op(
                0,
                Operation::NodePairSnapshot {
                    resource: h.imports[1],
                },
            );
        } else if phase == 1 {
            h.op(
                0,
                Operation::NodePairSnapshot {
                    resource: h.imports[1],
                },
            );
            h.step(1);
        } else {
            let sources = source_pair(&mut h);
            if phase >= 3 {
                let node_pairs = begin(&mut h, 0, if phase == 3 { 2 } else { 1 });
                h.op(
                    3,
                    Operation::NodePairAppend {
                        id: node_pairs,
                        previous: Some(id(1)),
                        initial: Some(initial(target, sources, false)),
                    },
                );
                h.step(1);
                h.drain();
                if phase >= 5 {
                    let topology = h.ready(
                        0,
                        &compile(limits(0), &changed_description(2, false)).unwrap(),
                    );
                    let graph = h.graph(0, 1, false, false);
                    let applied = h.op(7, configuration(node_pairs, topology, graph, 1, 0));
                    h.op(
                        7,
                        transition(
                            target,
                            Transition::Record {
                                monitor_previous: true,
                            },
                        ),
                    );
                    let query = h.op(7, Operation::State { target });
                    if phase >= 6 {
                        if phase == 6 {
                            h.step(1);
                        } else {
                            let [left, right] = &mut h.output;
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
                        }
                        h.drain();
                        assert_eq!(
                            h.reply(applied),
                            Reply::ConfigurationActivated {
                                node_pairs,
                                topology,
                                graph,
                                graph_epoch: Epoch(2),
                                topology_epoch: Epoch(1)
                            }
                        );
                        let Reply::State(cut) = h.reply(query) else {
                            panic!("cut")
                        };
                        assert_eq!(
                            (
                                cut.frame,
                                cut.state.id,
                                cut.state.mode,
                                cut.graph_epoch,
                                cut.topology_epoch
                            ),
                            (Frame(7), target, Mode::Recording, Epoch(2), Epoch(1))
                        );
                        published = true;
                    }
                }
            }
        }
        let Harness {
            control,
            mut engine,
            imports,
            mut output,
            mut midi,
            ..
        } = h;
        drop(control);
        if phase == 5 {
            let [left, right] = &mut output;
            assert_no_heap(|| {
                engine.process(
                    AudioBlock {
                        frame: Frame(7),
                        frames: 1,
                        inputs: &[],
                        outputs: &mut [&mut left[..1], &mut right[..1]],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut midi,
                    },
                )
            })
            .unwrap();
            published = true;
        }
        assert_eq!(
            engine.diagnostics().epoch,
            Epoch(if published {
                2
            } else if phase == 0 {
                0
            } else {
                1
            })
        );
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
