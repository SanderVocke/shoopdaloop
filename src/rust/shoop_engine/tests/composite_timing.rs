use shoop_engine::{
    compile_composite_plan, AcceptedTimelineControl, BoundaryTargetAction, ChannelMode,
    CompiledCompositePlan, CompositeBoundaryTimeline, CompositeEntry, CompositePlanDescriptor,
    CompositePlanLimits, CompositeSection, CompositeTimeline, CompositeTimelineFault,
    CompositeTimelineLimits, CompositeTimelineNode, DummyAudioPort, LoopIdentity, LoopMode,
    LoopTargetCatalog, LoopTargetKind, LoopTargetMetadata, Port, PortDirection, PortId, Session,
    SessionError,
};

fn basic(slot: u32) -> LoopIdentity {
    LoopIdentity {
        slot,
        generation: 1,
        kind: LoopTargetKind::Basic,
    }
}

fn composite(slot: u32) -> LoopIdentity {
    LoopIdentity {
        slot,
        generation: 1,
        kind: LoopTargetKind::Composite,
    }
}

fn compile_plan(
    source: LoopIdentity,
    sync_length: u64,
    entries: &[(LoopIdentity, i64, Option<LoopMode>)],
    lengths: &[(LoopIdentity, u64)],
) -> CompiledCompositePlan {
    let catalog = LoopTargetCatalog::new(
        lengths
            .iter()
            .map(|(identity, length_samples)| LoopTargetMetadata {
                identity: *identity,
                length_samples: *length_samples,
            })
            .collect(),
    )
    .unwrap();
    compile_composite_plan(
        &CompositePlanDescriptor {
            source,
            sync_length,
            timelines: vec![CompositeTimeline {
                sections: vec![CompositeSection {
                    entries: entries
                        .iter()
                        .map(|(target, delay, mode)| CompositeEntry {
                            target: *target,
                            delay: *delay,
                            n_cycles: Some(1),
                            mode: *mode,
                        })
                        .collect(),
                }],
            }],
        },
        &catalog,
        &[],
        CompositePlanLimits::default(),
    )
    .unwrap()
}

fn start(at_sample: u64, target: LoopIdentity, mode: LoopMode) -> AcceptedTimelineControl {
    AcceptedTimelineControl {
        at_sample,
        target,
        action: BoundaryTargetAction::SetMode {
            mode,
            offset_samples: 0,
            retrigger: true,
        },
        acceptance_sequence: 1,
    }
}

struct AudioFixture {
    session: Session,
    output: usize,
    source: usize,
    child: usize,
    composite: LoopIdentity,
}

fn audio_fixture() -> AudioFixture {
    let mut session = Session::default();
    let output = session.add_port(Port::Dummy(DummyAudioPort::new(
        PortId(1),
        "out",
        PortDirection::Output,
        16,
    )));
    let source = session.create_loop();
    let child = session.create_loop();
    let channel = session
        .add_audio_channel(child, 16, ChannelMode::Direct)
        .unwrap();
    session.connect_channel_output(channel, output).unwrap();
    session
        .loop_mut(child)
        .unwrap()
        .audio_channel_mut(0)
        .unwrap()
        .load_data(&[1.0, 2.0, 3.0, 4.0]);
    session.loop_mut(child).unwrap().set_length(4);
    session.loop_mut(source).unwrap().set_length(4);
    session.set_loop_mode(source, LoopMode::Playing).unwrap();
    session.apply_graph_changes().unwrap();

    let composite = composite(10);
    let plan = compile_plan(
        composite,
        4,
        &[(basic(child as u32), 1, None)],
        &[
            (basic(source as u32), 4),
            (basic(child as u32), 4),
            (composite, 4),
        ],
    );
    let timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan,
            sync_source: basic(source as u32),
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    session.install_composite_timeline(timeline).unwrap();
    session
        .composite_timeline_mut()
        .queue_control(start(0, composite, LoopMode::Playing))
        .unwrap();

    AudioFixture {
        session,
        output,
        source,
        child,
        composite,
    }
}

