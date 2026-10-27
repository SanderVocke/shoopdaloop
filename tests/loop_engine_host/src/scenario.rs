use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    processor::{ProcessBlock, Processor},
    resource::{CaptureGroupConfig, MidiIndexConfig, ResourceStore},
    transport::Lane,
    Budgets, Config, Error, Frame, NodeId, PlannerId,
};
use shoop_loop_engine::{
    audio::Smoothing,
    compile,
    runtime::{Binding, Controller, Engine, LoopPlanner, Operation, Reply, Settings},
    Limits, LoopId, LoopKind, LoopSpec, Outcome, Timing, TopologyDescription, Transition,
};

const FRAMES: usize = 13;
const PAGES: usize = 256;

pub(crate) struct Mix;
impl Processor<LoopPlanner> for Mix {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 4,
            audio_outputs: 2,
            midi_inputs: 2,
            midi_outputs: 1,
        }
    }

    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        for channel in 0..2 {
            for frame in 0..block.frames {
                let live = block.audio.input(channel).ok_or(Error::InvalidShape)?[frame];
                let playback = block.audio.input(channel + 2).ok_or(Error::InvalidShape)?[frame];
                block.audio.output(channel).ok_or(Error::InvalidShape)?[frame] =
                    (live + playback) * 0.5;
            }
        }
        let mut cursors = [0; 2];
        while let Some((offset, port)) = (0..2)
            .filter_map(|port| {
                block.midi_inputs[port]
                    .get(cursors[port])
                    .map(|(offset, _)| (offset, port))
            })
            .min()
        {
            let (_, packet) = block.midi_inputs[port]
                .get(cursors[port])
                .ok_or(Error::InvalidState)?;
            cursors[port] += 1;
            block.midi_outputs[0].push(offset, block.frames, packet)?;
        }
        Ok(())
    }
}

pub(crate) fn prepare() -> (Controller<Mix>, Engine<Mix>, LoopId) {
    let config = Config {
        max_frames: 128,
        max_nodes: 4,
        max_ports: 24,
        max_edges: 16,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        midi_events_per_port: 256,
        midi_bytes_per_port: 1024,
        resource_slots: 16,
        resource_pages: PAGES,
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
    .expect("fixture configuration");
    let resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: 4,
            replay_events: 128,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 4096,
        },
    )
    .expect("prepared capture stores");
    let target = LoopId::from_parts(0, 1).unwrap();
    let playback = NodeId::from_parts(0, 1).unwrap();
    let capture = NodeId::from_parts(1, 1).unwrap();
    let effect = NodeId::from_parts(2, 1).unwrap();
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
    .expect("fixture topology");
    let mut graph = GraphDescription {
        nodes: vec![playback, effect, capture],
        connections: Vec::new(),
    };
    for (kind, channels) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
        for port in 0..channels {
            graph.connections.extend([
                Connection {
                    source: Endpoint::host(kind, port),
                    destination: Endpoint::node(effect, kind, port),
                },
                Connection {
                    source: Endpoint::node(playback, kind, port),
                    destination: Endpoint::node(effect, kind, port + channels),
                },
                Connection {
                    source: Endpoint::node(effect, kind, port),
                    destination: Endpoint::node(capture, kind, port),
                },
                Connection {
                    source: Endpoint::node(effect, kind, port),
                    destination: Endpoint::host(kind, port),
                },
            ]);
        }
    }
    let (control, engine) = Engine::prepare(
        config,
        topology,
        resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 4,
            media_slots: 6,
            export_slots: 2,
        },
        vec![Binding {
            target,
            playback,
            capture,
            channels: 2,
            midi: true,
            gain: 1.,
            smoothing: Smoothing::None,
            midi_events: 128,
            midi_bytes: 512,
            packet_bytes: 512,
        }],
        vec![],
        vec![(effect, Mix)],
        graph,
    )
    .expect("prepared facade");
    (control, engine, target)
}

/// Native harness data collected outside guarded callbacks, not a wire format.
#[derive(Debug, PartialEq)]
pub struct Report {
    pub samples: Vec<[f32; 2]>,
    pub midi: Vec<(u64, Vec<u8>)>,
    pub transitions: Vec<(u64, u64, u64)>,
    pub free_pages: usize,
    pub free_versions: usize,
    pub free_snapshots: usize,
    pub media_references: usize,
}

