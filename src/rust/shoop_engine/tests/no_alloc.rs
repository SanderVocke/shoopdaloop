#![cfg(not(target_arch = "wasm32"))]

//! Proves the process path does not allocate.
//!
//! This is the strongest argument for the Rust port: "is this realtime-safe?"
//! becomes a test that fails rather than a code-review opinion. `assert_no_alloc`
//! installs an allocator that aborts on any allocation inside `assert_no_alloc!`,
//! so a regression shows up immediately instead of as an xrun months later.
//!
//! In a separate integration test because it needs a global allocator, which must
//! not be imposed on the unit test binary.

use assert_no_alloc::*;
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::content_snapshot::ContentSnapshotRuntime;
use shoop_engine::dummy_midi_port::DummyMidiPort;
use shoop_engine::dummy_port::{DummyAudioPort, PortId};
use shoop_engine::external_audio_port::ExternalAudioPort;
use shoop_engine::external_midi_port::ExternalMidiPort;
use shoop_engine::internal_audio_port::InternalAudioPort;
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi;
use shoop_engine::port::{PortConnectability, PortDirection};
use shoop_engine::realtime_alloc_guard;
use shoop_engine::session::{AudioRingbufferAdoption, Port, Session};
use shoop_engine::{
    compile_composite_plan, BoundaryTargetAction, CompositeBoundaryTimeline, CompositeEntry,
    CompositePlanDescriptor, CompositePlanLimits, CompositeRuntime, CompositeSection,
    CompositeTimeline, CompositeTimelineLimits, CompositeTimelineNode, ContentMutation,
    LoopIdentity, LoopTargetCatalog, LoopTargetKind, LoopTargetMetadata, MidiStorageElem,
    PreparedAudioChannelData, PreparedAudioRingbufferAdoptionChannel,
};