fn render(partitions: &[usize]) -> (Vec<f32>, Vec<(u64, LoopIdentity, BoundaryTargetAction)>) {
    let mut fixture = audio_fixture();
    let total: usize = partitions.iter().sum();
    fixture
        .session
        .port_mut(fixture.output)
        .unwrap()
        .as_dummy_mut()
        .unwrap()
        .request_data(total);
    let mut trace = Vec::new();
    for &partition in partitions {
        fixture.session.process(partition);
        trace.extend(
            fixture
                .session
                .composite_timeline()
                .trace()
                .iter()
                .map(|entry| (entry.at_sample, entry.target, entry.action)),
        );
    }
    let audio = fixture
        .session
        .port_mut(fixture.output)
        .unwrap()
        .as_dummy_mut()
        .unwrap()
        .dequeue_data(total)
        .unwrap();
    (audio, trace)
}

#[shoop_wasm_test_support::shoop_test]
fn session_rechecks_primitive_generations_before_timeline_installation() {
    let mut session = Session::default();
    let child = session.create_loop();
    let child_identity = session.loop_identity(child).unwrap();
    session.remove_loop(child).unwrap();
    let source = composite(10);
    let plan = compile_plan(
        source,
        4,
        &[(child_identity, 0, None)],
        &[(child_identity, 4), (source, 4)],
    );
    let timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan,
            sync_source: child_identity,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        session.install_composite_timeline(timeline),
        Err(SessionError::StaleCompositeTarget(0))
    ));
}

#[shoop_wasm_test_support::shoop_test]
fn session_rejects_cycles_spanning_composite_and_primitive_sync_edges() {
    let mut session = Session::default();
    let source_basic = session.create_loop();
    let follower_basic = session.create_loop();
    session
        .set_loop_sync_source(follower_basic, Some(source_basic))
        .unwrap();
    let source = composite(10);
    let source_identity = basic(source_basic as u32);
    let follower_identity = basic(follower_basic as u32);
    let plan = compile_plan(
        source,
        4,
        &[(source_identity, 0, None)],
        &[(source_identity, 4), (follower_identity, 4), (source, 4)],
    );
    let timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan,
            sync_source: follower_identity,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        session.install_composite_timeline(timeline),
        Err(SessionError::CompositeTimeline(
            shoop_engine::CompositeTimelineBuildError::DependencyCycle
        ))
    ));
}

#[shoop_wasm_test_support::shoop_test]
fn mid_callback_composite_transition_changes_the_exact_first_output_sample() {
    let (audio, trace) = render(&[8]);
    assert_eq!(audio, vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
    assert!(trace.iter().any(|(sample, target, action)| {
        *sample == 4
            && *target == basic(1)
            && matches!(
                action,
                BoundaryTargetAction::SetMode {
                    mode: LoopMode::Playing,
                    ..
                }
            )
    }));
}

#[shoop_wasm_test_support::shoop_test]
fn callback_size_and_arbitrary_partitions_do_not_change_audio_or_transition_trace() {
    let contiguous = render(&[12]);
    let split = render(&[3, 2, 1, 4, 2]);
    let single_samples = render(&[1; 12]);
    assert_eq!(contiguous, split);
    assert_eq!(contiguous, single_samples);
    assert_eq!(
        contiguous.0,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]
    );
}

#[shoop_wasm_test_support::shoop_test]
fn a_source_that_wraps_multiple_times_advances_every_composite_boundary() {
    let mut fixture = audio_fixture();
    fixture.session.process(12);
    let runtime = fixture
        .session
        .composite_timeline()
        .runtime(fixture.composite)
        .unwrap();
    assert_eq!(runtime.iteration(), 1);
    assert_eq!(runtime.cycle_count(), 1);
    assert_eq!(fixture.session.position_of(fixture.source), Some(0));
    assert_eq!(fixture.session.position_of(fixture.child), Some(0));
}

