//! Whole-callback measurements of the fixed core with independent sample checks.
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    processing::{self, Block, Processor, Quantum},
    resource::{Description, Format},
    Error, Frame, NodeId, Revision,
};
use shoop_loops::{
    media,
    processing::{Bank, Binding, Command, Node, Reply},
    LoopId,
};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
use std::{io::Write, time::Instant};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

enum Dsp {
    Loop(Node),
    Fx,
}
impl Processor<Bank> for Dsp {
    fn validate_state(&self, id: NodeId, s: &Bank) -> Result<(), Error> {
        match self {
            Self::Loop(p) => p.validate_state(id, s),
            Self::Fx => Ok(()),
        }
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(p) => p.ports(),
            Self::Fx => PortCounts {
                audio_inputs: 4,
                audio_outputs: 2,
                ..Default::default()
            },
        }
    }
    fn process(&mut self, b: &mut Block<'_>, s: &mut Bank) -> Result<(), Error> {
        match self {
            Self::Loop(p) => p.process(b, s),
            Self::Fx => {
                for ch in 0..2 {
                    for f in 0..b.tick.quantum.frames() {
                        let v = (b.audio.input(ch).unwrap()[f] + b.audio.input(ch + 2).unwrap()[f])
                            * 0.5;
                        b.audio.output(ch).unwrap()[f] = v;
                    }
                }
                Ok(())
            }
        }
    }
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let q: usize = args.get(1).map_or(32, |s| s.parse().unwrap());
    let loops: usize = args.get(2).map_or(16, |s| s.parse().unwrap());
    let scenario = args.get(3).map_or("steady", String::as_str);
    let iterations: usize = args.get(4).map_or(8192, |s| s.parse().unwrap());
    assert!(matches!(loops, 1 | 4 | 16 | 64));
    assert!(iterations > 0);
    assert!(matches!(scenario, "steady" | "two-seeks" | "two-bursts"));
    assert!(scenario == "steady" || q <= 32);
    let c = processing::Config {
        quantum: Quantum::new(q).unwrap(),
        max_host_frames: 128,
        capacities: rt_runtime::Config {
            max_nodes: loops * 3,
            max_ports: loops * 14 + 6,
            max_edges: loops * 8,
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 0,
            midi_outputs: 0,
            resource_slots: (loops * 8 + 8).max(16),
            resource_pages: 32768,
            page_bytes: 256,
            command_slots: 256,
            budgets: rt_runtime::Budgets {
                commands: 256,
                events: 256,
                publications: 256,
                ..Default::default()
            },
            ..Default::default()
        },
    }
    .prepare()
    .unwrap();
    let mut resources = Bank::resource_store(c, loops).unwrap();
    let mut imports = Vec::new();
    let mut imported = Vec::new();
    let mut bindings = Vec::new();
    let mut graph = GraphDescription {
        nodes: vec![],
        connections: vec![],
    };
    for slot in 0..loops as u32 {
        let bytes: Vec<_> = (0..1024)
            .flat_map(|f| {
                let v = ((f % 257) as f32 - 128.) / 128.;
                [v, v * 10.].into_iter().flat_map(f32::to_le_bytes)
            })
            .collect();
        let resource = resources.create().unwrap();
        imported.push(resource);
        let upload = resources
            .begin_upload(
                resource,
                Revision(0),
                Description {
                    format: Format::Audio { channels: 2 },
                    frames: 1024,
                    bytes: bytes.len(),
                },
            )
            .unwrap();
        for (i, part) in bytes.chunks(256).enumerate() {
            resources.begin_callback();
            resources.write_upload(upload, i * 256, part).unwrap();
        }
        resources.finish_upload(upload).unwrap();
        resources.commit(upload).unwrap();
        let snapshot = resources.snapshot(resource, Frame(0)).unwrap();
        let index = resources.read_index(snapshot.id).unwrap();
        imports.push(media::Input {
            description: media::Description {
                owner: id(slot),
                revision: Revision(0),
                frames: 1024,
                channels: 2,
                midi: false,
            },
            audio: Some(media::Source { snapshot, index }),
            midi: None,
        });
        let p = node(slot * 3);
        let capture = node(slot * 3 + 1);
        let fx = node(slot * 3 + 2);
        bindings.push(Binding {
            target: id(slot),
            playback: p,
            capture,
            channels: 2,
            midi: false,
            gain: 1.,
            smoothing: shoop_loops::audio::Smoothing::None,
        });
        graph.nodes.extend([p, capture, fx]);
        for ch in 0..2 {
            for (source, destination) in [
                (
                    Endpoint::host(PortType::Audio, ch),
                    Endpoint::node(fx, PortType::Audio, ch),
                ),
                (
                    Endpoint::node(p, PortType::Audio, ch),
                    Endpoint::node(fx, PortType::Audio, ch + 2),
                ),
                (
                    Endpoint::node(fx, PortType::Audio, ch),
                    Endpoint::node(capture, PortType::Audio, ch),
                ),
                (
                    Endpoint::node(fx, PortType::Audio, ch),
                    Endpoint::host(PortType::Audio, ch),
                ),
            ] {
                graph.connections.push(Connection {
                    source,
                    destination,
                });
            }
        }
    }
    let prepared = Bank::prepare(c, resources, bindings, imports).unwrap();
    let mut nodes: Vec<_> = prepared
        .nodes
        .into_iter()
        .map(|(id, p)| (id, Dsp::Loop(p)))
        .collect();
    for slot in 0..loops as u32 {
        nodes.push((node(slot * 3 + 2), Dsp::Fx));
    }
    let mut runtime =
        processing::Runtime::prepare(c, nodes, prepared.bank, prepared.resources, &graph).unwrap();
    let mut control = prepared.control;
    for slot in 0..loops as u32 {
        control
            .submit(
                Frame(0),
                Command::Play {
                    target: id(slot),
                    position: 0,
                },
            )
            .unwrap();
    }
    let input = [[0f32; 128]; 2];
    let mut output = [[0f32; 128]; 2];
    let mut positions = vec![0u64; loops];
    let mut times = vec![0u64; iterations];
    for t in &mut times {
        *std::hint::black_box(t) = 0;
    }
    for iteration in 0..1024 + iterations {
        let start_frame = runtime.next_frame();
        if scenario != "steady" {
            for (offset, position) in [(32, 0), (96, 512)] {
                for slot in 0..loops {
                    if scenario == "two-seeks" && slot != if offset == 32 { 0 } else { loops - 1 } {
                        continue;
                    }
                    control
                        .submit(
                            Frame(start_frame.0 + offset),
                            Command::Seek {
                                target: id(slot as u32),
                                position,
                            },
                        )
                        .unwrap();
                }
            }
        }
        let [left, right] = &mut output;
        let start = Instant::now();
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: start_frame,
                    frames: 128,
                    inputs: &[&input[0], &input[1]],
                    outputs: &mut [&mut left[..], &mut right[..]],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        let elapsed = start.elapsed().as_nanos() as u64;
        if iteration >= 1024 {
            times[iteration - 1024] = elapsed;
        }
        for offset in 0..128 {
            if scenario != "steady" && matches!(offset, 32 | 96) {
                let pos = if offset == 32 { 0 } else { 512 };
                if scenario == "two-bursts" {
                    positions.fill(pos)
                } else {
                    positions[if offset == 32 { 0 } else { loops - 1 }] = pos
                }
            }
            let mut expected = [0f32; 2];
            for pos in &mut positions {
                let v = ((*pos % 257) as f32 - 128.) / 128.;
                expected[0] += v * 0.5;
                expected[1] += (v * 10.) * 0.5;
                *pos = (*pos + 1) % 1024;
            }
            assert_eq!(output[0][offset], expected[0]);
            assert_eq!(output[1][offset], expected[1]);
        }
        while let Some(receipt) = control.try_receipt() {
            assert_eq!(receipt.result, Ok(Reply::Done));
            assert_eq!(receipt.actual, receipt.requested);
        }
        assert_eq!(control.outstanding(), 0);
        let d = runtime.diagnostics();
        assert_eq!(d.ticks, 128 / q);
        assert_eq!(d.nodes, loops * 3 * 128 / q);
        assert_eq!(d.edges, loops * 8 * 128 / q);
        for (slot, pos) in positions.iter().enumerate() {
            assert_eq!(
                runtime.state().status(id(slot as u32)).unwrap().position,
                *pos
            );
        }
    }
    if let Some(path) = args.get(5) {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        writeln!(f, "callback,nanoseconds").unwrap();
        for (i, t) in times.iter().enumerate() {
            writeln!(f, "{i},{t}").unwrap();
        }
    }
    times.sort_unstable();
    let mean = times.iter().sum::<u64>() as f64 / iterations as f64;
    println!("q={q} loops={loops} scenario={scenario} callbacks={iterations} mean_us={} budget_percent={} p999_us={} max_us={} misses={}",mean/1000.,mean*48000./128./1e7,times[(iterations-1)*999/1000]as f64/1000.,times[iterations-1]as f64/1000.,times.iter().filter(|&&t|t*48000>128_000_000_000).count());
    let at = runtime.next_frame();
    let (mut bank, mut resources) = runtime.into_parts();
    bank.cleanup(&mut resources, at).unwrap();
    for r in imported {
        resources.remove(r).unwrap();
    }
    for _ in 0..32768 {
        resources.begin_callback();
        resources.reclaim();
    }
    assert_eq!(resources.usage().free_pages, 32768);
}