#[cfg(debug_assertions)]
#[global_allocator]
static A: AllocDisabler = AllocDisabler;

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn realtime_guard_reverse_guard_allows_exceptional_allocations() {
    realtime_alloc_guard::set_enabled(true);
    realtime_alloc_guard::forbid_alloc_if_enabled(|| {
        realtime_alloc_guard::allow_alloc(|| {
            let _v = vec![0_u8; 8];
        });
    });
    realtime_alloc_guard::set_enabled(false);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn realtime_lock_guard_is_allocation_free() {
    use shoop_engine::realtime_lock_guard;

    let mutex = realtime_lock_guard::Mutex::new(0_u32);
    // Warm up platform mutex state before measuring the checked guard path.
    drop(mutex.lock().unwrap());
    realtime_lock_guard::set_enabled(true);
    assert_no_alloc(|| {
        realtime_lock_guard::forbid_locks_if_enabled(|| {
            let mut value =
                shoop_engine::realtime_allow_lock!("no-allocation test permission", mutex.lock())
                    .unwrap();
            *value += 1;
        });
    });
    realtime_lock_guard::set_enabled(false);
    assert_eq!(*mutex.lock().unwrap(), 1);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn snapshot_process_publication_is_allocation_free() {
    let runtime = ContentSnapshotRuntime::new();
    let (mut audio, _audio_control, _audio_reader) = runtime.create_audio_channel(8, 4);
    let (mut midi, _midi_control, _midi_reader) = runtime.create_midi_channel(4, 20);
    let mut midi_events = [MidiStorageElem::default(); 32];
    for (index, event) in midi_events.iter_mut().enumerate() {
        *event = MidiStorageElem::new(index as u32, &[0x90, index as u8, 100]).expect("valid MIDI");
    }
    let (mut audio_install, audio_control, _reader) = runtime.create_audio_channel(8, 2);
    let audio_prepared = audio_control
        .prepare(&[1.0, 2.0], ContentMutation::Loading)
        .expect("prepare audio");
    let (mut midi_install, midi_control, _reader) = runtime.create_midi_channel(4, 2);
    let midi_prepared = midi_control
        .prepare(
            &[shoop_engine::MidiEvent::new(1, vec![0x90, 60, 100])],
            4,
            ContentMutation::Loading,
        )
        .expect("prepare MIDI");

    assert_no_alloc(|| {
        assert!(audio.begin_mutation(ContentMutation::Recording));
        assert!(audio
            .publish_range(0, &[1.0, 2.0, 3.0, 4.0], 4, true)
            .is_some());
        audio.finish_mutation(false);

        assert!(midi.begin_mutation(ContentMutation::Recording));
        assert!(midi.append_state_events(&midi_events[..16], 32).is_some());
        assert!(midi.append_storage_events(&midi_events, 32, true).is_some());
        midi.finish_mutation(false);

        assert!(audio_install.install_prepared(audio_prepared));
        assert!(midi_install.install_prepared(midi_prepared));
    });
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn midi_replacement_is_allocation_free() {
    let mut channel =
        shoop_engine::midi_channel::MidiChannel::with_capacity_elems(64, ChannelMode::Direct);
    channel.set_contents(
        &[
            MidiStorageElem::new(1, &midi::note_on(0, 60, 100)).unwrap(),
            MidiStorageElem::new(2, &midi::note_off(0, 60, 0)).unwrap(),
        ],
        4,
        None,
    );
    channel.set_recording_buffer(4);
    channel.set_playback_buffer(4);
    let input = [
        MidiStorageElem::new(0, &midi::note_on(0, 64, 100)).unwrap(),
        MidiStorageElem::new(3, &midi::note_off(0, 64, 0)).unwrap(),
    ];
    let mut output = Vec::with_capacity(8);

    assert_no_alloc(|| {
        channel
            .process(
                LoopMode::Replacing,
                LoopMode::Unknown,
                None,
                None,
                4,
                0,
                4,
                4,
                &input,
                &mut output,
            )
            .unwrap();
    });
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn interrupting_midi_playback_is_allocation_free() {
    let mut channel =
        shoop_engine::midi_channel::MidiChannel::with_capacity_elems(1, ChannelMode::Direct);
    channel.set_contents(
        &[MidiStorageElem::new(0, &midi::note_on(0, 60, 100)).unwrap()],
        4,
        None,
    );
    let mut output = Vec::with_capacity(2);
    channel.set_playback_buffer(4);
    channel
        .process(
            LoopMode::Playing,
            LoopMode::Unknown,
            None,
            None,
            4,
            0,
            4,
            4,
            &[],
            &mut output,
        )
        .unwrap();
    output.clear();
    channel.set_playback_buffer(4);

    assert_no_alloc(|| {
        channel
            .process(
                LoopMode::Playing,
                LoopMode::Unknown,
                None,
                None,
                4,
                0,
                4,
                4,
                &[],
                &mut output,
            )
            .unwrap();
    });
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn midi_passthrough_cleanup_is_allocation_free() {
    let mut session = Session::default();
    let source = session.add_port(midi_port(1, "source", PortDirection::Input));
    let target = session.add_port(midi_port(2, "target", PortDirection::Output));
    session.connect_ports_internal(source, target).unwrap();
    session.apply_graph_changes().unwrap();
    session
        .port_mut(source)
        .unwrap()
        .as_dummy_midi_mut()
        .unwrap()
        .queue_msg(0, &midi::note_on(0, 60, 100));
    session.process(1);

    assert_no_alloc(|| {
        session
            .port_mut(source)
            .unwrap()
            .midi_mut()
            .unwrap()
            .set_passthrough_muted(true);
        session.process(1);
    });
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn snapshot_process_endpoint_retirement_defers_pooled_destruction() {
    let runtime = ContentSnapshotRuntime::new();
    let (audio, _audio_control, _audio_reader) = runtime.create_audio_channel(8, 4);
    let (midi, _midi_control, _midi_reader) = runtime.create_midi_channel(4, 4);

    assert_no_alloc(|| {
        drop(audio);
        drop(midi);
    });
}

fn audio_port(id: u64, name: &str, dir: PortDirection) -> Port {
    Port::Dummy(DummyAudioPort::new(PortId(id), name, dir, 4))
}

fn midi_port(id: u64, name: &str, dir: PortDirection) -> Port {
    Port::DummyMidi(DummyMidiPort::new(PortId(id), name, dir))
}

fn internal(name: &str, n: usize) -> Port {
    Port::Internal(InternalAudioPort::new(
        name,
        n,
        PortConnectability::INTERNAL,
        PortConnectability::INTERNAL,
        4,
    ))
}

/// Builds a session, runs a warm-up cycle, then asserts further cycles are
/// allocation-free.
///
/// The warm-up matters: the first cycle may still size buffers it could not size
/// earlier. What must never allocate is the steady state.
fn assert_steady_state_is_alloc_free(mut s: Session, n_frames: usize, cycles: usize) {
    s.apply_graph_changes().expect("graph should schedule");
    s.process(n_frames);

    assert_no_alloc(|| {
        for _ in 0..cycles {
            s.process(n_frames);
        }
    });
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn global_fx_active_inactive_full_restore_and_external_send_are_allocation_free() {
    let mut session = Session::default();
    session.set_buffer_size(4);
    session.set_test_fx_active("fx", false);
    let processor_midi = session.add_port(midi_port(40, "fx:midi_in", PortDirection::Input));
    let global = session.add_port(midi_port(41, "global:fx", PortDirection::Input));
    session
        .set_processor_ports("fx", vec![], vec![], vec![processor_midi])
        .unwrap();
    session.set_global_fx_midi_input(global).unwrap();
    for index in 0..shoop_plugin_protocol::MAX_MIDI_EVENTS_PER_BLOCK {
        let channel = (index / 120) as u8;
        let controller = (index % 120) as u8;
        session
            .port_mut(global)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(0, &midi::cc(channel, controller, controller));
    }
    session.apply_graph_changes().unwrap();
    assert_no_alloc(|| session.process(4));
    session.set_test_fx_active("fx", true);
    assert_no_alloc(|| {
        session.process(4);
        session.process(4);
    });

    let mut external = Session::default();
    external.set_buffer_size(4);
    external.set_external_processor("external");
    let send = external.add_port(Port::ExternalMidi(ExternalMidiPort::new(
        "external:send",
        PortDirection::Output,
    )));
    let global = external.add_port(midi_port(42, "global:fx", PortDirection::Input));
    external
        .set_processor_ports("external", vec![], vec![], vec![send])
        .unwrap();
    external.set_global_fx_midi_input(global).unwrap();
    external
        .port_mut(global)
        .unwrap()
        .as_dummy_midi_mut()
        .unwrap()
        .queue_msg(0, &midi::pitch_wheel(0, 4_096));
    external.apply_graph_changes().unwrap();
    assert_no_alloc(|| external.process(4));
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn test_and_external_processor_nodes_are_allocation_free() {
    let mut test = Session::default();
    test.set_buffer_size(4);
    let input = test.add_port(internal("test:audio_in_0", 4));
    let output = test.add_port(internal("test:audio_out_0", 4));
    test.set_test_fx_active("test", false);
    test.set_processor_ports("test", vec![input], vec![output], vec![])
        .unwrap();
    test.apply_graph_changes().unwrap();
    test.process(4);
    test.set_test_fx_active("test", true);
    assert_no_alloc(|| test.process(4));

    let mut external = Session::default();
    external.set_buffer_size(4);
    let return_ = external.add_port(Port::External(ExternalAudioPort::new(
        "return",
        PortDirection::Input,
        4,
    )));
    external.set_external_processor("external");
    external
        .set_processor_ports("external", vec![], vec![return_], vec![])
        .unwrap();
    external.apply_graph_changes().unwrap();
    external.process(4);
    external
        .port_mut(return_)
        .unwrap()
        .as_external_mut()
        .unwrap()
        .stage_input(&[1.0; 4]);
    assert_no_alloc(|| external.process(4));
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn composite_state_machine_does_not_allocate_or_free() {
    let source = LoopIdentity {
        slot: 10,
        generation: 1,
        kind: LoopTargetKind::Composite,
    };
    let child = LoopIdentity {
        slot: 20,
        generation: 3,
        kind: LoopTargetKind::Basic,
    };
    let replacement_child = LoopIdentity {
        slot: 30,
        generation: 2,
        kind: LoopTargetKind::Basic,
    };
    let catalog = LoopTargetCatalog::new(vec![
        LoopTargetMetadata {
            identity: source,
            length_samples: 0,
        },
        LoopTargetMetadata {
            identity: child,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: replacement_child,
            length_samples: 4,
        },
    ])
    .unwrap();
    let descriptor = CompositePlanDescriptor {
        source,
        sync_length: 4,
        timelines: vec![CompositeTimeline {
            sections: vec![
                CompositeSection {
                    entries: vec![CompositeEntry {
                        target: child,
                        delay: 0,
                        n_cycles: Some(2),
                        mode: None,
                    }],
                },
                CompositeSection {
                    entries: vec![CompositeEntry {
                        target: child,
                        delay: 0,
                        n_cycles: Some(1),
                        mode: None,
                    }],
                },
            ],
        }],
    };
    let plan =
        compile_composite_plan(&descriptor, &catalog, &[], CompositePlanLimits::default()).unwrap();
    let replacement_descriptor = CompositePlanDescriptor {
        source,
        sync_length: 4,
        timelines: vec![CompositeTimeline {
            sections: vec![CompositeSection {
                entries: vec![CompositeEntry {
                    target: replacement_child,
                    delay: 0,
                    n_cycles: Some(1),
                    mode: None,
                }],
            }],
        }],
    };
    let replacement = compile_composite_plan(
        &replacement_descriptor,
        &catalog,
        &[],
        CompositePlanLimits::default(),
    )
    .unwrap();
    let mut runtime = CompositeRuntime::new(&plan);
    let mut replacement_runtime = CompositeRuntime::new(&plan);

    assert_no_alloc(|| {
        runtime
            .transition_immediate(&plan, LoopMode::Playing, None, |_| true)
            .unwrap();
        runtime.sync_boundary(&plan, |_| true).unwrap();
        runtime.seek(&plan, 2, |_| true).unwrap();
        runtime.request_transition(LoopMode::Recording, 0).unwrap();
        runtime.sync_boundary(&plan, |_| true).unwrap();
        runtime.stop(&plan, |_| true).unwrap();
        assert_eq!(runtime.active_children().count(), 0);

        replacement_runtime
            .transition_immediate(&plan, LoopMode::Playing, Some(2), |_| true)
            .unwrap();
        replacement_runtime
            .activate_plan(&plan, &replacement, |_| true)
            .unwrap();
        replacement_runtime
            .activate_deferred_at_iteration_zero(&plan, &replacement, |_| true)
            .unwrap();
    });
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn composite_timeline_processing_does_not_allocate_or_free() {
    let mut session = Session::default();
    let sync = session.create_loop();
    let child = session.create_loop();
    session.loop_mut(sync).unwrap().set_length(4);
    session.loop_mut(child).unwrap().set_length(4);
    session.set_loop_mode(sync, LoopMode::Playing).unwrap();

    let source = LoopIdentity {
        slot: 10,
        generation: 1,
        kind: LoopTargetKind::Composite,
    };
    let sync_identity = session.loop_identity(sync).unwrap();
    let child_identity = session.loop_identity(child).unwrap();
    let catalog = LoopTargetCatalog::new(vec![
        LoopTargetMetadata {
            identity: source,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: sync_identity,
            length_samples: 4,
        },
        LoopTargetMetadata {
            identity: child_identity,
            length_samples: 4,
        },
    ])
    .unwrap();
    let descriptor = CompositePlanDescriptor {
        source,
        sync_length: 4,
        timelines: vec![CompositeTimeline {
            sections: vec![CompositeSection {
                entries: vec![CompositeEntry {
                    target: child_identity,
                    delay: 0,
                    n_cycles: Some(1),
                    mode: None,
                }],
            }],
        }],
    };
    let plan =
        compile_composite_plan(&descriptor, &catalog, &[], CompositePlanLimits::default()).unwrap();
    let mut replacement_descriptor = descriptor.clone();
    replacement_descriptor.timelines[0].sections[0].entries[0].n_cycles = Some(2);
    let replacement_plan = compile_composite_plan(
        &replacement_descriptor,
        &catalog,
        &[],
        CompositePlanLimits::default(),
    )
    .unwrap();
    let mut timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan,
            sync_source: sync_identity,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    timeline.prepare_install(1, &[None, None]).unwrap();
    let composite_state = std::sync::Arc::clone(timeline.state_mirror(source).unwrap());
    let mut replacement_timeline = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan: replacement_plan,
            sync_source: sync_identity,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    replacement_timeline
        .prepare_install(2, &[None, None])
        .unwrap();

    session
        .apply_graph_changes()
        .expect("graph should schedule");
    let (mut engine, mut handle) = shoop_engine::engine::split(session, 16);
    for _ in 0..4 {
        engine.process(4);
        handle.poll_trace();
    }
    let stale_timeline = timeline.clone();
    let mut install = handle.send_composite_timeline(timeline).unwrap();
    let mut stale_install = handle.send_composite_timeline(stale_timeline).unwrap();
    let mut accepted = handle
        .send_composite_control(
            source,
            BoundaryTargetAction::SetMode {
                mode: LoopMode::Playing,
                offset_samples: 0,
                retrigger: true,
            },
            None,
        )
        .unwrap();

    assert_no_alloc(|| {
        engine.process(4);
    });

    assert_eq!(install.pop().unwrap().unwrap().n_composites(), 0);
    let rejected = stale_install.pop().unwrap().unwrap_err();
    assert_eq!(
        rejected.error,
        shoop_engine::SessionError::StaleCompositeVersion(1)
    );
    assert_eq!(accepted.pop(), Ok(Ok(0)));

    let mut replacement = handle
        .send_composite_timeline(replacement_timeline)
        .unwrap();
    assert_no_alloc(|| {
        engine.process(4);
    });
    assert!(replacement.pop().unwrap().is_ok());
    assert_eq!(engine.session().composite_timeline().n_retired_plans(), 1);

    let mut reclaimed = handle.send_composite_plan_reclamation(64).unwrap();
    assert_no_alloc(|| {
        for _ in 0..8 {
            engine.process(4);
        }
    });
    assert_eq!(reclaimed.pop().unwrap().len(), 1);
    let snapshot = composite_state.read();
    assert!(snapshot.installed);
    assert_eq!(snapshot.mode, LoopMode::Playing);
    assert_eq!(snapshot.active_children().count(), 1);

    let mut stopped = handle
        .send_composite_control(
            source,
            BoundaryTargetAction::SetMode {
                mode: LoopMode::Stopped,
                offset_samples: 0,
                retrigger: true,
            },
            None,
        )
        .unwrap();
    assert_no_alloc(|| {
        engine.process(4);
    });
    assert!(stopped.pop().unwrap().is_ok());
    assert_eq!(
        engine.session().loop_(child).unwrap().mode(),
        LoopMode::Stopped
    );

    let mut restarted = handle
        .send_composite_control(
            source,
            BoundaryTargetAction::SetMode {
                mode: LoopMode::Playing,
                offset_samples: 0,
                retrigger: true,
            },
            None,
        )
        .unwrap();
    assert_no_alloc(|| {
        engine.process(4);
    });
    assert!(restarted.pop().unwrap().is_ok());

    let mut empty_timeline =
        CompositeBoundaryTimeline::new(Vec::new(), CompositeTimelineLimits::default()).unwrap();
    empty_timeline.prepare_install(3, &[None, None]).unwrap();
    let mut removed = handle.send_composite_timeline(empty_timeline).unwrap();
    assert_no_alloc(|| {
        engine.process(4);
    });
    let displaced = removed.pop().unwrap().unwrap();
    assert_eq!(displaced.n_composites(), 1);
    assert_eq!(engine.session().composite_timeline().n_composites(), 0);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn dense_composite_events_and_fail_closed_overflow_do_not_allocate() {
    let source = LoopIdentity {
        slot: 100,
        generation: 1,
        kind: LoopTargetKind::Composite,
    };
    let sync = LoopIdentity {
        slot: 0,
        generation: 1,
        kind: LoopTargetKind::Basic,
    };
    let children: Vec<_> = (1..=64)
        .map(|slot| LoopIdentity {
            slot,
            generation: 1,
            kind: LoopTargetKind::Basic,
        })
        .collect();
    let mut metadata = vec![LoopTargetMetadata {
        identity: source,
        length_samples: 4,
    }];
    metadata.push(LoopTargetMetadata {
        identity: sync,
        length_samples: 4,
    });
    metadata.extend(children.iter().copied().map(|identity| LoopTargetMetadata {
        identity,
        length_samples: 4,
    }));
    let catalog = LoopTargetCatalog::new(metadata).unwrap();
    let descriptor = CompositePlanDescriptor {
        source,
        sync_length: 4,
        timelines: vec![CompositeTimeline {
            sections: vec![CompositeSection {
                entries: children
                    .iter()
                    .copied()
                    .map(|target| CompositeEntry {
                        target,
                        delay: 0,
                        n_cycles: Some(1),
                        mode: Some(LoopMode::Playing),
                    })
                    .collect(),
            }],
        }],
    };
    let plan =
        compile_composite_plan(&descriptor, &catalog, &[], CompositePlanLimits::default()).unwrap();
    let mut dense = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan: plan.clone(),
            sync_source: sync,
        }],
        CompositeTimelineLimits::default(),
    )
    .unwrap();
    dense
        .queue_control(shoop_engine::AcceptedTimelineControl {
            at_sample: 0,
            target: source,
            action: BoundaryTargetAction::SetMode {
                mode: LoopMode::Playing,
                offset_samples: 0,
                retrigger: true,
            },
            acceptance_sequence: 0,
        })
        .unwrap();
    assert_no_alloc(|| {
        dense.resolve_boundary(&[], &[], |_| true).unwrap();
    });
    assert_eq!(dense.runtime(source).unwrap().active_children().count(), 64);

    let mut overflow = CompositeBoundaryTimeline::new(
        vec![CompositeTimelineNode {
            plan,
            sync_source: sync,
        }],
        CompositeTimelineLimits {
            max_primitive_events: 1,
            ..CompositeTimelineLimits::default()
        },
    )
    .unwrap();
    assert_no_alloc(|| {
        let error = overflow
            .resolve_boundary(&[sync, children[0]], &[], |_| true)
            .unwrap_err();
        assert_eq!(
            error.fault,
            shoop_engine::CompositeTimelineFault::PrimitiveEventCapacity
        );
    });
    assert_eq!(overflow.runtime(source).unwrap().mode(), LoopMode::Stopped);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn composite_callback_state_sources_are_structurally_lock_free() {
    for source in [
        include_str!("../src/composite_plan.rs"),
        include_str!("../src/composite_runtime.rs"),
        include_str!("../src/composite_timeline.rs"),
    ] {
        for forbidden in ["Mutex", "RwLock", ".lock("] {
            assert!(
                !source.contains(forbidden),
                "composite callback source contains lock primitive {forbidden}"
            );
        }
    }
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn transactional_audio_ringbuffer_adoption_does_not_allocate_or_partially_apply() {
    let mut session = Session::default();
    let input = session.add_port(audio_port(1, "in", PortDirection::Input));
    let first = session.create_loop();
    let second = session.create_loop();
    for loop_idx in [first, second] {
        let channel = session
            .add_audio_channel(loop_idx, 4, ChannelMode::Direct)
            .unwrap();
        session.connect_channel_input(channel, input).unwrap();
    }
    session.apply_graph_changes().unwrap();
    session
        .port_mut(input)
        .unwrap()
        .as_dummy_mut()
        .unwrap()
        .queue_data(&[0.1, 0.2, 0.3, 0.4]);
    session.process(4);

    let requests = [first, second].map(|loop_idx| AudioRingbufferAdoption {
        loop_idx,
        reverse_start_cycle: None,
        cycles_length: None,
        go_to_cycle: None,
        go_to_mode: LoopMode::Playing,
    });
    assert_no_alloc(|| {
        session.adopt_audio_ringbuffers(&requests).unwrap();
    });
    for loop_idx in [first, second] {
        let loop_ = session.loop_(loop_idx).unwrap();
        assert_eq!(loop_.mode(), LoopMode::Playing);
        assert_eq!(loop_.length(), 4);
        assert_eq!(
            loop_.audio_channel(0).unwrap().data(),
            vec![0.1, 0.2, 0.3, 0.4]
        );
    }

    let shape = session
        .describe_audio_ringbuffer_adoption(&requests)
        .unwrap();
    let shapes: Vec<_> = shape.channels().collect();
    let mut prepared: Vec<_> = shapes
        .iter()
        .map(|channel| PreparedAudioRingbufferAdoptionChannel {
            loop_idx: channel.loop_idx,
            channel_idx: channel.channel_idx,
            data: PreparedAudioChannelData::new(channel.chunk_size, channel.capacity),
        })
        .collect();
    let mut copies: Vec<_> = shapes
        .iter()
        .map(|channel| Vec::with_capacity(channel.capacity))
        .collect();
    assert_no_alloc(|| {
        session
            .adopt_audio_ringbuffers_prepared_with_copies(&requests, &mut prepared, &mut copies)
            .unwrap();
    });
    assert_eq!(copies, vec![vec![0.1, 0.2, 0.3, 0.4]; 2]);

    let duplicate = [requests[0], requests[0]];
    let before = session
        .loop_(first)
        .unwrap()
        .audio_channel(0)
        .unwrap()
        .data();
    assert_no_alloc(|| {
        assert_eq!(
            session.adopt_audio_ringbuffers(&duplicate),
            Err(shoop_engine::SessionError::AudioRingbufferAdoptionCapacity)
        );
    });
    assert_eq!(
        session
            .loop_(first)
            .unwrap()
            .audio_channel(0)
            .unwrap()
            .data(),
        before
    );
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn an_empty_session_does_not_allocate() {
    assert_steady_state_is_alloc_free(Session::default(), 4, 8);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn recording_audio_does_not_allocate() {
    let mut s = Session::default();
    let input = s.add_port(audio_port(1, "in", PortDirection::Input));
    let l = s.create_loop();
    let c = s.add_audio_channel(l, 64, ChannelMode::Direct).unwrap();
    s.connect_channel_input(c, input).unwrap();
    s.set_loop_mode(l, LoopMode::Recording).unwrap();

    // A chunk size that comfortably exceeds what the cycles need, so recording
    // growth does not have to reach for a new chunk mid-test.
    assert_steady_state_is_alloc_free(s, 4, 8);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn playing_audio_does_not_allocate() {
    let mut s = Session::default();
    let output = s.add_port(audio_port(1, "out", PortDirection::Output));
    let l = s.create_loop();
    let c = s.add_audio_channel(l, 64, ChannelMode::Direct).unwrap();
    s.connect_channel_output(c, output).unwrap();
    s.loop_mut(l)
        .unwrap()
        .audio_channel_mut(0)
        .unwrap()
        .load_data(&[0.1, 0.2, 0.3, 0.4]);
    s.loop_mut(l).unwrap().set_length(4);
    s.set_loop_mode(l, LoopMode::Playing).unwrap();

    assert_steady_state_is_alloc_free(s, 4, 8);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn a_full_audio_chain_does_not_allocate() {
    let mut s = Session::default();
    let input = s.add_port(audio_port(1, "in", PortDirection::Input));
    let mid = s.add_port(internal("mid", 4));
    let output = s.add_port(audio_port(2, "out", PortDirection::Output));
    s.connect_ports_internal(input, mid).unwrap();
    s.connect_ports_internal(mid, output).unwrap();

    let l = s.create_loop();
    let c = s.add_audio_channel(l, 64, ChannelMode::Direct).unwrap();
    s.connect_channel_input(c, mid).unwrap();
    s.connect_channel_output(c, output).unwrap();
    s.loop_mut(l).unwrap().set_length(4);
    s.set_loop_mode(l, LoopMode::Playing).unwrap();

    assert_steady_state_is_alloc_free(s, 4, 8);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn midi_routing_does_not_allocate() {
    let mut s = Session::default();
    let input = s.add_port(midi_port(1, "min", PortDirection::Input));
    let output = s.add_port(midi_port(2, "mout", PortDirection::Output));
    let l = s.create_loop();
    let c = s.add_midi_channel(l, 256, ChannelMode::Direct).unwrap();
    s.connect_channel_input(c, input).unwrap();
    s.connect_channel_output(c, output).unwrap();
    s.loop_mut(l).unwrap().set_length(8);
    s.set_loop_mode(l, LoopMode::Playing).unwrap();

    assert_steady_state_is_alloc_free(s, 4, 8);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn several_loops_and_channels_do_not_allocate() {
    let mut s = Session::default();
    let ain = s.add_port(audio_port(1, "ain", PortDirection::Input));
    let aout = s.add_port(audio_port(2, "aout", PortDirection::Output));
    let min = s.add_port(midi_port(3, "min", PortDirection::Input));

    for _ in 0..3 {
        let l = s.create_loop();
        let ac = s.add_audio_channel(l, 64, ChannelMode::Direct).unwrap();
        let mc = s.add_midi_channel(l, 256, ChannelMode::Direct).unwrap();
        s.connect_channel_input(ac, ain).unwrap();
        s.connect_channel_output(ac, aout).unwrap();
        s.connect_channel_input(mc, min).unwrap();
        s.loop_mut(l).unwrap().set_length(16);
        s.set_loop_mode(l, LoopMode::Playing).unwrap();
    }

    assert_steady_state_is_alloc_free(s, 4, 8);
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn feeding_queued_input_does_not_allocate_while_processing() {
    let mut s = Session::default();
    let input = s.add_port(audio_port(1, "in", PortDirection::Input));
    let l = s.create_loop();
    let c = s.add_audio_channel(l, 256, ChannelMode::Direct).unwrap();
    s.connect_channel_input(c, input).unwrap();
    s.set_loop_mode(l, LoopMode::Recording).unwrap();
    s.apply_graph_changes().unwrap();

    // Queue outside the guarded region: feeding a test port is not a realtime
    // operation, but consuming what was queued is.
    for _ in 0..10 {
        s.port_mut(input)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[0.5, 0.5, 0.5, 0.5]);
    }
    s.process(4);

    assert_no_alloc(|| {
        for _ in 0..8 {
            s.process(4);
        }
    });
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn queued_midi_is_consumed_without_allocating() {
    let mut s = Session::default();
    let input = s.add_port(midi_port(1, "min", PortDirection::Input));
    let l = s.create_loop();
    let c = s.add_midi_channel(l, 256, ChannelMode::Direct).unwrap();
    s.connect_channel_input(c, input).unwrap();
    s.set_loop_mode(l, LoopMode::Recording).unwrap();
    s.apply_graph_changes().unwrap();
    s.process(4);

    // Queue ahead of time, then consume it under the guard.
    let d = s.port_mut(input).unwrap().as_dummy_midi_mut().unwrap();
    for i in 0..8u32 {
        d.queue_msg(i, &midi::note_on(0, 60, 100));
    }

    assert_no_alloc(|| {
        for _ in 0..8 {
            s.process(4);
        }
    });
}

/// Recording long enough to grow past its first chunk, which used to allocate.
#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn recording_past_a_chunk_boundary_does_not_allocate() {
    let mut s = Session::default();
    let input = s.add_port(audio_port(1, "in", PortDirection::Input));
    let l = s.create_loop();
    // A deliberately small chunk, so 8 cycles of 4 frames cross several
    // boundaries and have to take chunks from the reserve.
    let c = s.add_audio_channel(l, 4, ChannelMode::Direct).unwrap();
    s.connect_channel_input(c, input).unwrap();
    s.set_loop_mode(l, LoopMode::Recording).unwrap();
    s.apply_graph_changes().unwrap();
    s.process(4);

    assert_no_alloc(|| {
        for _ in 0..8 {
            s.process(4);
        }
    });

    // Nine cycles of four frames, so the recording really did span chunks.
    assert_eq!(s.loop_(l).unwrap().length(), 36);
    let ch = s.loop_(l).unwrap().audio_channel(0).unwrap();
    assert!(ch.data().len() >= 36);
}

/// Playback state restoration emits a burst of messages in one cycle, so its
/// scratch buffer is the obvious place for a hidden reallocation.
#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn restoring_drifted_state_on_playback_does_not_allocate() {
    let mut s = Session::default();
    let input = s.add_port(midi_port(1, "min", PortDirection::Input));
    let output = s.add_port(midi_port(2, "mout", PortDirection::Output));
    let l = s.create_loop();
    let c = s.add_midi_channel(l, 4096, ChannelMode::Direct).unwrap();
    s.connect_channel_input(c, input).unwrap();
    s.connect_channel_output(c, output).unwrap();
    s.apply_graph_changes().unwrap();

    let d = s.port_mut(input).unwrap().as_dummy_midi_mut().unwrap();
    d.queue_msg(0, &midi::note_on(0, 60, 100));
    d.queue_msg(1, &midi::note_off(0, 60, 64));
    s.set_loop_mode(l, LoopMode::Recording).unwrap();
    s.process(4);

    // Move a lot of controllers on the input while stopped, so the restore has
    // plenty to revert.
    s.set_loop_mode(l, LoopMode::Stopped).unwrap();
    let d = s.port_mut(input).unwrap().as_dummy_midi_mut().unwrap();
    for ch in 0..16u8 {
        for cc in 0..40u8 {
            d.queue_msg(4, &midi::cc(ch, cc, 99));
        }
    }
    s.process(4);

    s.loop_mut(l).unwrap().set_length(4);
    s.set_loop_mode(l, LoopMode::Playing).unwrap();
    s.port_mut(output)
        .unwrap()
        .as_dummy_midi_mut()
        .unwrap()
        .request_data(4096)
        .unwrap();

    assert_no_alloc(|| {
        s.process(4);
    });

    // The restore really did fire, so the guard above was not vacuous.
    let got = s
        .port_mut(output)
        .unwrap()
        .as_dummy_midi_mut()
        .unwrap()
        .take_written_requested_msgs();
    assert!(
        got.len() > 100,
        "expected a large restore burst, got {}",
        got.len()
    );
}

/// Applying control commands must not allocate *or free* on the audio thread.
///
/// This is what the engine's return queue is for: running a boxed closure and then
/// dropping the box would free here, which `assert_no_alloc` treats exactly like an
/// allocation. Reverting the return queue to a plain drop fails this test.
#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn applying_commands_does_not_allocate() {
    use shoop_engine::engine::split;
    use shoop_engine::loop_mode::LoopMode;

    let mut s = Session::default();
    let input = s.add_port(midi_port(1, "min", PortDirection::Input));
    let l = s.create_loop();
    let c = s.add_midi_channel(l, 256, ChannelMode::Direct).unwrap();
    s.connect_channel_input(c, input).unwrap();
    s.loop_mut(l).unwrap().set_length(8);
    s.apply_graph_changes().unwrap();

    let (mut engine, mut handle) = split(s, 64);
    engine.process(4); // warm-up, as elsewhere

    // Queued outside the guarded region: building a command allocates, and that is
    // the control thread's business.
    let modes = [
        LoopMode::Playing,
        LoopMode::Stopped,
        LoopMode::Playing,
        LoopMode::Stopped,
    ];
    for m in modes {
        handle
            .send(Box::new(move |s: &mut Session| {
                let _ = s.set_loop_mode(0, m);
            }))
            .expect("queue has room");
    }

    assert_no_alloc(|| {
        for _ in 0..4 {
            engine.process(4);
        }
    });

    // The commands really did run, so the guard above was not vacuous.
    assert_eq!(
        engine
            .stats()
            .commands_applied
            .load(std::sync::atomic::Ordering::Relaxed),
        4
    );
    // And their boxes came back to be freed on this side.
    assert_eq!(handle.reclaim(), 4);
}

/// Publishing state must not allocate. The snapshot boxes are refilled and reused, and
/// the audio thread deliberately publishes a short snapshot rather than growing one.
/// The shape `send_and_wait` builds: a command that stores a result on its way out.
///
/// `send_and_wait` itself blocks, so it cannot be called from inside the guard. What
/// is checked here is the part that runs on the audio thread -- applying the closure
/// and handing the result over -- which is all the audio thread ever does for it.
#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn returning_a_result_from_a_command_does_not_allocate() {
    use shoop_engine::engine::split;

    let mut s = Session::default();
    s.create_loop();
    s.apply_graph_changes().unwrap();

    let (mut engine, mut handle) = split(s, 16);
    engine.process(4);

    // One response slot per command, allocated here on the control thread.
    let mut receivers = Vec::new();
    for _ in 0..4 {
        let (mut tx, rx) = rtrb::RingBuffer::<u32>::new(1);
        receivers.push(rx);
        handle
            .send(Box::new(move |s: &mut Session| {
                let _ = tx.push(s.n_loops() as u32);
            }))
            .expect("queue has room");
    }

    assert_no_alloc(|| {
        engine.process(4);
    });

    // Every command ran and left its answer behind.
    for mut rx in receivers {
        assert_eq!(rx.pop(), Ok(1));
    }
}

/// Resizing a loop is reachable from the audio thread, so it must not allocate there.
///
/// The clear button and the bar control both send `resize_loop` as a command, and it grows a
/// channel's storage to hold the new bar. That growth has to come from the refilling pool rather
/// than the allocator, which is exactly what this asserts: shrink and re-grow inside the guard,
/// having first taken the chunks so the pool is what supplies them.
#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires native allocation instrumentation",
    no_tracy = "measures allocation behavior without an outer capture"
)]
fn resizing_a_loop_from_the_audio_thread_does_not_allocate() {
    use shoop_engine::channel_mode::ChannelMode;
    use shoop_engine::engine::split;

    let mut s = Session::default();
    let l = s.create_loop();
    s.add_audio_channel(l, 256, ChannelMode::Direct).unwrap();
    s.apply_graph_changes().unwrap();
    // Sized for the largest bar up front, off the audio thread. This is what the GUI does at loop
    // creation, and it is what makes the resizes below shrink-and-regrow within storage that
    // already exists rather than growth the audio thread has to find memory for.
    const MAX_BAR: u32 = 8 * 48_000;
    s.resize_loop(l, MAX_BAR).unwrap();

    let (mut engine, mut handle) = split(s, 16);
    engine.process(4);

    // The full range the tempo control offers, in both directions.
    for length in [12_000u32, MAX_BAR, 24_000, MAX_BAR] {
        handle
            .send(Box::new(move |s: &mut Session| {
                s.resize_loop(0, length).unwrap();
            }))
            .expect("queue has room");
    }

    assert_no_alloc(|| {
        engine.process(4);
    });
}
