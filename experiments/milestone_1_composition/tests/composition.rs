#![forbid(unsafe_code)]

use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    processor::{ProcessBlock, Processor},
    resource::{CaptureGroupConfig, ResourceStore},
    transport::Lane,
    Budgets, Config, Error, Frame, NodeId, PlannerId,
};
use shoop_loop_engine::{
    audio::Smoothing,
    compile,
    runtime::{Binding, Controller, Engine, LoopPlanner, Operation, Settings},
    Limits, LoopId, LoopKind, LoopSpec, Timing, TopologyDescription, Transition,
};
use shoop_m1_spike_dsp::{prepare, Client, Dsp, PresetId, RenderControl, Reply};
use shoop_wasm_test_support::{heap, shoop_test};

#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;

struct CheckedDsp(Dsp);
impl Drop for CheckedDsp {
    fn drop(&mut self) {
        assert!(!heap::is_rendering());
    }
}
impl Processor<LoopPlanner> for CheckedDsp {
    fn ports(&self) -> PortCounts {
        <Dsp as Processor<LoopPlanner>>::ports(&self.0)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.0.process(block)
    }
}

struct Fixture {
    control: Controller<CheckedDsp>,
    engine: Engine<CheckedDsp>,
    render: RenderControl,
}

fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn preset() -> PresetId {
    PresetId {
        slot: 1,
        generation: 1,
    }
}

fn setup() -> (Client, Fixture) {
    let (client, render, dsp) = prepare().unwrap();
    let config = Config {
        max_frames: 8,
        max_nodes: 4,
        max_ports: 16,
        max_edges: 16,
        midi_inputs: 0,
        midi_outputs: 0,
        resource_slots: 16,
        resource_pages: 128,
        page_bytes: 64,
        command_slots: 8,
        scheduled_events: 8,
        budgets: Budgets {
            commands: 8,
            events: 8,
            subranges: 0,
            transfer_bytes: 64,
            validation_steps: 128,
            publications: 8,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let target = LoopId::from_parts(0, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 1,
            pending_requests: 5,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: target,
                kind: LoopKind::Primitive,
                length: 0,
                sync_source: None,
            }],
            ..TopologyDescription::default()
        },
    )
    .unwrap();
    let resources = ResourceStore::with_capture_groups(
        config,
        None,
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 256,
        },
    )
    .unwrap();
    let mut graph = GraphDescription {
        nodes: vec![node(0), node(2), node(1)],
        connections: Vec::new(),
    };
    for port in 0..2 {
        graph.connections.extend([
            Connection {
                source: Endpoint::host(PortType::Audio, port),
                destination: Endpoint::node(node(2), PortType::Audio, port),
            },
            Connection {
                source: Endpoint::node(node(0), PortType::Audio, port),
                destination: Endpoint::node(node(2), PortType::Audio, port + 2),
            },
            Connection {
                source: Endpoint::node(node(2), PortType::Audio, port),
                destination: Endpoint::node(node(1), PortType::Audio, port),
            },
            Connection {
                source: Endpoint::node(node(2), PortType::Audio, port),
                destination: Endpoint::host(PortType::Audio, port),
            },
        ]);
    }
    let (mut control, engine) = Engine::prepare(
        config,
        topology,
        resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 4,
            media_slots: 4,
            export_slots: 1,
        },
        vec![Binding {
            target,
            playback: node(0),
            capture: node(1),
            channels: 2,
            midi: false,
            gain: 1.,
            smoothing: Smoothing::None,
            midi_events: 1,
            midi_bytes: 4,
            packet_bytes: 4,
        }],
        vec![],
        vec![(node(2), CheckedDsp(dsp))],
        graph,
    )
    .unwrap();
    for (at, transition) in [
        (
            1,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
    ] {
        control
            .submit(
                Lane::Normal,
                Frame(at),
                Operation::Transition {
                    target,
                    timing: Timing::At,
                    transition,
                },
            )
            .unwrap_or_else(|e| panic!("submit: {:?}", e.error));
    }
    (
        client,
        Fixture {
            control,
            engine,
            render,
        },
    )
}

