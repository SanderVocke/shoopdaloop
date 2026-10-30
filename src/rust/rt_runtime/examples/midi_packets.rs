//! Complete packets, callback partitioning, stable ties and backpressured recovery.

use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::NoPlanner;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Budgets, Config, Error, Frame, NodeId};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct Pass {
    received_resets: Arc<AtomicUsize>,
}

impl Processor for Pass {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }

    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        output.copy_from_slice(input);
        let input = &block.midi_inputs[0];
        if input.is_recovery() {
            assert_eq!(input.len(), 32);
            for index in 0..32 {
                let expected = [
                    0xb0 | (index / 2) as u8,
                    if index % 2 == 0 { 120 } else { 123 },
                    0,
                ];
                assert_eq!(input.get(index), Some((0, &expected[..])));
            }
            self.received_resets.fetch_add(32, Ordering::Relaxed);
        } else {
            for index in 0..input.len() {
                let (offset, packet) = input.get(index).ok_or(Error::InvalidEvent)?;
                block.midi_outputs[0].push(offset, block.frames, packet)?;
            }
        }
        Ok(())
    }
}

fn prepare(
    events: usize,
    bytes: usize,
    recovery_work: usize,
) -> Result<(Control<Pass>, GraphRuntime<Pass>, Arc<AtomicUsize>), Error> {
    let config = Config {
        max_frames: 4,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 2,
        midi_outputs: 1,
        max_nodes: 1,
        max_ports: 12,
        max_edges: 5,
        midi_events_per_port: events,
        midi_bytes_per_port: bytes,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        budgets: Budgets {
            events: recovery_work,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let node = NodeId::from_parts(0, 1)?;
    let edge = |kind, source, destination| Connection {
        source: match source {
            Some(node) => Endpoint::node(node, kind, 0),
            None => Endpoint::host(kind, 0),
        },
        destination: match destination {
            Some(node) => Endpoint::node(node, kind, 0),
            None => Endpoint::host(kind, 0),
        },
    };
    let graph = GraphDescription {
        nodes: vec![node],
        connections: vec![
            edge(PortType::Audio, None, Some(node)),
            edge(PortType::Audio, Some(node), None),
            edge(PortType::Midi, None, Some(node)),
            Connection {
                source: Endpoint::host(PortType::Midi, 1),
                destination: Endpoint::node(node, PortType::Midi, 0),
            },
            edge(PortType::Midi, Some(node), None),
        ],
    };
    let received_resets = Arc::new(AtomicUsize::new(0));
    let (control, runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(
            node,
            Pass {
                received_resets: Arc::clone(&received_resets),
            },
        )],
        NoPlanner,
        ResourceStore::new(config)?,
        &graph,
        &[],
    )?;
    Ok((control, runtime, received_resets))
}

fn render(
    runtime: &mut GraphRuntime<Pass>,
    frames: usize,
    inputs: &[MidiBuffer; 2],
    outputs: &mut [MidiBuffer; 1],
    audio: &mut [f32; 4],
) -> Result<(), Error> {
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames,
                inputs: &[&[1.0; 4]],
                outputs: &mut [audio],
            },
            MidiBlock { inputs, outputs },
        )
    })
}

fn exact_packets(partition: &[usize]) -> Result<(), Error> {
    let (mut control, mut runtime, _) = prepare(8, 32, 8)?;
    let ticket = control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(2),
                action: Action::Midi {
                    node: NodeId::from_parts(0, 1)?,
                    port: 0,
                },
                payload: Box::new([0x90, 72, 100]),
            },
        )
        .map_err(|refusal| refusal.error)?;
    let mut inputs = [MidiBuffer::new(8, 32)?, MidiBuffer::new(8, 32)?];
    let mut outputs = [MidiBuffer::new(8, 32)?];
    let mut audio = [0.0; 4];
    let host_packets = [
        (0, 0, [0x90, 60, 100]),
        (2, 0, [0x90, 70, 100]),
        (2, 1, [0x90, 71, 100]),
        (3, 0, [0x80, 60, 0]),
    ];
    let mut observed = Vec::with_capacity(5);
    for &frames in partition {
        let start = runtime.next_frame().0;
        for input in &mut inputs {
            input.clear();
        }
        for &(at, port, packet) in &host_packets {
            if at >= start && at < start + frames as u64 {
                inputs[port].push((at - start) as u32, frames, &packet)?;
            }
        }
        if start == 2 {
            render(&mut runtime, 0, &inputs, &mut outputs, &mut audio)?;
            assert_eq!(runtime.next_frame(), Frame(2));
            assert!(control.try_execution().is_none());
        }
        render(&mut runtime, frames, &inputs, &mut outputs, &mut audio)?;
        assert_eq!(audio[..frames], [1.0; 4][..frames]);
        for index in 0..outputs[0].len() {
            let (offset, packet) = outputs[0].get(index).unwrap();
            observed.push((start + offset as u64, <[u8; 3]>::try_from(packet).unwrap()));
        }
    }
    assert_eq!(runtime.next_frame(), Frame(4));
    assert_eq!(
        observed,
        [
            (0, [0x90, 60, 100]),
            (2, [0x90, 70, 100]),
            (2, [0x90, 71, 100]),
            (2, [0x90, 72, 100]),
            (3, [0x80, 60, 0]),
        ]
    );
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(
        completion.outcome,
        Outcome::Scheduled {
            effective_frame: Frame(2),
            late_by: 0
        }
    );
    drop(completion);
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.actual_frame, Frame(2));
    assert_eq!(execution.result, Ok(()));
    assert_eq!(control.outstanding(), 0);
    drop(runtime);
    drop(control);
    Ok(())
}