#[shoop_wasm_test_support::shoop_test]
fn iteration_aligned_composite_events_reuse_the_source_poi() {
    let mut with_composite = audio_fixture();
    // Consume the timestamp-zero control boundary before comparing steady-state work.
    with_composite.session.process(1);
    with_composite.session.process(3);
    with_composite.session.process(4);
    assert_eq!(with_composite.session.n_sub_blocks_last_cycle(), 1);

    let mut without = Session::default();
    let source = without.create_loop();
    without.loop_mut(source).unwrap().set_length(4);
    without.set_loop_mode(source, LoopMode::Playing).unwrap();
    without.apply_graph_changes().unwrap();
    without.process(4);
    assert_eq!(without.n_sub_blocks_last_cycle(), 1);
}

#[shoop_wasm_test_support::shoop_test]
fn sub_block_overflow_latches_and_never_processes_the_remainder_late() {
    let mut session = Session::default();
    let source = session.create_loop();
    session.loop_mut(source).unwrap().set_length(1);
    session.set_loop_mode(source, LoopMode::Playing).unwrap();
    session.apply_graph_changes().unwrap();

    session.process(20);
    assert_eq!(
        session.composite_timeline().fault().fault,
        CompositeTimelineFault::SubBlockCapacity
    );
    assert_eq!(session.composite_timeline().sample_clock(), 16);
    let frozen_position = session.position_of(source);
    session.process(4);
    assert_eq!(session.composite_timeline().sample_clock(), 16);
    assert_eq!(session.position_of(source), frozen_position);
}

