//! A pending program/route pair, with scalar FX output independent of loop media.
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Command, Retired},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    processor::{ProcessBlock, Processor},
    resource::ResourceStore,
    transport::{Lane, Outcome as Admission},
    Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId, Sequence,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{
        Binding, Controller, Engine, LoopPlanner, Operation, Reply, Settings, TopologySettings,
    },
    topology::staging::StageId,
    Entry, Limits, LoopId, LoopKind, LoopSpec, Outcome, PreparedTopology, ProgramDescription,
    Section, Timing, TopologyDescription, Transition,
};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
struct Half;
impl Processor<LoopPlanner> for Half {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = *input * 0.5;
        }
        Ok(())
    }
}
fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn limits() -> Limits {
    Limits {
        loop_slots: 3,
        relations: 4,
        program_entries: 4,
        program_segments: 8,
        pending_requests: 8,
        journal_events: 0,
    }
}
fn topology(duration: u64) -> PreparedTopology {
    compile(
        limits(),
        &TopologyDescription {
            loops: (0..3)
                .map(|slot| LoopSpec {
                    id: id(slot),
                    kind: if slot < 2 {
                        LoopKind::Primitive
                    } else {
                        LoopKind::Composite
                    },
                    length: u64::from(slot < 2),
                    sync_source: (slot == 2).then_some(id(0)),
                })
                .collect(),
            programs: vec![ProgramDescription {
                source: id(2),
                nominal_unit_frames: 1,
                sections: vec![Section {
                    entries: vec![Entry {
                        target: id(1),
                        delay: 0,
                        duration: Some(duration),
                        repetitions: 1,
                        mode: None,
                    }],
                }],
            }],
            ..TopologyDescription::default()
        },
    )
    .unwrap()
}
fn graph(bypass: bool) -> GraphDescription {
    let input = Endpoint::host(PortType::Audio, 0);
    let output = Endpoint::host(PortType::Audio, 0);
    GraphDescription {
        nodes: (0..5).map(node).collect(),
        connections: if bypass {
            vec![Connection {
                source: input,
                destination: output,
            }]
        } else {
            vec![
                Connection {
                    source: input,
                    destination: Endpoint::node(node(4), PortType::Audio, 0),
                },
                Connection {
                    source: Endpoint::node(node(4), PortType::Audio, 0),
                    destination: output,
                },
            ]
        },
    }
}
fn step(engine: &mut Engine<Half>, frames: usize) -> [f32; 8] {
    let input = [2.0; 8];
    let mut output = [-99.0; 8];
    assert_no_heap(|| {
        engine.process(
            AudioBlock {
                frame: engine.next_frame(),
                frames,
                inputs: &[&input],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    assert_eq!(&output[frames..], &[-99.0; 8][frames..]);
    output
}
fn collect(
    control: &mut Controller<Half>,
    sequences: &[Sequence],
) -> Vec<(Reply, Option<Frame>, Frame)> {
    for _ in sequences {
        let completion = control.try_completion().unwrap();
        assert!(sequences.contains(&completion.sequence));
        assert!(matches!(completion.outcome, Admission::Scheduled { .. }));
    }
    let mut results = vec![None; sequences.len()];
    for _ in sequences {
        let execution = control.try_execution().unwrap();
        assert_eq!(execution.result, Ok(()));
        let index = sequences
            .iter()
            .position(|seq| *seq == execution.sequence)
            .unwrap();
        assert!(results[index].is_none());
        results[index] = Some((
            execution.operation_result.unwrap(),
            execution.delivered_frame,
            execution.actual_frame,
        ));
    }
    assert!(control.try_completion().is_none());
    assert!(control.try_execution().is_none());
    results.into_iter().map(Option::unwrap).collect()
}
fn operation(
    control: &mut Controller<Half>,
    engine: &mut Engine<Half>,
    operation: Operation,
    bytes: Box<[u8]>,
) -> Reply {
    let sequence = control
        .submit_payload(Lane::Normal, engine.next_frame(), operation, bytes)
        .unwrap_or_else(|e| panic!("prepare operation: {}", e.error))
        .sequence();
    assert_eq!(step(engine, 1)[0], 1.0);
    collect(control, &[sequence])[0].0
}
fn ready(control: &mut Controller<Half>, engine: &mut Engine<Half>) -> StageId {
    let candidate = topology(2);
    let mut header = [0; 64];
    candidate.encode_header(&mut header);
    let Reply::TopologyStatus { id, .. } = operation(
        control,
        engine,
        Operation::TopologyBegin { slot: 0 },
        Box::new(header),
    ) else {
        panic!("begin")
    };
    for first_record in 0..candidate.encoded_records() {
        let mut record = [0; 64];
        candidate.encode_record(first_record, &mut record).unwrap();
        operation(
            control,
            engine,
            Operation::TopologyAppend { id, first_record },
            Box::new(record),
        );
    }
    for _ in 0..512 {
        let Reply::TopologyProgress { progress, .. } = operation(
            control,
            engine,
            Operation::TopologyValidate { id },
            Box::new([]),
        ) else {
            panic!("validate")
        };
        if progress.ready {
            return id;
        }
    }
    panic!("finite preparation bound")
}
fn send(control: &mut Controller<Half>, at: Frame, operation: Operation) -> Sequence {
    control
        .submit(Lane::Normal, at, operation)
        .unwrap_or_else(|e| panic!("submit: {}", e.error))
        .sequence()
}
fn musical(target: u32, transition: Transition) -> Operation {
    Operation::Transition {
        target: id(target),
        timing: Timing::At,
        transition,
    }
}
fn run(parts: &[usize]) {
    let config = Config {
        max_frames: 8,
        max_nodes: 6,
        max_ports: 4,
        max_edges: 4,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        command_slots: 8,
        scheduled_events: 8,
        resource_slots: 8,
        resource_pages: 8,
        page_bytes: 16,
        budgets: Budgets {
            commands: 8,
            events: 8,
            transfer_bytes: 64,
            validation_steps: 128,
            publications: 8,
            subranges: 0,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let bindings = (0..2)
        .map(|slot| Binding {
            target: id(slot),
            playback: node(slot * 2),
            capture: node(slot * 2 + 1),
            channels: 0,
            midi: false,
            gain: 1.0,
            smoothing: Smoothing::None,
            midi_events: 0,
            midi_bytes: 0,
            packet_bytes: 0,
        })
        .collect();
    let (mut control, mut engine) = Engine::prepare_with_staging(
        config,
        topology(4),
        ResourceStore::new(config).unwrap(),
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 4,
            media_slots: 8,
            export_slots: 0,
        },
        bindings,
        vec![],
        vec![(node(4), Half)],
        graph(false),
        TopologySettings {
            candidates: 1,
            records_per_callback: 1,
            validation_steps_per_callback: 64,
        },
        None,
    )
    .unwrap();
    engine.prepare_graph_staging(1).unwrap();
    let topology = ready(&mut control, &mut engine);
    let candidate = control.compile_graph(&graph(true)).unwrap();
    let ticket = control
        .stage_graph(0, Epoch(1), candidate)
        .unwrap_or_else(|e| panic!("stage: {}", e.error));
    assert_eq!(step(&mut engine, 1)[0], 1.0);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.outcome, Admission::Applied);
    let Command::StageGraph {
        plan: None,
        id: Some(graph),
        ..
    } = completion.request
    else {
        panic!("Ready graph")
    };
    drop(ticket);
    let start = engine.next_frame();
    let delivered = start.checked_add(1).unwrap();
    let cut = start.checked_add(4).unwrap();
    let stop = start.checked_add(6).unwrap();
    let sequences = [
        send(
            &mut control,
            start,
            musical(
                0,
                Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            ),
        ),
        send(
            &mut control,
            start,
            musical(
                2,
                Transition::PlayIteration {
                    iteration: 0,
                    retrigger: true,
                },
            ),
        ),
        send(
            &mut control,
            delivered,
            Operation::ProgramGraphActivate {
                id: topology,
                target: id(2),
                graph,
                graph_epoch: Epoch(1),
                topology_epoch: Epoch(0),
            },
        ),
        send(&mut control, cut, Operation::State { target: id(2) }),
        send(&mut control, cut, Operation::State { target: id(1) }),
        send(&mut control, stop, musical(0, Transition::Stop)),
        send(&mut control, stop, musical(2, Transition::Stop)),
    ];
    let mut samples = [0.0; 8];
    let mut offset = 0;
    for &frames in parts {
        if engine.next_frame() == cut {
            let before = engine.diagnostics();
            step(&mut engine, 0);
            assert_eq!(engine.next_frame(), cut);
            assert_eq!(engine.diagnostics(), before);
            assert_eq!(before.epoch, Epoch(1));
        }
        samples[offset..offset + frames].copy_from_slice(&step(&mut engine, frames)[..frames]);
        offset += frames;
    }
    assert_eq!(offset, 8);
    assert_eq!(samples, [1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0]);
    let results = collect(&mut control, &sequences);
    let (
        Reply::ProgramGraphChanged {
            id: changed,
            graph: changed_graph,
            graph_epoch,
            topology_epoch,
            completion,
        },
        actual_delivery,
        terminal,
    ) = results[2]
    else {
        panic!("paired result")
    };
    assert_eq!(
        (changed, changed_graph, graph_epoch, topology_epoch),
        (topology, graph, Epoch(2), Epoch(1))
    );
    assert_eq!(
        (
            completion.requested,
            completion.effective,
            actual_delivery,
            terminal,
            completion.outcome
        ),
        (
            delivered,
            delivered,
            Some(delivered),
            cut,
            Outcome::Applied { frame: cut }
        )
    );
    let Reply::State(program) = results[3].0 else {
        panic!("program cut")
    };
    assert_eq!(
        (
            program.frame,
            program.graph_epoch,
            program.topology_epoch,
            program.state.content_length
        ),
        (cut, Epoch(2), Epoch(1), 2)
    );
    assert_eq!(
        (
            program.state.composite.unwrap().iteration,
            program.state.composite.unwrap().passes
        ),
        (0, 1)
    );
    let Reply::State(child) = results[4].0 else {
        panic!("continuing child")
    };
    assert_eq!(child.state.cycles, 4);
    let recycle = send(
        &mut control,
        engine.next_frame(),
        Operation::TopologyRecycle { id: topology },
    );
    step(&mut engine, 1);
    assert!(matches!(
        collect(&mut control, &[recycle])[0].0,
        Reply::TopologyStatus {
            status: shoop_loops::topology::staging::Status::Idle,
            ..
        }
    ));
    let ticket = control
        .release_graph(graph)
        .unwrap_or_else(|e| panic!("release: {}", e.error));
    assert_eq!(step(&mut engine, 1)[0], 2.0);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.outcome, Admission::Applied);
    assert!(matches!(completion.retired, Some(Retired::Plan(_))));
    drop((completion, ticket, control));
    let mut stopped = engine.into_stopped();
    stopped.cleanup().unwrap();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.media_usage().references, 0);
    assert_eq!(stopped.resource_usage().free_pages, 8);
    assert_eq!(stopped.resource_usage().free_versions, 8);
    assert_eq!(stopped.resource_usage().free_snapshots, 8);
}
fn main() {
    run(&[8]);
    run(&[1; 8]);
    run(&[3, 1, 4]);
    println!("program/graph cut: Pending until real zero, paired epochs/samples, continuing child, zero no-op and independent retirement verified");
}