/// Record four stereo/MIDI frames, play twice through FX, then stop at frame 12.
/// Zero calls may be interspersed. All commands are admitted before their deadlines.
pub fn run(partitions: &[usize]) -> Report {
    assert!(partitions.iter().all(|&frames| frames <= 128));
    assert_eq!(partitions.iter().sum::<usize>(), FRAMES);
    let (mut control, mut engine, target) = prepare();
    for (at, transition) in [
        (
            0,
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
        (12, Transition::Stop),
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
            .unwrap_or_else(|error| panic!("submission: {:?}", error.error));
    }
    let mut input = [MidiBuffer::new(2, 6).unwrap()];
    let mut output = [MidiBuffer::new(256, 1024).unwrap()];
    let mut samples = Vec::new();
    let mut midi = Vec::new();
    let mut transitions = Vec::new();
    for &frames in partitions {
        let start = engine.next_frame().0;
        let mut input_left = [0.; 128];
        let mut input_right = [0.; 128];
        let mut left = [0.; 128];
        let mut right = [0.; 128];
        for offset in 0..frames {
            let at = start + offset as u64;
            if at < 4 {
                input_left[offset] = 2. * (at + 1) as f32;
                input_right[offset] = input_left[offset] * 10.;
            }
        }
        input[0].clear();
        output[0].clear();
        for (at, packet) in [(0, [0x90, 60, 100]), (2, [0x80, 60, 45])] {
            if at >= start && at < start + frames as u64 {
                input[0].push((at - start) as u32, frames, &packet).unwrap();
            }
        }
        crate::guarded(|| {
            engine.process(
                AudioBlock {
                    frame: Frame(start),
                    frames,
                    inputs: &[&input_left[..frames], &input_right[..frames]],
                    outputs: &mut [&mut left[..frames], &mut right[..frames]],
                },
                MidiBlock {
                    inputs: &input,
                    outputs: &mut output,
                },
            )
        })
        .expect("guarded callback");
        assert_eq!(engine.next_frame().0, start + frames as u64);
        for offset in 0..frames {
            samples.push([left[offset], right[offset]]);
        }
        for index in 0..output[0].len() {
            let (offset, packet) = output[0].get(index).unwrap();
            midi.push((start + u64::from(offset), packet.to_vec()));
        }
        while let Some(completion) = control.try_completion() {
            assert!(matches!(
                completion.outcome,
                rt_runtime::transport::Outcome::Scheduled { .. }
            ));
        }
        while let Some(execution) = control.try_execution() {
            assert_eq!(execution.result, Ok(()));
            let Some(Reply::Transition(completion)) = execution.operation_result else {
                panic!("transition receipt")
            };
            assert!(matches!(completion.outcome, Outcome::Applied { .. }));
            transitions.push((
                execution.requested_frame.0,
                execution.delivered_frame.unwrap().0,
                execution.actual_frame.0,
            ));
        }
    }
    // Scalar oracle does not consult resolver state, rendered output or captured bytes.
    for (at, &actual) in samples.iter().enumerate() {
        let expected = match at {
            0..=3 => (at + 1) as f32,
            4..=11 => ((at - 4) % 4 + 1) as f32 * 0.5,
            _ => 0.,
        };
        assert_eq!(actual, [expected, expected * 10.], "sample {at}");
    }
    assert_eq!(transitions, [(0, 0, 0), (4, 4, 4), (12, 12, 12)]);
    let mut expected_midi = vec![(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])];
    for at in [4, 8] {
        // The captured initial projection has neutral pitch and three released holds
        // on every channel. Restore that state before the original note-on.
        for channel in 0..16 {
            expected_midi.push((at, vec![0xe0 | channel, 0, 64]));
            for cc in [64, 69, 66] {
                expected_midi.push((at, vec![0xb0 | channel, cc, 0]));
            }
        }
        expected_midi.extend([(at, vec![0x90, 60, 100]), (at + 2, vec![0x80, 60, 45])]);
    }
    assert_eq!(midi, expected_midi);
    drop(control);
    let mut stopped = engine.into_stopped();
    stopped.cleanup().expect("stopped cleanup");
    for _ in 0..512 {
        stopped.resources().begin_callback();
        stopped.resources().reclaim();
    }
    let resources = stopped.resource_usage();
    let media = stopped.media_usage();
    assert_eq!(resources.free_pages, PAGES);
    assert_eq!(resources.free_versions, 16);
    assert_eq!(resources.free_snapshots, 16);
    assert_eq!(media.references, 0);
    Report {
        samples,
        midi,
        transitions,
        free_pages: resources.free_pages,
        free_versions: resources.free_versions,
        free_snapshots: resources.free_snapshots,
        media_references: media.references,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_partitions_match_full_media_and_owning_recovery() {
        let whole = super::run(&[13]);
        assert_eq!(super::run(&[1; 13]), whole);
        assert_eq!(super::run(&[3, 5, 2, 3]), whole);
        assert_eq!(super::run(&[0, 4, 0, 8, 0, 1, 0]), whole);
    }

    #[test]
    fn allocator_instrumentation_detects_all_heap_operations() {
        assert!(crate::allocator_canaries());
    }

    #[test]
    #[should_panic(expected = "heap operation during rendering")]
    fn guard_rejects_allocation() {
        crate::guarded(|| std::hint::black_box(Box::new(7)));
    }

    #[test]
    #[should_panic(expected = "heap operation during rendering")]
    fn guard_rejects_reallocation() {
        let mut bytes = Vec::with_capacity(1);
        bytes.push(0u8);
        crate::guarded(|| bytes.push(std::hint::black_box(1)));
    }

    #[test]
    #[should_panic(expected = "heap operation during rendering")]
    fn guard_rejects_deallocation() {
        let owner = std::hint::black_box(Box::new(7));
        crate::guarded(|| drop(owner));
    }
}
