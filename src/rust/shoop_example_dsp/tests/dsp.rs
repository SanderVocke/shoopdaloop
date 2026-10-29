#![forbid(unsafe_code)]

use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Control, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    planner::NoPlanner,
    processor::{ProcessBlock, Processor},
    resource::ResourceStore,
    Error, Frame, NodeId,
};
use shoop_example_dsp::{
    prepare, Client, Config, Dsp, Meters, Prepared, Preset, PresetId, Receipt, Refusal,
    RenderControl,
};
use shoop_wasm_test_support::{heap, shoop_test};

#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

type Event<'a> = (u64, usize, &'a [u8]);
const EVENTS: &[Event<'static>] = &[
    (0, 0, &[0x90, 60, 100]),
    (0, 1, &[0x91, 62, 80]),
    (2, 0, &[0xB0, 1, 20]),
    (5, 0, &[0xB0, 7, 40]),
    (5, 0, &[0x90, 64, 88]),
    (5, 1, &[0x81, 62, 0]),
    (5, 1, &[0xC0, 4]),
    (6, 0, &[0xF0, 0x7D, 1, 2, 0xF7]),
    (8, 1, &[0x91, 65, 100]),
    (10, 0, &[0x80, 60, 0]),
    (15, 1, &[0x81, 65, 0]),
];
fn id(slot: u32) -> PresetId {
    PresetId::from_parts(slot, 7 + slot * 2).unwrap()
}
fn config() -> Config {
    Config {
        max_frames: 8,
        max_delay_frames: 8,
        midi_events_per_input: 16,
        packet_bytes: 32,
        ..Config::default()
    }
}
fn bank() -> Vec<Preset> {
    vec![
        Preset {
            id: id(0),
            live_gain: [1.; 2],
            playback_gain: [1.; 2],
            swap_channels: false,
            delay_frames: 0,
        },
        Preset {
            id: id(1),
            live_gain: [2., -1.],
            playback_gain: [0.5, 0.25],
            swap_channels: true,
            delay_frames: 0,
        },
        Preset {
            id: id(2),
            live_gain: [1.; 2],
            playback_gain: [0.; 2],
            swap_channels: false,
            delay_frames: 3,
        },
        Preset {
            id: id(3),
            live_gain: [0.; 2],
            playback_gain: [0.; 2],
            swap_channels: false,
            delay_frames: 0,
        },
    ]
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Fault {
    None,
    Before,
    After,
}
struct Checked {
    dsp: Dsp,
    fault: Fault,
}
impl Drop for Checked {
    fn drop(&mut self) {
        assert!(!heap::is_rendering());
    }
}
impl Processor for Checked {
    fn ports(&self) -> PortCounts {
        <Dsp as Processor>::ports(&self.dsp)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        if self.fault == Fault::Before {
            return Err(Error::InvalidState);
        }
        self.dsp.process(block)?;
        if self.fault == Fault::After {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
}
struct Fixture {
    _control: Control<Checked>,
    runtime: GraphRuntime<Checked>,
    render: RenderControl,
    meters: Meters,
    input: [MidiBuffer; 2],
    output: [MidiBuffer; 1],
    recovery: MidiBuffer,
}
struct Output {
    left: [f32; 8],
    right: [f32; 8],
    midi: Vec<(u64, Vec<u8>)>,
    recovery: Vec<(u64, Vec<u8>)>,
    result: Result<(), Error>,
}
fn setup(initial: u32, fault: Fault, events: usize) -> (Client, Fixture) {
    let Prepared {
        client,
        render,
        processor,
        meters,
    } = prepare(config(), bank(), id(initial)).unwrap();
    let cfg = rt_runtime::Config {
        max_frames: 8,
        audio_inputs: 4,
        audio_outputs: 2,
        midi_inputs: 2,
        midi_outputs: 1,
        max_nodes: 1,
        max_ports: 24,
        max_edges: 12,
        midi_events_per_port: events,
        midi_bytes_per_port: 1024,
        resource_slots: 4,
        resource_pages: 8,
        page_bytes: 64,
        ..rt_runtime::Config::default()
    }
    .prepare()
    .unwrap();
    let node = NodeId::from_parts(0, 1).unwrap();
    let mut graph = GraphDescription {
        nodes: vec![node],
        connections: Vec::new(),
    };
    for (kind, count, output) in [
        (PortType::Audio, 4, false),
        (PortType::Audio, 2, true),
        (PortType::Midi, 2, false),
        (PortType::Midi, 1, true),
    ] {
        for port in 0..count {
            let host = Endpoint::host(kind, port);
            let effect = Endpoint::node(node, kind, port);
            graph.connections.push(if output {
                Connection {
                    source: effect,
                    destination: host,
                }
            } else {
                Connection {
                    source: host,
                    destination: effect,
                }
            });
        }
    }
    let (control, runtime) = GraphRuntime::prepare_with_initial_graph(
        cfg,
        vec![PreparedNode::new(
            node,
            Checked {
                dsp: processor,
                fault,
            },
        )],
        NoPlanner,
        ResourceStore::new(cfg).unwrap(),
        &graph,
        &[],
    )
    .unwrap();
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_eq!(runtime.resource_usage().free_pages, 8);
    (
        client,
        Fixture {
            _control: control,
            runtime,
            render,
            meters,
            input: std::array::from_fn(|_| MidiBuffer::new(32, 1024).unwrap()),
            output: [MidiBuffer::new(64, 2048).unwrap()],
            recovery: MidiBuffer::new(64, 2048).unwrap(),
        },
    )
}
fn inputs(start: u64) -> [[f32; 8]; 4] {
    std::array::from_fn(|channel| {
        std::array::from_fn(|i| {
            let t = start + i as u64;
            match channel {
                0 => (t + 1) as f32,
                1 => -10. * (t + 1) as f32,
                2 => (t % 4 + 1) as f32,
                _ => 100. + t as f32 * 2.,
            }
        })
    })
}
impl Fixture {
    fn step(&mut self, frames: usize, events: &[Event<'_>]) -> Output {
        self.with_inputs(frames, &inputs(self.runtime.next_frame().0), events)
    }
    fn with_inputs(
        &mut self,
        frames: usize,
        input: &[[f32; 8]; 4],
        events: &[Event<'_>],
    ) -> Output {
        let frame = self.runtime.next_frame();
        for port in &mut self.input {
            port.clear();
        }
        self.output[0].clear();
        self.recovery.clear();
        for &(at, port, packet) in events {
            if frame.0 <= at && at < frame.0 + frames as u64 {
                self.input[port]
                    .push((at - frame.0) as u32, frames, packet)
                    .unwrap();
            }
        }
        let mut left = [0.; 8];
        let mut right = [0.; 8];
        let result = heap::assert_no_heap(|| {
            self.render.begin_callback(frame, frames)?;
            let result = self.runtime.process(
                AudioBlock {
                    frame,
                    frames,
                    inputs: &[
                        &input[0][..frames],
                        &input[1][..frames],
                        &input[2][..frames],
                        &input[3][..frames],
                    ],
                    outputs: &mut [&mut left[..frames], &mut right[..frames]],
                },
                MidiBlock {
                    inputs: &self.input,
                    outputs: &mut self.output,
                },
            );
            let completion = self.render.finish_callback(result);
            let recovery = if frames == 0 {
                Ok(())
            } else {
                self.runtime
                    .drain_midi_recovery(0, &mut self.recovery, frames)
                    .map(|_| ())
            };
            result.and(completion).and(recovery)
        });
        let midi = (0..self.output[0].len())
            .map(|i| {
                let (offset, packet) = self.output[0].get(i).unwrap();
                (frame.0 + offset as u64, packet.to_vec())
            })
            .collect();
        let recovery = (0..self.recovery.len())
            .map(|i| {
                let (offset, packet) = self.recovery.get(i).unwrap();
                (frame.0 + offset as u64, packet.to_vec())
            })
            .collect();
        Output {
            left,
            right,
            midi,
            recovery,
            result,
        }
    }
}
fn oracle(t: u64, alternate: bool) -> (f32, f32) {
    let live = (t + 1) as f32;
    let playback = (t % 4 + 1) as f32;
    if alternate {
        (
            -20. * live + (100. + 2. * t as f32) / 2.,
            -live + playback / 4.,
        )
    } else {
        (live + playback, 100. + 2. * t as f32 - live * 10.)
    }
}
fn applied(frame: u64, previous_revision: u32, slot: u32) -> Receipt {
    Receipt::Applied {
        requested_frame: Frame(frame),
        frame: Frame(frame),
        previous_revision,
        revision: previous_revision + 1,
        preset: id(slot),
    }
}

#[shoop_test]
fn complete_scalar_and_packet_oracles_match_across_callback_partitions() {
    for parts in [vec![8, 8], vec![1; 16], vec![3, 2, 3, 8], vec![5, 0, 3, 8]] {
        let (mut client, mut host) = setup(0, Fault::None, 32);
        let ticket = heap::assert_no_heap(|| client.select(Frame(5), id(1), 1)).unwrap();
        let mut t = 0;
        let mut packets = Vec::new();
        for frames in parts {
            let out = host.step(frames, EVENTS);
            out.result.unwrap();
            for i in 0..frames {
                assert_eq!(
                    (out.left[i], out.right[i]),
                    oracle(t + i as u64, t + i as u64 >= 5)
                );
            }
            packets.extend(out.midi);
            t += frames as u64;
        }
        assert_eq!(t, 16);
        assert_eq!(
            packets,
            EVENTS
                .iter()
                .map(|&(at, _, bytes)| (at, bytes.to_vec()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            client.select(Frame(16), id(0), 2).unwrap_err().error,
            Error::Full
        );
        let completion = heap::assert_no_heap(|| client.try_completion()).unwrap();
        assert_eq!(completion.sequence, ticket.sequence());
        assert_eq!(completion.request.result, Some(Ok(applied(5, 1, 1))));
        assert_eq!(host.render.revision(), 2);
        assert_eq!(client.outstanding(), 0);
    }
}

#[shoop_test]
fn first_callback_has_real_audio_and_interior_configuration_changes() {
    let (mut client, mut host) = setup(0, Fault::None, 32);
    let _ticket = client.select(Frame(5), id(1), 1).unwrap();
    let out = host.step(8, &[]);
    out.result.unwrap();
    assert_eq!(out.left, [2., 4., 6., 8., 6., -65., -84., -103.]);
    assert_eq!(out.right, [90., 82., 74., 66., 58., -5.5, -6.25, -7.]);
    assert_eq!(client.try_completion().unwrap().frame, Frame(0));
}

#[shoop_test]
fn prepared_delay_is_physical_and_same_preset_activation_resets_history() {
    for parts in [vec![8, 8], vec![1; 16], vec![2, 3, 3, 5, 3]] {
        let (mut client, mut host) = setup(2, Fault::None, 32);
        let _ticket = client.select(Frame(5), id(2), 1).unwrap();
        let mut t = 0;
        let mut packets = Vec::new();
        for frames in parts {
            let out = host.step(frames, EVENTS);
            out.result.unwrap();
            packets.extend(out.midi);
            for i in 0..frames {
                let at = t + i as u64;
                let value = if at < 3 || (5..8).contains(&at) {
                    0.
                } else {
                    (at - 2) as f32
                };
                assert_eq!(out.left[i], value);
                assert_eq!(out.right[i], -10. * value);
            }
            t += frames as u64;
        }
        assert_eq!(
            packets,
            EVENTS
                .iter()
                .map(|&(at, _, bytes)| (at, bytes.to_vec()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            client.try_completion().unwrap().request.result,
            Some(Ok(applied(5, 1, 2)))
        );
    }
}

#[shoop_test]
fn frame_zero_and_callback_end_activation_have_exact_nonempty_semantics() {
    for at in [0, 8] {
        let (mut client, mut host) = setup(0, Fault::None, 32);
        let _ticket = client.select(Frame(at), id(1), 1).unwrap();
        host.step(0, &[]).result.unwrap();
        assert_eq!(host.render.callbacks(), 0);
        assert!(client.try_completion().is_none());
        let first = host.step(8, &[]);
        first.result.unwrap();
        for i in 0..8 {
            assert_eq!((first.left[i], first.right[i]), oracle(i as u64, at == 0));
        }
        if at == 8 {
            assert!(client.try_completion().is_none());
            host.step(0, &[]).result.unwrap();
            assert_eq!(host.render.callbacks(), 1);
            let next = host.step(8, &[]);
            next.result.unwrap();
            for i in 0..8 {
                assert_eq!((next.left[i], next.right[i]), oracle(i as u64 + 8, true));
            }
        }
        assert_eq!(
            client.try_completion().unwrap().request.result,
            Some(Ok(applied(at, 1, 1)))
        );
    }
}

#[shoop_test]
fn refusal_details_preserve_requested_frames_and_do_not_change_state() {
    let bad = PresetId::from_parts(1, 10).unwrap();
    for (at, preset, revision, refusal) in [
        (8, bad, 1, Refusal::InvalidPreset { preset: bad }),
        (
            8,
            PresetId::from_parts(99, 7).unwrap(),
            1,
            Refusal::InvalidPreset {
                preset: PresetId::from_parts(99, 7).unwrap(),
            },
        ),
        (
            8,
            id(1),
            2,
            Refusal::StaleRevision {
                expected: 2,
                actual: 1,
            },
        ),
        (
            7,
            id(1),
            1,
            Refusal::Late {
                requested: Frame(7),
                decision_frame: Frame(8),
            },
        ),
    ] {
        let (mut client, mut host) = setup(0, Fault::None, 32);
        host.step(8, &[]).result.unwrap();
        let _ticket = client.select(Frame(at), preset, revision).unwrap();
        let out = host.step(8, &[]);
        out.result.unwrap();
        for i in 0..8 {
            assert_eq!((out.left[i], out.right[i]), oracle(i as u64 + 8, false));
        }
        assert_eq!(
            client.try_completion().unwrap().request.result,
            Some(Err(refusal))
        );
        assert_eq!(host.render.revision(), 1);
    }
}

#[shoop_test]
fn explicit_recovery_cancels_dequeued_work_with_normal_credit_full() {
    let (mut client, mut host) = setup(0, Fault::None, 32);
    let ticket = client.select(Frame(100), id(1), 1).unwrap();
    host.step(8, &[]).result.unwrap();
    ticket.cancel();
    assert!(client.try_completion().is_none());
    assert_eq!(
        client.select(Frame(16), id(1), 1).unwrap_err().error,
        Error::Full
    );
    let recovery = heap::assert_no_heap(|| client.cancel_pending(&ticket)).unwrap();
    let out = host.step(8, &[]);
    out.result.unwrap();
    for i in 0..8 {
        assert_eq!((out.left[i], out.right[i]), oracle(i as u64 + 8, false));
    }
    let cancel = client.try_completion().unwrap();
    assert_eq!(cancel.sequence, recovery.sequence());
    assert_eq!(
        cancel.request.result,
        Some(Ok(Receipt::CancelRegistered {
            frame: Frame(8),
            target: ticket.sequence()
        }))
    );
    assert_eq!(
        client.try_completion().unwrap().request.result,
        Some(Err(Refusal::Cancelled { frame: Frame(8) }))
    );
    assert_eq!(host.render.work_counters().completions, 2);
}

#[shoop_test]
fn cancelled_dequeue_and_retained_handles_never_reallocate_submission_flags() {
    let (mut client, mut host) = setup(0, Fault::None, 32);
    let ticket = client.select(Frame(5), id(1), 1).unwrap();
    ticket.cancel();
    host.step(8, &[]).result.unwrap();
    let completion = client.try_completion().unwrap();
    assert_eq!(
        completion.request.result,
        Some(Err(Refusal::Cancelled { frame: Frame(0) }))
    );
    assert_eq!(client.outstanding(), 0);
    assert_eq!(
        heap::assert_no_heap(|| client.select(Frame(8), id(1), 1))
            .unwrap_err()
            .error,
        Error::Busy
    );
    drop(ticket);
    drop(completion);
    let _next = heap::assert_no_heap(|| client.select(Frame(8), id(1), 1)).unwrap();
    host.step(8, &[]).result.unwrap();
    assert_eq!(
        client.try_completion().unwrap().request.result,
        Some(Ok(applied(8, 1, 1)))
    );
}

#[shoop_test]
fn foreign_component_tickets_are_refused_before_recovery_admission() {
    let (mut client, mut host) = setup(0, Fault::None, 32);
    let (mut other, _other_host) = setup(0, Fault::None, 32);
    let ticket = other.select(Frame(5), id(1), 1).unwrap();
    assert_eq!(
        client.cancel_pending(&ticket).unwrap_err().error,
        Error::InvalidId
    );
    assert_eq!(client.outstanding(), 0);
    host.step(8, &[]).result.unwrap();
    assert!(client.try_completion().is_none());
}

#[shoop_test]
fn peaks_accumulate_since_read_without_control_requests_or_dsp_changes() {
    let (client, mut host) = setup(0, Fault::None, 32);
    assert_eq!(host.meters.read_peaks(), [0.; 2]);
    host.step(8, &[]).result.unwrap();
    assert_eq!(
        heap::assert_no_heap(|| host.meters.read_peaks()),
        [12., 90.]
    );
    assert_eq!(host.meters.read_peak(2), None);
    assert_eq!(host.meters.read_peaks(), [0.; 2]);
    host.step(8, &[]).result.unwrap();
    assert_eq!(host.meters.read_peak(0), Some(20.));
    assert_eq!(host.meters.read_peak(1), Some(30.));
    assert_eq!(client.outstanding(), 0);
    assert_eq!(host.render.work_counters().operations, 0);
}

#[shoop_test]
fn meter_read_frequency_changes_neither_samples_nor_packets() {
    let (_client, mut frequent) = setup(0, Fault::None, 32);
    let (_other, mut infrequent) = setup(0, Fault::None, 32);
    let mut peaks = [0f32; 2];
    for _ in 0..16 {
        let a = frequent.step(1, EVENTS);
        let b = infrequent.step(1, EVENTS);
        a.result.unwrap();
        b.result.unwrap();
        assert_eq!(a.left, b.left);
        assert_eq!(a.right, b.right);
        assert_eq!(a.midi, b.midi);
        for (peak, value) in peaks.iter_mut().zip(frequent.meters.read_peaks()) {
            *peak = peak.max(value);
        }
    }
    assert_eq!(infrequent.meters.read_peaks(), peaks);
}

#[shoop_test]
fn faulted_callbacks_preserve_actual_preset_publication_without_replay() {
    for fault in [Fault::Before, Fault::After] {
        let (mut client, mut host) = setup(0, fault, 32);
        let _ticket = client.select(Frame(5), id(1), 1).unwrap();
        assert_eq!(host.step(8, &[]).result, Err(Error::InvalidState));
        let result = client.try_completion().unwrap().request.result;
        assert_eq!(
            result,
            Some(if fault == Fault::After {
                Ok(applied(5, 1, 1))
            } else {
                Err(Refusal::Fault {
                    decision_frame: Frame(0),
                    error: Error::InvalidState,
                })
            })
        );
        assert_eq!(
            host.render.revision(),
            if fault == Fault::After { 2 } else { 1 }
        );
    }
}

#[shoop_test]
fn packet_event_and_output_capacity_errors_are_guarded_and_truthful() {
    let packet = [0x90, 60, 90];
    let many = (0..17)
        .map(|_| (0, 0, packet.as_slice()))
        .collect::<Vec<_>>();
    let mut sysex = [1u8; 34];
    sysex[0] = 0xF0;
    sysex[1] = 0x7D;
    sysex[33] = 0xF7;
    let packet_bytes = packet.as_slice();
    let overflow = (0..2)
        .flat_map(|port| (0..8).map(move |_| (0, port, packet_bytes)))
        .collect::<Vec<_>>();
    for (events, capacity, expected, activated) in [
        (many.as_slice(), 32, Error::Capacity, false),
        (&[(0, 0, sysex.as_slice())][..], 32, Error::Capacity, true),
        (overflow.as_slice(), 8, Error::Full, true),
    ] {
        let (mut client, mut host) = setup(0, Fault::None, capacity);
        let _ticket = client.select(Frame(5), id(1), 1).unwrap();
        let out = host.step(8, events);
        if expected == Error::Full {
            assert_eq!(out.result, Ok(()));
            assert!(host.runtime.diagnostics().midi_overflows > 0);
            let reset = (0..16u8)
                .flat_map(|channel| {
                    [
                        (0, vec![0xB0 | channel, 120, 0]),
                        (0, vec![0xB0 | channel, 123, 0]),
                    ]
                })
                .collect::<Vec<_>>();
            assert!(out.midi.is_empty());
            assert_eq!(out.recovery, reset);
            for i in 0..8 {
                assert_eq!((out.left[i], out.right[i]), oracle(i as u64, i >= 5));
            }
        } else {
            assert_eq!(out.result, Err(expected));
        }
        assert_eq!(
            client.try_completion().unwrap().request.result,
            Some(if activated {
                Ok(applied(5, 1, 1))
            } else {
                Err(Refusal::Fault {
                    decision_frame: Frame(0),
                    error: expected,
                })
            })
        );
    }
}

#[shoop_test]
fn disconnected_future_work_has_no_render_side_destruction() {
    let (mut client, mut host) = setup(0, Fault::None, 32);
    let ticket = client.select(Frame(100), id(1), 1).unwrap();
    host.step(8, &[]).result.unwrap();
    drop(ticket);
    drop(client);
    let out = host.step(8, &[]);
    out.result.unwrap();
    assert!(!host.render.connected());
    for i in 0..8 {
        assert_eq!((out.left[i], out.right[i]), oracle(i as u64 + 8, false));
    }
}

#[shoop_test]
fn preparation_and_callback_limits_refuse_before_consuming_requests() {
    assert_eq!(PresetId::from_parts(0, 0), Err(Error::InvalidId));
    for cfg in [
        Config {
            max_frames: 0,
            ..config()
        },
        Config {
            max_presets: 0,
            ..config()
        },
    ] {
        assert_eq!(
            prepare(cfg, bank(), id(0)).err(),
            Some(Error::InvalidConfig)
        );
    }
    for cfg in [
        Config {
            max_delay_frames: usize::MAX,
            ..config()
        },
        Config {
            max_presets: usize::MAX,
            ..config()
        },
        Config {
            packet_bytes: usize::MAX,
            ..config()
        },
    ] {
        assert_eq!(prepare(cfg, bank(), id(0)).err(), Some(Error::Capacity));
    }
    let mut presets = bank();
    presets[0].live_gain[0] = f32::NAN;
    assert_eq!(
        prepare(config(), presets, id(0)).err(),
        Some(Error::InvalidParameter)
    );
    assert_eq!(
        prepare(config(), bank(), PresetId::from_parts(0, 1).unwrap()).err(),
        Some(Error::InvalidId)
    );
    let (mut client, mut host) = setup(0, Fault::None, 32);
    let _ticket = client.select(Frame(0), id(1), 1).unwrap();
    assert_eq!(
        heap::assert_no_heap(|| host.render.begin_callback(Frame(0), 9)),
        Err(Error::InvalidShape)
    );
    assert_eq!(
        heap::assert_no_heap(|| host.render.begin_callback(Frame(u64::MAX - 1), 8)),
        Err(Error::Capacity)
    );
    assert_eq!(host.render.callbacks(), 0);
    host.step(8, &[]).result.unwrap();
    assert_eq!(
        client.try_completion().unwrap().request.result,
        Some(Ok(applied(0, 1, 1)))
    );
}

#[shoop_test]
fn nonfinite_meter_saturation_does_not_modify_audio() {
    let (_client, mut host) = setup(0, Fault::None, 32);
    let mut input = [[0.; 8]; 4];
    input[0][0] = f32::INFINITY;
    input[1][0] = f32::NAN;
    let out = host.with_inputs(8, &input, &[]);
    out.result.unwrap();
    assert_eq!(out.left[0], f32::INFINITY);
    assert!(out.right[0].is_nan());
    assert_eq!(host.meters.read_peaks(), [f32::MAX, 0.]);
}