impl Fixture {
    fn step(&mut self, frames: usize) -> ([f32; 8], [f32; 8]) {
        let frame = self.engine.next_frame();
        let input: [f32; 8] = std::array::from_fn(|i| (frame.0 + i as u64 + 1) as f32);
        let input_right = input.map(|x| x * 10.);
        let mut left = [0.; 8];
        let mut right = [0.; 8];
        heap::assert_no_heap(|| {
            self.render.begin_callback(frame, frames)?;
            let processing = self.engine.process(
                AudioBlock {
                    frame,
                    frames,
                    inputs: &[&input[..frames], &input_right[..frames]],
                    outputs: &mut [&mut left[..frames], &mut right[..frames]],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            );
            let completion = self.render.finish_callback(processing);
            processing.and(completion)
        })
        .unwrap();
        (left, right)
    }

    fn cleanup(self) {
        drop(self.control);
        let mut stopped = self.engine.into_stopped();
        stopped.cleanup().unwrap();
        for _ in 0..512 {
            stopped.resources().begin_callback();
            stopped.resources().reclaim();
        }
        let usage = stopped.resource_usage();
        assert_eq!(usage.free_pages, 128);
        assert_eq!(usage.free_versions, 16);
        assert_eq!(usage.free_snapshots, 16);
        assert_eq!(stopped.media_usage().references, 0);
    }
}

fn expected(frame: usize, changed: bool) -> (f32, f32) {
    let live = frame as f32 + 1.;
    let recorded = [2., 3., 4.];
    let playback = if frame >= 4 {
        recorded[(frame - 4) % 3]
    } else {
        0.
    };
    if changed && frame >= 5 {
        let mix = live * 2. + playback * 0.5;
        (mix * 10., mix)
    } else {
        let mix = live + playback;
        (mix, mix * 10.)
    }
}

#[shoop_test]
fn interior_typed_activation_and_stalled_return_preserve_loop_intervals_and_wraps() {
    for parts in [
        vec![8],
        vec![1; 8],
        vec![3, 2, 3],
        vec![5, 3],
        vec![0, 3, 0, 2, 3],
    ] {
        let (mut client, mut fixture) = setup();
        let ticket = client.select(Frame(5), preset(), 1).unwrap();
        let mut absolute = 0;
        let mut completion_start = Frame(0);
        for &frames in &parts {
            if absolute <= 5 && 5 < absolute + frames {
                completion_start = Frame(absolute as u64);
            }
            let (left, right) = fixture.step(frames);
            for i in 0..frames {
                assert_eq!((left[i], right[i]), expected(absolute + i, true));
            }
            absolute += frames;
        }
        assert_eq!(absolute, 8);
        assert_eq!(
            client.select(Frame(16), preset(), 2).unwrap_err().error,
            Error::Full
        );
        let (left, right) = fixture.step(8);
        assert_eq!(left, [195., 220., 230., 255., 280., 290., 315., 340.]);
        assert_eq!(right, left.map(|x| x * 0.1));
        let completion = client.try_completion().unwrap();
        assert_eq!(completion.sequence, ticket.sequence());
        assert_eq!(completion.frame, completion_start);
        assert_eq!(
            completion.request.reply,
            Some(Reply::Applied {
                frame: Frame(5),
                previous_revision: 1,
                revision: 2,
                preset: preset()
            })
        );
        assert!(client.try_completion().is_none());
        assert_eq!(
            fixture.render.cycles(),
            parts.iter().filter(|&&n| n > 0).count() as u64 + 1
        );
        fixture.cleanup();
    }
}

#[shoop_test]
fn recovery_cancels_delivered_future_activation_with_normal_credit_full() {
    let (mut client, mut fixture) = setup();
    let normal = client.select(Frame(100), preset(), 1).unwrap();
    fixture.step(8);
    assert!(client.try_completion().is_none());
    normal.cancel();
    assert_eq!(
        client.select(Frame(16), preset(), 1).unwrap_err().error,
        Error::Full
    );
    let recovery = client.cancel_pending(normal.sequence()).unwrap();
    let (left, right) = fixture.step(8);
    assert_eq!(left, [12., 14., 13., 15., 17., 16., 18., 20.]);
    assert_eq!(right, left.map(|x| x * 10.));
    let cancel = client.try_completion().unwrap();
    assert_eq!(cancel.sequence, recovery.sequence());
    assert_eq!(
        cancel.request.reply,
        Some(Reply::CancelRegistered {
            target: normal.sequence()
        })
    );
    let original = client.try_completion().unwrap();
    assert_eq!(original.sequence, normal.sequence());
    assert_eq!(original.request.reply, Some(Reply::Cancelled));
    assert!(client.try_completion().is_none());
    fixture.cleanup();
}

#[shoop_test]
fn cancellation_before_dequeue_has_a_typed_terminal_without_applying() {
    let (mut client, mut fixture) = setup();
    let ticket = client.select(Frame(5), preset(), 1).unwrap();
    ticket.cancel();
    let (left, right) = fixture.step(8);
    assert_eq!(left, [1., 2., 3., 4., 7., 9., 11., 10.]);
    assert_eq!(right, left.map(|x| x * 10.));
    let completion = client.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.request.reply, Some(Reply::Cancelled));
    fixture.cleanup();
}

#[shoop_test]
fn malformed_stale_and_late_requests_refuse_without_mutating_dsp() {
    for (at, id, revision, error) in [
        (
            Frame(8),
            PresetId {
                slot: 2,
                generation: 1,
            },
            1,
            Error::InvalidId,
        ),
        (
            Frame(8),
            PresetId {
                slot: 1,
                generation: 2,
            },
            1,
            Error::InvalidId,
        ),
        (Frame(8), preset(), 2, Error::InvalidState),
        (Frame(7), preset(), 1, Error::InvalidEvent),
    ] {
        let (mut client, mut fixture) = setup();
        fixture.step(8);
        let ticket = client.select(at, id, revision).unwrap();
        let (left, right) = fixture.step(8);
        assert_eq!(left, [12., 14., 13., 15., 17., 16., 18., 20.]);
        assert_eq!(right, left.map(|x| x * 10.));
        let completion = client.try_completion().unwrap();
        assert_eq!(completion.sequence, ticket.sequence());
        assert_eq!(completion.frame, Frame(8));
        assert_eq!(completion.request.reply, Some(Reply::Refused(error)));
        fixture.cleanup();
    }
}

#[shoop_test]
fn disconnected_component_retains_undeliverable_owners_until_stopped_teardown() {
    let (mut client, mut fixture) = setup();
    let ticket = client.select(Frame(100), preset(), 1).unwrap();
    drop(ticket);
    drop(client);
    let (left, right) = fixture.step(8);
    assert_eq!(left, [1., 2., 3., 4., 7., 9., 11., 10.]);
    assert_eq!(right, left.map(|x| x * 10.));
    fixture.step(8);
    fixture.cleanup();
}