fn overflow_recovery() -> Result<(), Error> {
    let (control, mut runtime, received_resets) = prepare(1, 3, 2)?;
    let mut inputs = [MidiBuffer::new(1, 3)?, MidiBuffer::new(1, 3)?];
    inputs[0].push(0, 4, &[0x90, 60, 100])?;
    inputs[1].push(0, 4, &[0x90, 61, 100])?;
    let mut outputs = [MidiBuffer::new(1, 3)?];
    let mut sink = MidiBuffer::new(1, 3)?;
    let mut audio = [0.0; 4];
    let mut delivered = [[0; 3]; 32];
    for index in 0..32 {
        if index == 2 {
            for input in &mut inputs {
                input.clear();
            }
        }
        render(&mut runtime, 4, &inputs, &mut outputs, &mut audio)?;
        assert_eq!(audio, [1.0; 4]);
        assert!(outputs[0].is_empty());
        assert!(runtime.midi_recovery_pending(0)?);
        sink.clear();
        assert_no_heap(|| {
            assert_eq!(runtime.drain_midi_recovery(0, &mut sink, 4)?, 1);
            delivered[index].copy_from_slice(sink.get(0).unwrap().1);
            assert_eq!(runtime.drain_midi_recovery(0, &mut sink, 4)?, 0);
            Ok::<_, Error>(())
        })?;
        if index == 0 {
            sink.clear();
            render(&mut runtime, 0, &inputs, &mut outputs, &mut audio)?;
            assert_eq!(
                assert_no_heap(|| runtime.drain_midi_recovery(0, &mut sink, 4))?,
                0
            );
        }
    }
    assert_eq!(received_resets.load(Ordering::Relaxed), 64);
    for (index, packet) in delivered.iter().enumerate() {
        assert_eq!(
            *packet,
            [
                0xb0 | (index / 2) as u8,
                if index % 2 == 0 { 120 } else { 123 },
                0
            ]
        );
    }
    assert!(!runtime.midi_recovery_pending(0)?);
    inputs[0].push(0, 4, &[0x90, 62, 100])?;
    render(&mut runtime, 4, &inputs, &mut outputs, &mut audio)?;
    assert_eq!(outputs[0].get(0), Some((0, &[0x90, 62, 100][..])));
    assert_eq!(control.outstanding(), 0);
    drop(runtime);
    drop(control);
    Ok(())
}

fn main() -> Result<(), Error> {
    exact_packets(&[4])?;
    exact_packets(&[1, 3])?;
    exact_packets(&[1, 1, 1, 1])?;
    overflow_recovery()?;
    let mut packets = MidiBuffer::new(2, 8)?;
    assert_no_heap(|| {
// Packets are complete and carry an offset within the current interval.
        packets.push(0, 4, &[0x90, 60, 100])?;
        assert_eq!(packets.push(1, 4, &[0x80, 60]), Err(Error::InvalidEvent));
        assert_eq!(packets.len(), 1);
        packets.push(2, 4, &[0xf0, 0x7d, 1, 0xf7])?;
        assert_eq!(packets.byte_len(), 7);
        assert_eq!(packets.push(3, 4, &[0xf8]), Err(Error::Full));
        Ok::<_, Error>(())
    })?;
    println!("MIDI packets: exact frames/ties, complete SysEx, continuing audio and all 32 recovery packets verified");
    Ok(())
}
