#![forbid(unsafe_code)]

use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, Control, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    planner::NoPlanner,
    processor::{OperationContext, ProcessBlock, Processor},
    resource::ResourceStore,
    schedule::{Action, Execution},
    transport::{Lane, Outcome, Ticket},
    Error, Frame, NodeId, Sequence,
};
use shoop_example_dsp::{
    prepare, Config, Dsp, Meters, Operation, Prepared, Preset, PresetId, Receipt, Refusal,
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

/// Fails every processing interval that starts at or after the given frame.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Fault {
    None,
    From(u64),
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
impl Processor<NoPlanner, Operation> for Checked {
    fn ports(&self) -> PortCounts {
        <Dsp as Processor<NoPlanner, Operation>>::ports(&self.dsp)
    }
    fn operation(
        &mut self,
        operation: Operation,
        context: OperationContext<'_>,
    ) -> Result<Result<Receipt, Refusal>, Error> {
        <Dsp as Processor<NoPlanner, Operation>>::operation(&mut self.dsp, operation, context)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        if matches!(self.fault, Fault::From(frame) if block.frame.0 >= frame) {
            return Err(Error::InvalidState);
        }
        self.dsp.process(block)
    }
}
type Selection = Execution<(), (), Operation, Result<Receipt, Refusal>>;
struct Fixture {
    control: Control<Checked, NoPlanner, Operation>,
    runtime: GraphRuntime<Checked, NoPlanner, Operation>,
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
fn node() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
fn setup(initial: u32, fault: Fault, events: usize) -> Fixture {
    let Prepared { processor, meters } = prepare(config(), bank(), id(initial)).unwrap();
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
    let mut graph = GraphDescription {
        nodes: vec![node()],
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
            let effect = Endpoint::node(node(), kind, port);
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
    let (mut control, runtime) = GraphRuntime::prepare_with_initial_graph(
        cfg,
        vec![PreparedNode::new(
            node(),
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
    control.prepare_submissions().unwrap();
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_eq!(runtime.resource_usage().free_pages, 8);
    Fixture {
        control,
        runtime,
        meters,
        input: std::array::from_fn(|_| MidiBuffer::new(32, 1024).unwrap()),
        output: [MidiBuffer::new(64, 2048).unwrap()],
        recovery: MidiBuffer::new(64, 2048).unwrap(),
    }
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
    fn select(&mut self, at: u64, preset: PresetId, expected_revision: u32) -> Ticket {
        self.submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(at),
                action: Action::Processor {
                    node: node(),
                    operation: Operation::SelectPreparedPreset {
                        preset,
                        expected_revision,
                    },
                },
                payload: Box::new([]),
            },
        )
    }
    fn submit(&mut self, lane: Lane, command: Command<Checked, NoPlanner, Operation>) -> Ticket {
        heap::assert_no_heap(|| self.control.submit_prepared(lane, command))
            .unwrap_or_else(|refusal| panic!("submit: {:?}", refusal.error))
    }
    fn admissions(&mut self) -> Vec<Outcome> {
        std::iter::from_fn(|| self.control.try_completion())
            .map(|completion| completion.outcome)
            .collect()
    }
    fn selections(&mut self) -> Vec<Selection> {
        std::iter::from_fn(|| self.control.try_execution()).collect()
    }
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
            let recovery = if frames == 0 {
                Ok(())
            } else {
                self.runtime
                    .drain_midi_recovery(0, &mut self.recovery, frames)
                    .map(|_| ())
            };
            result.and(recovery)
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
fn applied(frame: u64, previous_revision: u32, slot: u32) -> Option<Result<Receipt, Refusal>> {
    Some(Ok(Receipt {
        requested_frame: Frame(frame),
        frame: Frame(frame),
        previous_revision,
        revision: previous_revision + 1,
        preset: id(slot),
    }))
}
/// The single terminal selection result, with the frame it was delivered at.
fn only(host: &mut Fixture, sequence: Sequence) -> Selection {
    let mut selections = host.selections();
    assert_eq!(selections.len(), 1);
    let selection = selections.remove(0);
    assert_eq!(selection.sequence, sequence);
    assert_eq!(selection.operation_result, None);
    selection
}

#[shoop_test]
fn complete_scalar_and_packet_oracles_match_across_callback_partitions() {
    for parts in [vec![8, 8], vec![1; 16], vec![3, 2, 3, 8], vec![5, 0, 3, 8]] {
        let mut host = setup(0, Fault::None, 32);
        let ticket = host.select(5, id(1), 1);
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
        assert!(matches!(host.admissions()[..], [Outcome::Scheduled { .. }]));
        let selection = only(&mut host, ticket.sequence());
        assert_eq!(selection.result, Ok(()));
        assert_eq!(selection.delivered_frame, Some(Frame(5)));
        assert_eq!(selection.actual_frame, Frame(5));
        assert_eq!(selection.processor_result, applied(5, 1, 1));
        assert_eq!(host.control.outstanding(), 0);
    }
}

#[shoop_test]
fn first_callback_has_real_audio_and_interior_configuration_changes() {
    let mut host = setup(0, Fault::None, 32);
    let ticket = host.select(5, id(1), 1);
    let out = host.step(8, &[]);
    out.result.unwrap();
    assert_eq!(out.left, [2., 4., 6., 8., 6., -65., -84., -103.]);
    assert_eq!(out.right, [90., 82., 74., 66., 58., -5.5, -6.25, -7.]);
    assert_eq!(
        only(&mut host, ticket.sequence()).processor_result,
        applied(5, 1, 1)
    );
}

#[shoop_test]
fn prepared_delay_is_physical_and_same_preset_activation_resets_history() {
    for parts in [vec![8, 8], vec![1; 16], vec![2, 3, 3, 5, 3]] {
        let mut host = setup(2, Fault::None, 32);
        let ticket = host.select(5, id(2), 1);
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
            only(&mut host, ticket.sequence()).processor_result,
            applied(5, 1, 2)
        );
    }
}

#[shoop_test]
fn frame_zero_and_callback_end_activation_have_exact_nonempty_semantics() {
    for at in [0, 8] {
        let mut host = setup(0, Fault::None, 32);
        let ticket = host.select(at, id(1), 1);
        host.step(0, &[]).result.unwrap();
        assert!(host.selections().is_empty());
        let first = host.step(8, &[]);
        first.result.unwrap();
        for i in 0..8 {
            assert_eq!((first.left[i], first.right[i]), oracle(i as u64, at == 0));
        }
        if at == 8 {
            assert!(host.selections().is_empty());
            host.step(0, &[]).result.unwrap();
            assert!(host.selections().is_empty());
            let next = host.step(8, &[]);
            next.result.unwrap();
            for i in 0..8 {
                assert_eq!((next.left[i], next.right[i]), oracle(i as u64 + 8, true));
            }
        }
        assert_eq!(
            only(&mut host, ticket.sequence()).processor_result,
            applied(at, 1, 1)
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
        let mut host = setup(0, Fault::None, 32);
        host.step(8, &[]).result.unwrap();
        let ticket = host.select(at, preset, revision);
        let out = host.step(8, &[]);
        out.result.unwrap();
        for i in 0..8 {
            assert_eq!((out.left[i], out.right[i]), oracle(i as u64 + 8, false));
        }
        let selection = only(&mut host, ticket.sequence());
        assert_eq!(selection.result, Ok(()));
        assert_eq!(selection.processor_result, Some(Err(refusal)));
        // The refused selection left revision one in place.
        let ticket = host.select(16, id(1), 1);
        host.step(8, &[]).result.unwrap();
        host.admissions();
        assert_eq!(
            only(&mut host, ticket.sequence()).processor_result,
            applied(16, 1, 1)
        );
    }
}

#[shoop_test]
fn recovery_cancels_a_future_selection_before_delivery() {
    let mut host = setup(0, Fault::None, 32);
    let ticket = host.select(100, id(1), 1);
    host.step(8, &[]).result.unwrap();
    assert!(matches!(host.admissions()[..], [Outcome::Scheduled { .. }]));
    // Ticket cancellation is only observed before dequeue.
    ticket.cancel();
    host.submit(
        Lane::Recovery,
        Command::Cancel {
            sequence: ticket.sequence(),
        },
    );
    let out = host.step(8, &[]);
    out.result.unwrap();
    for i in 0..8 {
        assert_eq!((out.left[i], out.right[i]), oracle(i as u64 + 8, false));
    }
    assert_eq!(host.admissions(), [Outcome::Applied]);
    let selection = only(&mut host, ticket.sequence());
    assert_eq!(selection.result, Err(Error::Cancelled));
    assert_eq!(selection.delivered_frame, None);
    assert_eq!(selection.actual_frame, Frame(8));
    assert_eq!(selection.processor_result, None);
    let next = host.select(16, id(1), 1);
    host.step(8, &[]).result.unwrap();
    host.admissions();
    assert_eq!(
        only(&mut host, next.sequence()).processor_result,
        applied(16, 1, 1)
    );
}

#[shoop_test]
fn ticket_cancellation_before_dequeue_has_no_selection() {
    let mut host = setup(0, Fault::None, 32);
    let ticket = host.select(5, id(1), 1);
    ticket.cancel();
    let out = host.step(8, &[]);
    out.result.unwrap();
    for i in 0..8 {
        assert_eq!((out.left[i], out.right[i]), oracle(i as u64, false));
    }
    assert_eq!(host.admissions(), [Outcome::Cancelled]);
    assert!(host.selections().is_empty());
    assert_eq!(host.control.outstanding(), 0);
    drop(ticket);
    let next = host.select(8, id(1), 1);
    host.step(8, &[]).result.unwrap();
    host.admissions();
    assert_eq!(
        only(&mut host, next.sequence()).processor_result,
        applied(8, 1, 1)
    );
}

#[shoop_test]
fn delivered_selections_cannot_be_cancelled() {
    let mut host = setup(0, Fault::None, 32);
    let ticket = host.select(5, id(1), 1);
    host.step(8, &[]).result.unwrap();
    host.admissions();
    host.submit(
        Lane::Normal,
        Command::Cancel {
            sequence: ticket.sequence(),
        },
    );
    host.step(8, &[]).result.unwrap();
    assert_eq!(host.admissions(), [Outcome::Rejected(Error::InvalidState)]);
    assert_eq!(
        only(&mut host, ticket.sequence()).processor_result,
        applied(5, 1, 1)
    );
}

#[shoop_test]
fn peaks_accumulate_since_read_without_control_requests_or_dsp_changes() {
    let mut host = setup(0, Fault::None, 32);
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
    assert_eq!(host.control.outstanding(), 0);
    assert!(host.selections().is_empty());
}

#[shoop_test]
fn meter_read_frequency_changes_neither_samples_nor_packets() {
    let mut frequent = setup(0, Fault::None, 32);
    let mut infrequent = setup(0, Fault::None, 32);
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
    for (fault, published) in [(0, false), (5, true)] {
        let mut host = setup(0, Fault::From(fault), 32);
        let ticket = host.select(5, id(1), 1);
        assert_eq!(host.step(8, &[]).result, Err(Error::InvalidState));
        let selections = host.selections();
        if published {
            assert_eq!(selections.len(), 1);
            assert_eq!(selections[0].sequence, ticket.sequence());
            assert_eq!(selections[0].processor_result, applied(5, 1, 1));
        } else {
            assert!(selections.is_empty());
        }
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
    for (events, capacity, expected) in [
        (many.as_slice(), 32, Error::Capacity),
        (&[(0, 0, sysex.as_slice())][..], 32, Error::Capacity),
        (overflow.as_slice(), 8, Error::Full),
    ] {
        let mut host = setup(0, Fault::None, capacity);
        let ticket = host.select(5, id(1), 1);
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
            assert_eq!(
                only(&mut host, ticket.sequence()).processor_result,
                applied(5, 1, 1)
            );
        } else {
            // The interval before frame 5 failed, so the selection was never delivered.
            assert_eq!(out.result, Err(expected));
            assert!(host.selections().is_empty());
        }
    }
}

#[shoop_test]
fn disconnected_future_work_has_no_render_side_destruction() {
    let mut host = setup(0, Fault::None, 32);
    let ticket = host.select(100, id(1), 1);
    host.step(8, &[]).result.unwrap();
    drop(ticket);
    let Fixture {
        control,
        mut runtime,
        ..
    } = host;
    drop(control);
    let mut left = [0.; 8];
    let mut right = [0.; 8];
    let input = inputs(8);
    let midi_in = [
        MidiBuffer::new(1, 8).unwrap(),
        MidiBuffer::new(1, 8).unwrap(),
    ];
    let mut midi_out = [MidiBuffer::new(1, 8).unwrap()];
    heap::assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(8),
                frames: 8,
                inputs: &[&input[0], &input[1], &input[2], &input[3]],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &midi_in,
                outputs: &mut midi_out,
            },
        )
    })
    .unwrap();
    for i in 0..8 {
        assert_eq!((left[i], right[i]), oracle(i as u64 + 8, false));
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
    let mut host = setup(0, Fault::None, 32);
    let ticket = host.select(0, id(1), 1);
    let mut left = [0.; 9];
    let mut right = [0.; 9];
    let input = [0.; 9];
    assert_eq!(
        heap::assert_no_heap(|| host.runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 9,
                inputs: &[&input, &input, &input, &input],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &host.input,
                outputs: &mut host.output,
            },
        )),
        Err(Error::InvalidShape)
    );
    assert_eq!(host.runtime.next_frame(), Frame(0));
    host.step(8, &[]).result.unwrap();
    assert_eq!(
        only(&mut host, ticket.sequence()).processor_result,
        applied(0, 1, 1)
    );
}

#[shoop_test]
fn nonfinite_meter_saturation_does_not_modify_audio() {
    let mut host = setup(0, Fault::None, 32);
    let mut input = [[0.; 8]; 4];
    input[0][0] = f32::INFINITY;
    input[1][0] = f32::NAN;
    let out = host.with_inputs(8, &input, &[]);
    out.result.unwrap();
    assert_eq!(out.left[0], f32::INFINITY);
    assert!(out.right[0].is_nan());
    assert_eq!(host.meters.read_peaks(), [f32::MAX, 0.]);
}