#[shoop_wasm_test_support::shoop_test]
fn composite_to_primitive_to_composite_propagation_settles_before_audio_advances() {
    let mut session = Session::default();
    let clock = session.create_loop();
    let started_source = session.create_loop();
    let primitive_follower = session.create_loop();
    let child = session.create_loop();
    session
        .set_loop_sync_source(primitive_follower, Some(started_source))
        .unwrap();
    for loop_idx in [clock, started_source, primitive_follower, child] {
        session.loop_mut(loop_idx).unwrap().set_length(4);
    }
    session.apply_graph_changes().unwrap();

    let follower = composite(10);
    let producer = composite(20);
    let identities = [
        (basic(clock as u32), 4),
        (basic(started_source as u32), 4),
        (basic(primitive_follower as u32), 4),
        (basic(child as u32), 4),
        (follower, 4),
        (producer, 4),
    ];
    let follower_plan = compile_plan(follower, 4, &[(basic(child as u32), 1, None)], &identities);
    let producer_plan = compile_plan(
        producer,
        4,
        &[(basic(started_source as u32), 0, None)],
        &identities,
    );
    session
        .install_composite_timeline(
            CompositeBoundaryTimeline::new(
                vec![
                    CompositeTimelineNode {
                        plan: follower_plan,
                        sync_source: basic(primitive_follower as u32),
                    },
                    CompositeTimelineNode {
                        plan: producer_plan,
                        sync_source: basic(clock as u32),
                    },
                ],
                CompositeTimelineLimits::default(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .composite_timeline_mut()
        .queue_control(start(0, follower, LoopMode::Playing))
        .unwrap();
    session
        .composite_timeline_mut()
        .queue_control(start(1, producer, LoopMode::Playing))
        .unwrap();
    session.process(2);

    assert_eq!(session.loop_(child).unwrap().mode(), LoopMode::Playing);
    assert!(session
        .composite_timeline()
        .trace()
        .iter()
        .any(|entry| entry.at_sample == 1 && entry.target == basic(child as u32)));
}

#[shoop_wasm_test_support::shoop_test]
fn an_explicit_script_stop_commits_at_its_source_boundary() {
    let mut session = Session::default();
    let source_loop = session.create_loop();
    let child_loop = session.create_loop();
    session.loop_mut(source_loop).unwrap().set_length(4);
    session.loop_mut(child_loop).unwrap().set_length(4);
    session
        .set_loop_mode(source_loop, LoopMode::Playing)
        .unwrap();
    session.apply_graph_changes().unwrap();

    let source = composite(30);
    let child = basic(child_loop as u32);
    let catalog = LoopTargetCatalog::new(vec![
        LoopTargetMetadata {
            identity: source,
            length_samples: 8,
        },
        LoopTargetMetadata {
            identity: child,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: basic(source_loop as u32),
            length_samples: 4,
        },
    ])
    .unwrap();
    let plan = compile_composite_plan(
        &CompositePlanDescriptor {
            source,
            sync_length: 4,
            timelines: vec![CompositeTimeline {
                sections: vec![
                    CompositeSection {
                        entries: vec![CompositeEntry {
                            target: child,
                            delay: 0,
                            n_cycles: Some(1),
                            mode: Some(LoopMode::Playing),
                        }],
                    },
                    CompositeSection {
                        entries: vec![CompositeEntry {
                            target: child,
                            delay: 0,
                            n_cycles: Some(1),
                            mode: Some(LoopMode::Stopped),
                        }],
                    },
                ],
            }],
        },
        &catalog,
        &[],
        CompositePlanLimits::default(),
    )
    .unwrap();
    session
        .install_composite_timeline(
            CompositeBoundaryTimeline::new(
                vec![CompositeTimelineNode {
                    plan,
                    sync_source: basic(source_loop as u32),
                }],
                CompositeTimelineLimits::default(),
            )
            .unwrap(),
        )
        .unwrap();
    session
        .composite_timeline_mut()
        .queue_control(start(0, source, LoopMode::Playing))
        .unwrap();
    session.process(1);
    assert_eq!(session.loop_(child_loop).unwrap().mode(), LoopMode::Playing);
    session.process(3);
    assert_eq!(session.loop_(child_loop).unwrap().mode(), LoopMode::Stopped);
}

#[shoop_wasm_test_support::shoop_test]
fn timestamped_script_modes_commit_before_post_boundary_samples() {
    let mut session = Session::default();
    let sync = session.create_loop();
    let loops: Vec<_> = (0..4).map(|_| session.create_loop()).collect();
    for &loop_idx in &loops {
        session.loop_mut(loop_idx).unwrap().set_length(8);
    }
    session.apply_graph_changes().unwrap();

    let source = composite(20);
    let modes = [
        LoopMode::Stopped,
        LoopMode::Recording,
        LoopMode::Replacing,
        LoopMode::Playing,
    ];
    let identities: Vec<_> = loops.iter().map(|index| basic(*index as u32)).collect();
    let mut lengths: Vec<_> = identities.iter().map(|identity| (*identity, 8)).collect();
    lengths.push((basic(sync as u32), 4));
    lengths.push((source, 4));
    let entries: Vec<_> = identities
        .iter()
        .zip(modes)
        .map(|(identity, mode)| (*identity, 0, Some(mode)))
        .collect();
    let plan = compile_plan(source, 4, &entries, &lengths);
    let timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan,
            sync_source: basic(sync as u32),
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    session.install_composite_timeline(timeline).unwrap();
    session
        .composite_timeline_mut()
        .queue_control(start(3, source, LoopMode::Playing))
        .unwrap();
    session.process(6);

    assert_eq!(session.loop_(loops[0]).unwrap().mode(), LoopMode::Stopped);
    assert_eq!(session.loop_(loops[1]).unwrap().mode(), LoopMode::Recording);
    assert_eq!(session.loop_(loops[2]).unwrap().mode(), LoopMode::Replacing);
    assert_eq!(session.loop_(loops[3]).unwrap().mode(), LoopMode::Playing);
    assert_eq!(
        session
            .composite_timeline()
            .trace()
            .iter()
            .filter(|entry| entry.at_sample == 3)
            .count(),
        5
    );
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
