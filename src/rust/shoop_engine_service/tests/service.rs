#![forbid(unsafe_code)]
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    Error, Frame,
};
use shoop_engine_service::{
    dsp, prepare, Client, Config, DeliveryPhase, ExecutionStatus, LoopOperation, LoopReply, Phase,
    Prepared, PreparedRoute, Render, Report,
};
use shoop_loops::{LoopId, Mode, Timing, Transition};
use shoop_wasm_test_support::{heap, shoop_test};

#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

type Event<'a> = (u64, &'a [u8]);
fn id(slot: u32) -> dsp::PresetId {
    dsp::PresetId::from_parts(slot, 1).unwrap()
}
fn bank() -> Vec<dsp::Preset> {
    vec![
        dsp::Preset {
            id: id(0),
            live_gain: [1.; 2],
            playback_gain: [1.; 2],
            swap_channels: false,
            delay_frames: 0,
        },
        dsp::Preset {
            id: id(1),
            live_gain: [2., -1.],
            playback_gain: [0.5, 0.25],
            swap_channels: true,
            delay_frames: 0,
        },
        dsp::Preset {
            id: id(2),
            live_gain: [1.; 2],
            playback_gain: [0.; 2],
            swap_channels: false,
            delay_frames: 3,
        },
    ]
}
struct Host {
    render: Render,
    input: [MidiBuffer; 1],
    output: [MidiBuffer; 1],
}
struct Output {
    left: Vec<f32>,
    right: Vec<f32>,
    midi: Vec<(u64, Vec<u8>)>,
    recovery: Vec<Vec<u8>>,
    result: Result<Report, Error>,
}
fn setup() -> (Client, Host, PreparedRoute, LoopId) {
    setup_with_exports(4)
}
fn setup_with_exports(export_slots: usize) -> (Client, Host, PreparedRoute, LoopId) {
    let Prepared {
        client,
        render,
        route,
        target,
    } = prepare(
        Config {
            max_frames: 8,
            resource_pages: 128,
            export_slots,
            ..Config::default()
        },
        bank(),
        id(0),
    )
    .unwrap();
    (
        client,
        Host {
            render,
            input: [MidiBuffer::new(2048, 8192).unwrap()],
            output: [MidiBuffer::new(2048, 8192).unwrap()],
        },
        route,
        target,
    )
}
impl Host {
    fn step(&mut self, frames: usize, events: &[Event<'_>]) -> Output {
        let frame = self.render.next_frame();
        self.input[0].clear();
        for &(at, bytes) in events {
            if frame.0 <= at && at < frame.0 + frames as u64 {
                self.input[0]
                    .push((at - frame.0) as u32, frames, bytes)
                    .unwrap();
            }
        }
        let input: [[f32; 64]; 2] = std::array::from_fn(|channel| {
            std::array::from_fn(|i| {
                (frame.0 + i as u64 + 1) as f32 * if channel == 0 { 1. } else { 10. }
            })
        });
        let mut left = [0.; 64];
        let mut right = [0.; 64];
        let result = heap::assert_no_heap(|| {
            self.render.process(
                AudioBlock {
                    frame,
                    frames,
                    inputs: &[&input[0][..frames], &input[1][..frames]],
                    outputs: &mut [&mut left[..frames], &mut right[..frames]],
                },
                MidiBlock {
                    inputs: &self.input,
                    outputs: &mut self.output,
                },
            )
        });
        let midi = (0..self.output[0].len())
            .map(|i| {
                let (offset, packet) = self.output[0].get(i).unwrap();
                (frame.0 + offset as u64, packet.to_vec())
            })
            .collect();
        let recovery = (0..self.render.midi_recovery().len())
            .map(|i| self.render.midi_recovery().get(i).unwrap().1.to_vec())
            .collect();
        Output {
            left: left[..frames].to_vec(),
            right: right[..frames].to_vec(),
            midi,
            recovery,
            result,
        }
    }
}
fn cleanup(client: Client, host: Host) {
    cleanup_with_exports(client, host, 4);
}
fn cleanup_with_exports(client: Client, host: Host, exports: usize) {
    drop(client);
    let mut stopped = host.render.into_stopped();
    stopped.cleanup().unwrap();
    stopped.cleanup().unwrap();
    let usage = stopped.resource_usage();
    assert_eq!(usage.free_pages, 128);
    assert_eq!(usage.free_versions, 32);
    assert_eq!(usage.free_snapshots, 32);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(usage.retired_pages, 0);
    assert_eq!(usage.retired_versions, 0);
    assert_eq!(stopped.midi_index_usage().free_versions, 8);
    let usage = stopped.media_usage();
    assert_eq!(usage.free_media, 8);
    assert_eq!(usage.free_exports, exports);
    assert_eq!(usage.free_pins, 8);
    assert_eq!(usage.references, 0);
}
fn transition(
    client: &mut Client,
    target: LoopId,
    at: u64,
    transition: Transition,
) -> shoop_engine_service::Ticket {
    client
        .loop_at(
            Frame(at),
            LoopOperation::Transition {
                target,
                timing: Timing::At,
                transition,
            },
        )
        .unwrap()
}
fn select(
    client: &mut Client,
    at: u64,
    preset: dsp::PresetId,
    expected_revision: u32,
) -> shoop_engine_service::Ticket {
    client
        .dsp_at(
            Frame(at),
            dsp::Operation::SelectPreparedPreset {
                preset,
                expected_revision,
            },
        )
        .unwrap()
}
/// The DSP outcome of the selection submitted with `ticket`.
fn selection(
    executions: &[shoop_engine_service::Execution],
    ticket: &shoop_engine_service::Ticket,
) -> (Option<Frame>, shoop_engine_service::DspResult) {
    let execution = executions
        .iter()
        .find(|execution| execution.id == Some(ticket.id()))
        .expect("selection execution");
    let ExecutionStatus::Executed(raw) = &execution.status else {
        panic!("actual selection");
    };
    assert_eq!(raw.result, Ok(()));
    assert_eq!(raw.operation_result, None);
    (raw.delivered_frame, raw.processor_result.unwrap())
}
fn applied(frame: u64) -> shoop_engine_service::DspResult {
    Ok(dsp::Receipt {
        requested_frame: Frame(frame),
        frame: Frame(frame),
        previous_revision: 1,
        revision: 2,
        preset: id(1),
    })
}
fn drain(client: &mut Client) -> Vec<shoop_engine_service::Execution> {
    while let Some(delivery) = client.try_delivery() {
        assert_eq!(delivery.phase, DeliveryPhase::Runtime);
    }
    let mut executions = Vec::new();
    while let Some(execution) = client.try_execution() {
        executions.push(execution);
    }
    executions
}
fn reset_packets() -> Vec<Vec<u8>> {
    (0..16u8)
        .flat_map(|channel| [vec![0xB0 | channel, 120, 0], vec![0xB0 | channel, 123, 0]])
        .collect()
}

fn initial_restore(at: u64) -> Vec<(u64, Vec<u8>)> {
    let mut packets = Vec::new();
    for channel in 0..16u8 {
        // Canonical center pitch and released holds; only channel zero has a key.
        for bytes in [
            [0xE0 | channel, 0, 64],
            [0xB0 | channel, 64, 0],
            [0xB0 | channel, 69, 0],
            [0xB0 | channel, 66, 0],
        ] {
            packets.push((at, bytes.to_vec()));
        }
        if channel == 0 {
            packets.push((at, vec![0x90, 60, 100]));
        }
    }
    packets
}

#[shoop_test]
fn actual_loop_and_component_control_match_full_partition_oracles_without_warmup() {
    for parts in [vec![8], vec![1; 8], vec![2, 3, 3], vec![0, 3, 0, 2, 3]] {
        let (mut client, mut host, _route, target) = setup();
        let _record = transition(
            &mut client,
            target,
            1,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let state = client
            .loop_at(Frame(4), LoopOperation::State { target })
            .unwrap();
        let _play = transition(
            &mut client,
            target,
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        let preset = select(&mut client, 5, id(1), 1);
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut packets = Vec::new();
        for frames in parts {
            let out = host.step(frames, &[(0, &[0x90, 60, 100]), (2, &[0x80, 60, 45])]);
            out.result.unwrap();
            assert!(out.recovery.is_empty());
            left.extend(out.left);
            right.extend(out.right);
            packets.extend(out.midi);
        }
        let executions = drain(&mut client);
        assert_eq!(executions.len(), 4);
        assert_eq!(
            selection(&executions, &preset),
            (Some(Frame(5)), applied(5))
        );
        for execution in &executions {
            let ExecutionStatus::Executed(raw) = &execution.status else {
                panic!("actual execution");
            };
            assert_eq!(raw.result, Ok(()), "{raw:?}");
            if let Some(LoopReply::State(cut)) = raw.operation_result {
                assert_eq!(cut.capture_failure, None, "{cut:?}");
            }
            if let Some(LoopReply::Transition(completion)) = raw.operation_result {
                assert!(
                    matches!(completion.outcome, shoop_loops::Outcome::Applied { .. }),
                    "{completion:?}"
                );
            }
        }
        assert_eq!(left, [1., 2., 3., 4., 7., 135., 160., 170.]);
        assert_eq!(right, [10., 20., 30., 40., 70., -5.25, -6., -7.5]);
        let mut expected_packets = vec![(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])];
        expected_packets.extend(initial_restore(4));
        expected_packets.push((5, vec![0x80, 60, 45]));
        expected_packets.extend(initial_restore(7));
        assert_eq!(packets, expected_packets);
        let state = executions
            .into_iter()
            .find(|e| e.id == Some(state.id()))
            .unwrap();
        let ExecutionStatus::Executed(state) = state.status else {
            panic!("actual state execution");
        };
        assert_eq!(state.actual_frame, Frame(4));
        let Some(LoopReply::State(cut)) = state.operation_result else {
            panic!("settled state");
        };
        assert_eq!(cut.frame, Frame(4));
        assert_eq!(cut.state.mode, Mode::Playing);
        assert_eq!(cut.state.content_length, 3);
        assert_eq!(cut.graph_epoch.0, 1);
        assert_eq!(client.outstanding(), 0);
        assert_eq!(
            heap::assert_no_heap(|| client.meters().read_peaks()),
            [170., 70.]
        );
        assert_eq!(client.meters().read_peaks(), [0.; 2]);
        let _stop = client.stop().unwrap();
        let out = host.step(1, &[]);
        out.result.unwrap();
        assert_eq!(out.left, [180.]);
        assert_eq!(out.right, [-9.]);
        assert_eq!(out.midi, [(8, vec![0x80, 60, 0])]);
        assert!(out.recovery.is_empty());
        drain(&mut client);
        cleanup(client, host);
    }
}

#[shoop_test]
fn disconnected_stop_keeps_recovery_available_when_ui_holds_all_prior_results() {
    let (mut client, mut host, _route, target) = setup();
    let _record = transition(
        &mut client,
        target,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    let _play = transition(
        &mut client,
        target,
        3,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
    );
    host.step(8, &[]).result.unwrap();
    drain(&mut client);
    let mut pending = Vec::new();
    for at in 100..108 {
        pending.push(
            client
                .loop_at(Frame(at), LoopOperation::State { target })
                .unwrap(),
        );
    }
    assert_eq!(
        client
            .loop_at(Frame(200), LoopOperation::State { target })
            .unwrap_err()
            .error,
        Error::Full
    );
    host.step(8, &[]).result.unwrap();
    let _cancel = client.cancel_pending(&pending[0]).unwrap();
    host.step(8, &[]).result.unwrap();
    // Neither owning returns nor domain results are consumed, including recovery.
    drop(client);
    let out = host.step(1, &[]);
    let report = out.result.unwrap();
    assert_eq!(report.phase, Phase::Quiescent);
    assert_eq!(out.left, [0.]);
    assert_eq!(out.right, [0.]);
    assert!(out.midi.is_empty());
    assert_eq!(out.recovery, reset_packets());
    let mut stopped = host.render.into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.resource_usage().free_pages, 128);
    assert_eq!(stopped.resource_usage().free_versions, 32);
    assert_eq!(stopped.resource_usage().free_snapshots, 32);
    assert_eq!(stopped.media_usage().references, 0);
}

#[shoop_test]
fn processor_fault_keeps_actual_component_and_loop_receipts_without_replay() {
    for fault_at in [0, 2, 6] {
        let (mut client, mut host, _route, target) = setup();
        let _record = transition(
            &mut client,
            target,
            1,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let _play = transition(
            &mut client,
            target,
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        let state = client
            .loop_at(Frame(4), LoopOperation::State { target })
            .unwrap();
        let future = client
            .loop_at(Frame(100), LoopOperation::State { target })
            .unwrap();
        let preset = select(&mut client, 5, id(1), 1);
        let mut sysex = vec![1; 600];
        sysex[0] = 0xF0;
        sysex[599] = 0xF7;
        let out = host.step(8, &[(fault_at, &sysex)]);
        assert_eq!(out.result, Err(Error::Capacity));
        assert_eq!(out.left, [0.; 8]);
        assert_eq!(out.right, [0.; 8]);
        assert!(out.midi.is_empty());
        assert_eq!(out.recovery, reset_packets());
        assert_eq!(host.render.next_frame(), Frame(8));
        let executions = drain(&mut client);
        // A selection delivered before the fault keeps its receipt; otherwise the
        // service aborts it without fabricating a DSP outcome.
        if fault_at == 6 {
            assert_eq!(
                selection(&executions, &preset),
                (Some(Frame(5)), applied(5))
            );
        } else {
            assert!(executions.iter().any(|e| e.id == Some(preset.id())
                && matches!(
                    e.status,
                    ExecutionStatus::Aborted {
                        error: Error::Capacity,
                        ..
                    }
                )));
        }
        assert!(executions.iter().any(|e| e.id == Some(future.id())
            && matches!(
                e.status,
                ExecutionStatus::Aborted {
                    error: Error::Capacity,
                    ..
                }
            )));
        if fault_at == 6 {
            assert!(executions
                .iter()
                .any(|e| e.id == Some(state.id())
                    && matches!(e.status, ExecutionStatus::Executed(_))));
        }
        if fault_at == 2 {
            assert!(executions.iter().any(|e| matches!(e.status,
                ExecutionStatus::Executed(raw) if matches!(raw.operation_result,
                    Some(LoopReply::Transition(completion)) if matches!(completion.outcome,
                        shoop_loops::Outcome::Applied { frame: Frame(1), .. })))));
            assert!(executions.iter().any(|e| e.id == Some(state.id())
                && matches!(
                    e.status,
                    ExecutionStatus::Aborted {
                        error: Error::Capacity,
                        ..
                    }
                )));
        }
        let again = host.step(1, &[]);
        assert_eq!(again.result, Err(Error::Capacity));
        assert_eq!(host.render.next_frame(), Frame(8));
        cleanup(client, host);
    }
}

#[shoop_test]
fn zero_and_callback_end_requests_wait_for_a_real_callback() {
    let (mut client, mut host, _route, target) = setup();
    let _state = heap::assert_no_heap(|| {
        client
            .loop_at(Frame(8), LoopOperation::State { target })
            .unwrap()
    });
    let preset = heap::assert_no_heap(|| select(&mut client, 8, id(1), 1));
    assert_eq!(host.step(0, &[]).result.unwrap().next_frame, Frame(0));
    assert!(client.try_delivery().is_none());
    let out = host.step(8, &[]);
    out.result.unwrap();
    assert_eq!(out.left, [1., 2., 3., 4., 5., 6., 7., 8.]);
    assert!(client.try_delivery().is_some());
    assert!(client.try_execution().is_none());
    assert_eq!(
        host.step(0, &[]).result.unwrap().work,
        shoop_engine_service::Work::default()
    );
    assert!(client.try_execution().is_none());
    let out = host.step(1, &[]);
    out.result.unwrap();
    assert_eq!(out.left, [180.]);
    assert_eq!(out.right, [-9.]);
    let executions = drain(&mut client);
    assert_eq!(executions.len(), 2);
    let ExecutionStatus::Executed(state) = &executions
        .iter()
        .find(|e| e.id != Some(preset.id()))
        .unwrap()
        .status
    else {
        panic!("state");
    };
    assert_eq!(state.actual_frame, Frame(8));
    assert_eq!(
        selection(&executions, &preset),
        (Some(Frame(8)), applied(8))
    );
    cleanup(client, host);
}

#[shoop_test]
fn prepared_route_cut_keeps_capture_live_and_returns_immutable_prefix_bytes() {
    let (mut client, mut host, route, target) = setup();
    let _record = transition(
        &mut client,
        target,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    let _stage = heap::assert_no_heap(|| client.stage_route(route, rt_runtime::Epoch(1)).unwrap());
    let mut staged = None;
    for _ in 0..16 {
        let start = host.render.next_frame().0;
        let out = host.step(2, &[]);
        out.result.unwrap();
        assert_eq!(out.left, [(start + 1) as f32, (start + 2) as f32]);
        while let Some(delivery) = client.try_delivery() {
            assert_eq!(delivery.phase, DeliveryPhase::Runtime);
            if let Some(shoop_engine_service::EngineCommand::StageGraph { id, .. }) =
                delivery.command
            {
                staged = id;
            }
        }
        while client.try_execution().is_some() {}
        if staged.is_some() {
            break;
        }
    }
    let staged = staged.expect("bounded prepared graph validation");
    let begin = host.render.next_frame().0;
    let cut_at = begin + 1;
    let export_at = begin + 3;
    let state = client
        .loop_at(Frame(cut_at), LoopOperation::State { target })
        .unwrap();
    let _activate = client
        .loop_at(
            Frame(cut_at),
            LoopOperation::GraphActivate {
                id: staged,
                graph_epoch: rt_runtime::Epoch(1),
                topology_epoch: rt_runtime::Epoch(0),
            },
        )
        .unwrap();
    let preset = select(&mut client, cut_at, id(1), 1);
    let _export = client
        .loop_at(
            Frame(export_at),
            LoopOperation::Export {
                target,
                selection: shoop_loops::runtime::Selection::Recording,
            },
        )
        .unwrap();
    let out = host.step(4, &[]);
    out.result.unwrap();
    let expected_left = (begin..begin + 4)
        .map(|t| {
            if t < cut_at {
                (t + 1) as f32
            } else {
                20. * (t + 1) as f32
            }
        })
        .collect::<Vec<_>>();
    let expected_right = (begin..begin + 4)
        .map(|t| {
            if t < cut_at {
                10. * (t + 1) as f32
            } else {
                -((t + 1) as f32)
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(out.left, expected_left);
    assert_eq!(out.right, expected_right);
    let mut export = None;
    let mut state_seen = false;
    let executions = drain(&mut client);
    assert_eq!(
        selection(&executions, &preset),
        (Some(Frame(cut_at)), applied(cut_at))
    );
    for execution in executions {
        if execution.id == Some(preset.id()) {
            continue;
        }
        let ExecutionStatus::Executed(raw) = execution.status else {
            panic!("real route/export outcome");
        };
        assert_eq!(raw.result, Ok(()));
        match raw.operation_result {
            Some(LoopReply::State(cut)) if execution.id == Some(state.id()) => {
                assert_eq!(cut.frame, Frame(cut_at));
                assert_eq!(cut.graph_epoch.0, 2);
                assert_eq!(cut.topology_epoch.0, 0);
                assert_eq!(cut.state.mode, Mode::Recording);
                assert_eq!(cut.capture_failure, None);
                state_seen = true;
            }
            Some(LoopReply::GraphActivated {
                graph_epoch,
                topology_epoch,
                ..
            }) => {
                assert_eq!(raw.actual_frame, Frame(cut_at));
                assert_eq!(graph_epoch.0, 2);
                assert_eq!(topology_epoch.0, 0);
            }
            Some(LoopReply::Export { id, cut, media }) => {
                assert_eq!(cut.frame, Frame(export_at));
                assert_eq!(media.view.description.frames, export_at);
                assert_eq!(media.view.description.channels, 2);
                export = Some(id);
            }
            reply => panic!("unexpected route outcome: {reply:?}"),
        }
    }
    assert!(state_seen);
    let export = export.unwrap();
    let _cursor = client
        .loop_at(
            host.render.next_frame(),
            LoopOperation::Cursor {
                export,
                midi: false,
            },
        )
        .unwrap();
    host.step(1, &[]).result.unwrap();
    let mut cursor = None;
    for execution in drain(&mut client) {
        let ExecutionStatus::Executed(raw) = execution.status else {
            panic!("cursor");
        };
        if let Some(LoopReply::Cursor(value)) = raw.operation_result {
            cursor = Some(value);
        }
    }
    let buffer = vec![0; 256].into_boxed_slice();
    let _read = heap::assert_no_heap(|| client.read(cursor.unwrap(), buffer).unwrap());
    host.step(1, &[]).result.unwrap();
    let mut bytes = None;
    while let Some(delivery) = client.try_delivery() {
        let Some(shoop_engine_service::EngineCommand::Resource(command)) = delivery.command else {
            panic!("owning read");
        };
        let Some(rt_runtime::resource::Reply::Read { written, eof: true }) = command.reply() else {
            panic!("complete prefix");
        };
        let rt_runtime::resource::Request::Read { bytes: buffer, .. } = command.into_request()
        else {
            panic!("returned buffer");
        };
        bytes = Some(buffer[..written].to_vec());
    }
    let expected = (0..export_at)
        .flat_map(|t| [(t + 1) as f32, 10. * (t + 1) as f32])
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(bytes.unwrap(), expected);
    let _stop = client.stop().unwrap();
    host.step(1, &[]).result.unwrap();
    drain(&mut client);
    let _release = client
        .loop_at(host.render.next_frame(), LoopOperation::Release { export })
        .unwrap();
    host.step(1, &[]).result.unwrap();
    let released = drain(&mut client);
    assert!(released.iter().any(|e| matches!(e.status, ExecutionStatus::Executed(raw) if raw.operation_result == Some(LoopReply::Done))));
    assert_eq!(client.outstanding(), 0);
    cleanup(client, host);
}

#[shoop_test]
fn refusals_cancellation_and_late_delivery_preserve_requested_and_actual_phases() {
    let (mut client, mut host, route, target) = setup();
    let unsupported = heap::assert_no_heap(|| {
        client.loop_at(
            Frame(0),
            LoopOperation::Cancel {
                sequence: rt_runtime::Sequence(999),
            },
        )
    })
    .unwrap_err();
    assert_eq!(unsupported.error, Error::InvalidEvent);
    assert_eq!(client.outstanding(), 0);
    let invalid = client
        .loop_at(
            Frame(0),
            LoopOperation::State {
                target: LoopId::from_parts(0, 2).unwrap(),
            },
        )
        .unwrap();
    let cancelled = client
        .loop_at(Frame(0), LoopOperation::State { target })
        .unwrap();
    cancelled.cancel();
    host.step(2, &[]).result.unwrap();
    let mut cancellation_seen = false;
    while let Some(delivery) = client.try_delivery() {
        if delivery.id == cancelled.id() {
            assert_eq!(delivery.phase, DeliveryPhase::CancelledBeforeForward);
            assert_eq!(delivery.outcome, rt_runtime::transport::Outcome::Cancelled);
            assert!(delivery.terminal);
            assert!(delivery.request.is_some());
            cancellation_seen = true;
        }
    }
    assert!(cancellation_seen);
    let execution = client.try_execution().unwrap();
    assert_eq!(execution.id, Some(invalid.id()));
    let ExecutionStatus::Executed(raw) = execution.status else {
        panic!("typed refusal");
    };
    assert_eq!(
        raw.operation_result,
        Some(LoopReply::Failed(shoop_loops::runtime::Failure::Timeline(
            shoop_loops::Error::InvalidId
        )))
    );
    assert!(client.try_execution().is_none());
    assert_eq!(client.outstanding(), 0);
    let late = client
        .loop_at(Frame(0), LoopOperation::State { target })
        .unwrap();
    let stage = client.stage_route(route, rt_runtime::Epoch(2)).unwrap();
    let out = host.step(2, &[]);
    out.result.unwrap();
    assert_eq!(out.left, [3., 4.]);
    let mut refused = false;
    while let Some(delivery) = client.try_delivery() {
        if delivery.id == stage.id() {
            assert_eq!(
                delivery.outcome,
                rt_runtime::transport::Outcome::Rejected(Error::StaleEpoch)
            );
            assert!(delivery.terminal);
            assert!(matches!(
                delivery.command,
                Some(shoop_engine_service::EngineCommand::StageGraph { plan: Some(_), .. })
            ));
            refused = true;
        }
    }
    assert!(refused);
    let execution = client.try_execution().unwrap();
    assert_eq!(execution.id, Some(late.id()));
    let ExecutionStatus::Executed(raw) = execution.status else {
        panic!("late state");
    };
    assert_eq!(raw.requested_frame, Frame(0));
    assert_eq!(raw.delivered_frame, Some(Frame(2)));
    assert_eq!(raw.actual_frame, Frame(2));
    assert_eq!(raw.late_by(), 2);
    let Some(LoopReply::State(cut)) = raw.operation_result else {
        panic!("state");
    };
    assert_eq!(cut.frame, Frame(2));
    assert_eq!(cut.graph_epoch.0, 1);
    assert_eq!(cut.topology_epoch.0, 0);
    assert_eq!(client.outstanding(), 0);
    cleanup(client, host);
}

#[shoop_test]
fn retained_terminal_tickets_do_not_silently_recycle_cancellation_flags() {
    let (mut client, mut host, _route, target) = setup();
    let mut tickets = Vec::new();
    for _ in 0..shoop_engine_service::NORMAL_SLOTS {
        tickets.push(heap::assert_no_heap(|| {
            client
                .loop_at(host.render.next_frame(), LoopOperation::State { target })
                .unwrap()
        }));
        host.step(1, &[]).result.unwrap();
        drain(&mut client);
    }
    assert_eq!(client.outstanding(), 0);
    assert_eq!(
        heap::assert_no_heap(
            || client.loop_at(host.render.next_frame(), LoopOperation::State { target })
        )
        .unwrap_err()
        .error,
        Error::Busy
    );
    drop(tickets);
    let _ticket = client
        .loop_at(host.render.next_frame(), LoopOperation::State { target })
        .unwrap();
    host.step(1, &[]).result.unwrap();
    assert_eq!(drain(&mut client).len(), 1);
    cleanup(client, host);
}

#[shoop_test]
fn state_queries_and_meter_read_frequency_leave_complete_streams_unchanged() {
    let mut baseline = None;
    for reads in [0, 1, 64] {
        let (mut client, mut host, _route, target) = setup();
        let _record = transition(
            &mut client,
            target,
            1,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let _play = transition(
            &mut client,
            target,
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        let _preset = select(&mut client, 5, id(1), 1);
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut midi = Vec::new();
        let mut peaks = [0f32; 2];
        for frames in [2, 3, 3] {
            if reads != 0 {
                let _query = heap::assert_no_heap(|| {
                    client
                        .loop_at(host.render.next_frame(), LoopOperation::State { target })
                        .unwrap()
                });
            }
            let out = host.step(frames, &[(0, &[0x90, 60, 100]), (2, &[0x80, 60, 45])]);
            out.result.unwrap();
            assert!(out.recovery.is_empty());
            left.extend(out.left);
            right.extend(out.right);
            midi.extend(out.midi);
            for _ in 0..reads {
                let read = heap::assert_no_heap(|| client.meters().read_peaks());
                for channel in 0..2 {
                    peaks[channel] = peaks[channel].max(read[channel]);
                }
            }
            for execution in drain(&mut client) {
                let ExecutionStatus::Executed(raw) = execution.status else {
                    panic!("execution");
                };
                assert_eq!(raw.result, Ok(()));
                if let Some(LoopReply::State(cut)) = raw.operation_result {
                    assert_eq!(cut.capture_failure, None);
                }
            }
        }
        let read = client.meters().read_peaks();
        for channel in 0..2 {
            peaks[channel] = peaks[channel].max(read[channel]);
        }
        assert_eq!(peaks, [170., 70.]);
        let actual = (left, right, midi);
        if let Some(expected) = &baseline {
            assert_eq!(&actual, expected);
        } else {
            baseline = Some(actual);
        }
        cleanup(client, host);
    }
}

#[shoop_test]
fn foreign_tickets_and_routes_refuse_without_consuming_recovery_or_owner_credits() {
    let (mut first, one, route, target) = setup();
    let (mut second, two, _route, _) = setup();
    let ticket = first
        .loop_at(Frame(100), LoopOperation::State { target })
        .unwrap();
    assert_eq!(
        heap::assert_no_heap(|| second.cancel_pending(&ticket))
            .unwrap_err()
            .error,
        Error::InvalidId
    );
    let error =
        heap::assert_no_heap(|| second.stage_route(route, rt_runtime::Epoch(1))).unwrap_err();
    assert_eq!(error.error, Error::InvalidId);
    assert_eq!(second.outstanding(), 0);
    cleanup(first, one);
    cleanup(second, two);
}

fn empty_restore(at: u64) -> Vec<(u64, Vec<u8>)> {
    (0..16u8)
        .flat_map(|channel| {
            [
                (at, vec![0xE0 | channel, 0, 64]),
                (at, vec![0xB0 | channel, 64, 0]),
                (at, vec![0xB0 | channel, 69, 0]),
                (at, vec![0xB0 | channel, 66, 0]),
            ]
        })
        .collect()
}
fn playing_cursor() -> (Client, Host, rt_runtime::resource::Cursor) {
    let (mut client, mut host, _route, target) = setup();
    transition(
        &mut client,
        target,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    transition(
        &mut client,
        target,
        3,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
    );
    let exported = client
        .loop_at(
            Frame(3),
            LoopOperation::Export {
                target,
                selection: shoop_loops::runtime::Selection::Playback,
            },
        )
        .unwrap();
    host.step(4, &[]).result.unwrap();
    let export = drain(&mut client)
        .into_iter()
        .find_map(|execution| {
            if execution.id != Some(exported.id()) {
                return None;
            }
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("export");
            };
            let Some(LoopReply::Export { id, media, .. }) = raw.operation_result else {
                panic!("export");
            };
            assert_eq!(media.view.description.frames, 3);
            Some(id)
        })
        .unwrap();
    let cursor = client
        .loop_at(
            Frame(4),
            LoopOperation::Cursor {
                export,
                midi: false,
            },
        )
        .unwrap();
    host.step(1, &[]).result.unwrap();
    let cursor = drain(&mut client)
        .into_iter()
        .find_map(|execution| {
            if execution.id != Some(cursor.id()) {
                return None;
            }
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("cursor");
            };
            let Some(LoopReply::Cursor(cursor)) = raw.operation_result else {
                panic!("cursor");
            };
            Some(cursor)
        })
        .unwrap();
    (client, host, cursor)
}

#[shoop_test]
fn all_normal_credits_and_both_return_streams_can_stall_while_wraps_and_stop_progress() {
    for meter_reads in [0, 64] {
        let (mut client, mut host, cursor) = playing_cursor();
        let target = LoopId::from_parts(0, 1).unwrap();
        let mut tickets = Vec::with_capacity(17);
        for _ in 0..8 {
            tickets.push(heap::assert_no_heap(|| {
                client
                    .loop_at(Frame(5), LoopOperation::State { target })
                    .unwrap()
            }));
        }
        for _ in 0..8 {
            let bytes = vec![0; 256].into_boxed_slice();
            tickets.push(heap::assert_no_heap(|| client.read(cursor, bytes).unwrap()));
        }
        assert_eq!(client.outstanding(), 16);
        assert_eq!(
            heap::assert_no_heap(|| client.loop_at(Frame(5), LoopOperation::State { target }))
                .unwrap_err()
                .error,
            Error::Full
        );
        let bytes = vec![0; 256].into_boxed_slice();
        let refused = heap::assert_no_heap(|| client.read(cursor, bytes)).unwrap_err();
        assert_eq!(refused.error, Error::Full);
        let shoop_engine_service::Request::Read { bytes, .. } = refused.value else {
            panic!("original read owner");
        };
        assert_eq!(bytes.len(), 256);
        for _ in 0..4 {
            let begin = host.render.next_frame().0;
            let out = host.step(8, &[]);
            out.result.unwrap();
            assert!(out.recovery.is_empty());
            for i in 0..8 {
                let at = begin + i as u64;
                let expected = (at + 1 + (at - 3) % 3 + 1) as f32;
                assert_eq!(out.left[i], expected);
                assert_eq!(out.right[i], expected * 10.);
            }
            let expected: Vec<_> = (begin..begin + 8)
                .filter(|at| at % 3 == 0)
                .flat_map(empty_restore)
                .collect();
            assert_eq!(out.midi, expected);
            for _ in 0..meter_reads {
                heap::assert_no_heap(|| client.meters().read_peaks());
            }
            assert_eq!(client.outstanding(), 16);
        }
        let stop_at = host.render.next_frame();
        tickets.push(heap::assert_no_heap(|| client.stop().unwrap()));
        assert_eq!(client.stop().unwrap_err().error, Error::Full);
        let out = host.step(1, &[]);
        out.result.unwrap();
        assert_eq!(out.left, [(stop_at.0 + 1) as f32]);
        assert_eq!(out.right, [((stop_at.0 + 1) * 10) as f32]);
        assert!(out.midi.is_empty());
        assert!(out.recovery.is_empty());
        let mut held = Vec::with_capacity(17);
        let mut reads = 0;
        while let Some(delivery) = client.try_delivery() {
            assert_eq!(delivery.phase, DeliveryPhase::Runtime);
            assert!(!matches!(
                delivery.outcome,
                rt_runtime::transport::Outcome::Rejected(_)
            ));
            if let Some(shoop_engine_service::EngineCommand::Resource(command)) = &delivery.command
            {
                assert_eq!(
                    command.reply(),
                    Some(rt_runtime::resource::Reply::Read {
                        written: 24,
                        eof: true
                    })
                );
                reads += 1;
            }
            held.push(delivery);
        }
        assert_eq!(held.len(), 17);
        assert_eq!(reads, 8);
        let mut states = 0;
        let mut stopped = false;
        while let Some(execution) = client.try_execution() {
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("execution");
            };
            assert_eq!(raw.result, Ok(()));
            match raw.operation_result.unwrap() {
                LoopReply::State(cut) => {
                    assert_eq!(cut.frame, Frame(5));
                    assert_eq!(cut.state.mode, Mode::Playing);
                    states += 1;
                }
                LoopReply::Transition(completion) => {
                    assert!(
                        matches!(completion.outcome, shoop_loops::Outcome::Applied { frame, .. } if frame == stop_at)
                    );
                    stopped = true;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(states, 8);
        assert!(stopped);
        assert_eq!(client.outstanding(), 0);
        cleanup(client, host);
        drop((held, tickets));
    }
}

#[shoop_test]
fn cancel_recovery_executes_with_all_normal_credits_held_then_disconnect_reclaims_future_work() {
    let (mut client, mut host, cursor) = playing_cursor();
    let target = LoopId::from_parts(0, 1).unwrap();
    let mut tickets = Vec::with_capacity(16);
    for at in 1000..1008 {
        tickets.push(
            client
                .loop_at(Frame(at), LoopOperation::State { target })
                .unwrap(),
        );
    }
    for _ in 0..8 {
        let bytes = vec![0; 256].into_boxed_slice();
        tickets.push(client.read(cursor, bytes).unwrap());
    }
    host.step(8, &[]).result.unwrap();
    let cancelled = tickets[0].id();
    let recovery = heap::assert_no_heap(|| client.cancel_pending(&tickets[0]).unwrap());
    let at = host.render.next_frame();
    host.step(1, &[]).result.unwrap();
    let mut owners = Vec::new();
    while let Some(delivery) = client.try_delivery() {
        owners.push(delivery);
    }
    assert_eq!(owners.len(), 17);
    let mut original = false;
    let mut registration = false;
    while let Some(execution) = client.try_execution() {
        let ExecutionStatus::Executed(raw) = execution.status else {
            panic!("actual cancellation");
        };
        assert_eq!(raw.actual_frame, at);
        assert!(matches!(
            raw.operation_result,
            Some(LoopReply::Cancelled {
                delivered: false,
                ..
            })
        ));
        if execution.id == Some(cancelled) {
            assert_eq!(raw.requested_frame, Frame(1000));
            original = true;
        } else {
            assert_eq!(execution.id, Some(recovery.id()));
            assert_eq!(raw.result, Ok(()));
            registration = true;
        }
    }
    assert!(original && registration);
    assert_eq!(client.outstanding(), 7);
    drop(client);
    let out = host.step(1, &[]);
    assert_eq!(out.result.unwrap().phase, Phase::Quiescent);
    assert_eq!(out.left, [0.]);
    assert_eq!(out.right, [0.]);
    assert_eq!(out.recovery, reset_packets());
    let mut stopped = host.render.into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.resource_usage().free_pages, 128);
    assert_eq!(stopped.resource_usage().free_versions, 32);
    assert_eq!(stopped.resource_usage().free_snapshots, 32);
    assert_eq!(stopped.midi_index_usage().free_versions, 8);
    assert_eq!(stopped.media_usage().references, 0);
    drop((owners, tickets, recovery));
}

#[shoop_test]
fn saturated_exports_invalid_presets_and_route_rejection_preserve_capture_across_repeated_lifecycles(
) {
    for _ in 0..8 {
        let (mut client, mut host, route, target) = setup_with_exports(1);
        transition(
            &mut client,
            target,
            0,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let first = client
            .loop_at(
                Frame(2),
                LoopOperation::Export {
                    target,
                    selection: shoop_loops::runtime::Selection::Recording,
                },
            )
            .unwrap();
        let refused = client
            .loop_at(
                Frame(3),
                LoopOperation::Export {
                    target,
                    selection: shoop_loops::runtime::Selection::Recording,
                },
            )
            .unwrap();
        let state = client
            .loop_at(Frame(4), LoopOperation::State { target })
            .unwrap();
        let stage = client.stage_route(route, rt_runtime::Epoch(2)).unwrap();
        let invalid = dsp::PresetId::from_parts(0, 2).unwrap();
        let refused_preset = select(&mut client, 1, invalid, 1);
        let out = host.step(5, &[]);
        out.result.unwrap();
        assert_eq!(out.left, [1., 2., 3., 4., 5.]);
        assert_eq!(out.right, [10., 20., 30., 40., 50.]);
        let mut route_refused = false;
        while let Some(delivery) = client.try_delivery() {
            if delivery.id == stage.id() {
                assert_eq!(
                    delivery.outcome,
                    rt_runtime::transport::Outcome::Rejected(Error::StaleEpoch)
                );
                assert!(matches!(
                    delivery.command,
                    Some(shoop_engine_service::EngineCommand::StageGraph { plan: Some(_), .. })
                ));
                route_refused = true;
            }
        }
        assert!(route_refused);
        let mut export = None;
        let mut full = false;
        let mut recording = false;
        let mut invalid_refused = false;
        while let Some(execution) = client.try_execution() {
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("export execution");
            };
            if execution.id == Some(refused_preset.id()) {
                assert_eq!(
                    raw.processor_result,
                    Some(Err(dsp::Refusal::InvalidPreset { preset: invalid }))
                );
                invalid_refused = true;
            } else if execution.id == Some(first.id()) {
                let Some(LoopReply::Export { id, media, .. }) = raw.operation_result else {
                    panic!("prefix");
                };
                assert_eq!(media.view.description.frames, 2);
                export = Some(id);
            } else if execution.id == Some(refused.id()) {
                assert_eq!(
                    raw.operation_result,
                    Some(LoopReply::Failed(shoop_loops::runtime::Failure::Runtime(
                        Error::Capacity
                    )))
                );
                full = true;
            } else if execution.id == Some(state.id()) {
                let Some(LoopReply::State(cut)) = raw.operation_result else {
                    panic!("state");
                };
                assert_eq!(cut.state.mode, Mode::Recording);
                assert_eq!(cut.state.capture_frames, 4);
                assert_eq!(cut.capture_failure, None);
                recording = true;
            }
        }
        assert!(full && recording && invalid_refused);
        let export = export.unwrap();
        let cursor = client
            .loop_at(
                Frame(5),
                LoopOperation::Cursor {
                    export,
                    midi: false,
                },
            )
            .unwrap();
        host.step(1, &[]).result.unwrap();
        let cursor = drain(&mut client)
            .into_iter()
            .find_map(|execution| {
                if execution.id != Some(cursor.id()) {
                    return None;
                }
                let ExecutionStatus::Executed(raw) = execution.status else {
                    panic!("cursor");
                };
                let Some(LoopReply::Cursor(cursor)) = raw.operation_result else {
                    panic!("cursor");
                };
                Some(cursor)
            })
            .unwrap();
        let read = client
            .read(cursor, vec![0; 256].into_boxed_slice())
            .unwrap();
        host.step(1, &[]).result.unwrap();
        let mut bytes_seen = false;
        while let Some(delivery) = client.try_delivery() {
            if delivery.id == read.id() {
                let Some(shoop_engine_service::EngineCommand::Resource(command)) = delivery.command
                else {
                    panic!("read owner");
                };
                assert_eq!(
                    command.reply(),
                    Some(rt_runtime::resource::Reply::Read {
                        written: 16,
                        eof: true
                    })
                );
                let rt_runtime::resource::Request::Read { bytes, .. } = command.into_request()
                else {
                    panic!("read bytes");
                };
                let expected: Vec<u8> = [1f32, 10., 2., 20.]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect();
                assert_eq!(&bytes[..16], expected.as_slice());
                bytes_seen = true;
            }
        }
        assert!(bytes_seen);
        let release = client
            .loop_at(Frame(7), LoopOperation::Release { export })
            .unwrap();
        client.stop().unwrap();
        host.step(1, &[]).result.unwrap();
        assert!(drain(&mut client)
            .iter()
            .any(|execution| execution.id == Some(release.id())));
        assert_eq!(client.outstanding(), 0);
        cleanup_with_exports(client, host, 1);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test]
fn separate_control_thread_observes_exact_execution_while_render_never_waits() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};
    let (mut client, mut host, _route, target) = setup();
    transition(
        &mut client,
        target,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    transition(
        &mut client,
        target,
        3,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
    );
    client
        .loop_at(Frame(137), LoopOperation::State { target })
        .unwrap();
    transition(&mut client, target, 2047, Transition::Stop);
    let ready = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let control_ready = Arc::clone(&ready);
    let control_done = Arc::clone(&done);
    let control = std::thread::spawn(move || {
        let preset = heap::assert_no_heap(|| select(&mut client, 137, id(1), 1));
        control_ready.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut receipts = Vec::with_capacity(5);
        let mut reads = 0;
        loop {
            while let Some(delivery) = client.try_delivery() {
                assert!(!matches!(
                    delivery.outcome,
                    rt_runtime::transport::Outcome::Rejected(_)
                ));
            }
            while let Some(execution) = client.try_execution() {
                receipts.push(execution);
            }
            heap::assert_no_heap(|| client.meters().read_peaks());
            reads += 1;
            if control_done.load(Ordering::Acquire) && client.outstanding() == 0 {
                break;
            }
            assert!(Instant::now() < deadline, "bounded control-side completion");
            std::thread::yield_now();
        }
        assert_eq!(receipts.len(), 5);
        assert_eq!(
            selection(&receipts, &preset),
            (Some(Frame(137)), applied(137))
        );
        for execution in receipts {
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("actual receipt");
            };
            assert_eq!(raw.result, Ok(()));
            assert_eq!(raw.late_by(), 0);
            if let Some(LoopReply::State(cut)) = raw.operation_result {
                assert_eq!(cut.frame, Frame(137));
                assert_eq!(cut.state.mode, Mode::Playing);
                assert_eq!(cut.capture_failure, None);
            }
        }
        assert!(reads > 0);
        client
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    // Startup coordination occurs before the first guarded callback.
    while !ready.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    for _ in 0..256 {
        let begin = host.render.next_frame().0;
        let out = host.step(8, &[]);
        out.result.unwrap();
        assert!(out.recovery.is_empty());
        for i in 0..8 {
            let at = begin + i as u64;
            let playback = if (3..2047).contains(&at) {
                ((at - 3) % 3 + 1) as f32
            } else {
                0.
            };
            let live = (at + 1) as f32;
            let expected = if at < 137 {
                [live + playback, (live + playback) * 10.]
            } else {
                [live * 20. + playback * 5., -live + playback * 0.25]
            };
            assert_eq!([out.left[i], out.right[i]], expected);
        }
        let expected: Vec<_> = (begin..begin + 8)
            .filter(|at| (3..2047).contains(at) && at % 3 == 0)
            .flat_map(empty_restore)
            .collect();
        assert_eq!(out.midi, expected);
    }
    done.store(true, Ordering::Release);
    let client = control.join().unwrap();
    cleanup(client, host);
}

#[shoop_test]
fn disconnect_during_recording_retains_full_returns_and_pending_preset_until_stopped_cleanup() {
    let (mut client, mut host, _route, target) = setup();
    transition(
        &mut client,
        target,
        0,
        Transition::Record {
            monitor_previous: false,
        },
    );
    let exported = client
        .loop_at(
            Frame(2),
            LoopOperation::Export {
                target,
                selection: shoop_loops::runtime::Selection::Recording,
            },
        )
        .unwrap();
    host.step(4, &[]).result.unwrap();
    let export = drain(&mut client)
        .into_iter()
        .find_map(|execution| {
            if execution.id != Some(exported.id()) {
                return None;
            }
            let ExecutionStatus::Executed(raw) = execution.status else {
                panic!("prefix");
            };
            let Some(LoopReply::Export { id, .. }) = raw.operation_result else {
                panic!("prefix");
            };
            Some(id)
        })
        .unwrap();
    let cursor = client
        .loop_at(
            Frame(4),
            LoopOperation::Cursor {
                export,
                midi: false,
            },
        )
        .unwrap();
    let query = client
        .loop_at(Frame(4), LoopOperation::State { target })
        .unwrap();
    host.step(1, &[]).result.unwrap();
    let mut recording = false;
    let mut read_cursor = None;
    for execution in drain(&mut client) {
        let ExecutionStatus::Executed(raw) = execution.status else {
            panic!("state/cursor");
        };
        if execution.id == Some(query.id()) {
            let Some(LoopReply::State(cut)) = raw.operation_result else {
                panic!("state");
            };
            assert_eq!(cut.state.mode, Mode::Recording);
            assert_eq!(cut.state.capture_frames, 4);
            assert_eq!(cut.capture_failure, None);
            recording = true;
        } else if execution.id == Some(cursor.id()) {
            let Some(LoopReply::Cursor(cursor)) = raw.operation_result else {
                panic!("cursor");
            };
            read_cursor = Some(cursor);
        }
    }
    assert!(recording);
    drop((exported, cursor, query));
    let mut pending = Vec::with_capacity(17);
    for at in 1000..1008 {
        pending.push(
            client
                .loop_at(Frame(at), LoopOperation::State { target })
                .unwrap(),
        );
    }
    for _ in 0..8 {
        let bytes = vec![0; 256].into_boxed_slice();
        pending.push(client.read(read_cursor.unwrap(), bytes).unwrap());
    }
    pending.push(select(&mut client, 1000, id(1), 1));
    let out = host.step(1, &[]);
    out.result.unwrap();
    assert_eq!(out.left, [6.]);
    assert_eq!(out.right, [60.]);
    assert_eq!(client.outstanding(), 17);
    // Full owning returns and active capture are left undrained at disconnect.
    drop(client);
    let out = host.step(1, &[]);
    assert_eq!(out.result.unwrap().phase, Phase::Quiescent);
    assert_eq!(out.left, [0.]);
    assert_eq!(out.right, [0.]);
    assert!(out.midi.is_empty());
    assert_eq!(out.recovery, reset_packets());
    let mut stopped = host.render.into_stopped();
    stopped.cleanup().unwrap();
    stopped.cleanup().unwrap();
    let resources = stopped.resource_usage();
    assert_eq!(
        (
            resources.free_pages,
            resources.free_versions,
            resources.free_snapshots
        ),
        (128, 32, 32)
    );
    assert_eq!(
        (
            resources.reserved_pages,
            resources.retired_pages,
            resources.retired_versions
        ),
        (0, 0, 0)
    );
    assert_eq!(stopped.midi_index_usage().free_versions, 8);
    let media = stopped.media_usage();
    assert_eq!(
        (
            media.free_media,
            media.free_exports,
            media.free_pins,
            media.references
        ),
        (8, 4, 8, 0)
    );
    drop(pending);
}
