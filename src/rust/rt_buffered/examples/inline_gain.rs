use rt_buffered::{BufferedProcessor, MidiLimits, Timing};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    fixed::{Config, Quantum, Runtime},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::ResourceStore,
    Frame, NodeId,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .map(|s| s.parse())
        .collect::<Result<_, _>>()?;
    let quantum = Quantum::new(args.first().copied().unwrap_or(16))?;
    let timing = Timing {
        batch_quanta: args.get(1).copied().unwrap_or(2),
        slack_quanta: args.get(2).copied().unwrap_or(1),
    };
    let node = BufferedProcessor::prepare(quantum, timing, MidiLimits::default())?;
    let latency = node.latency_frames();
    let block = node.timing().batch_frames();
    let q = quantum.frames();
    let config = Config {
        quantum,
        max_host_frames: q,
        capacities: rt_runtime::Config {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
            ..Default::default()
        },
    }
    .prepare()?;
    let id = NodeId::from_parts(0, 1)?;
    let mut graph = GraphDescription {
        nodes: vec![id],
        connections: Vec::new(),
    };
    for (kind, count) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
        for c in 0..count {
            for (source, destination) in [
                (Endpoint::host(kind, c), Endpoint::node(id, kind, c)),
                (Endpoint::node(id, kind, c), Endpoint::host(kind, c)),
            ] {
                graph.connections.push(Connection {
                    source,
                    destination,
                });
            }
        }
    }
    let mut runtime = Runtime::<BufferedProcessor, ()>::prepare(
        config,
        vec![(id, node)],
        (),
        ResourceStore::new(config.resources())?,
        &graph,
    )?;
    let source: &[(usize, &[u8], &[u8])] = &[
        (q - 1, &[0x9f, 60, 100], &[0x90, 60, 100]),
        (block + q - 1, &[0x8f, 60, 0], &[0x80, 60, 0]),
        (block + q - 1, &[0xf0, 1, 2, 0xf7], &[0xf0, 1, 2, 0xf7]),
    ];
    let mut input = MidiBuffer::new(8, 128)?;
    let mut output = MidiBuffer::new(8, 128)?;
    let mut received = Vec::new();
    for frame in (0..2 * block + latency + q).step_by(q) {
        let left: Vec<_> = (frame..frame + q)
            .map(|n| if n < 2 * block { n as f32 * 0.25 } else { 0. })
            .collect();
        let right: Vec<_> = (frame..frame + q)
            .map(|n| {
                if n < 2 * block {
                    -(n as f32) * 0.25 - 1.
                } else {
                    0.
                }
            })
            .collect();
        let mut out_left = vec![0.; q];
        let mut out_right = vec![0.; q];
        input.clear();
        output.clear();
        for &(at, bytes, _) in source {
            if at >= frame && at < frame + q {
                input.push((at - frame) as u32, q, bytes)?;
            }
        }
        runtime.process(
            AudioBlock {
                frame: Frame(frame as u64),
                frames: q,
                inputs: &[&left, &right],
                outputs: &mut [&mut out_left, &mut out_right],
            },
            MidiBlock {
                inputs: core::slice::from_ref(&input),
                outputs: core::slice::from_mut(&mut output),
            },
        )?;
        for n in 0..q {
            let at = frame + n;
            let active = at >= latency && at < latency + 2 * block;
            let sample = if active {
                (at - latency) as f32 * 0.25
            } else {
                0.
            };
            assert_eq!(out_left[n], 2. * sample);
            assert_eq!(out_right[n], if active { 2. * (-sample - 1.) } else { 0. });
        }
        for i in 0..output.len() {
            let (at, bytes) = output.get(i).unwrap();
            received.push((frame + at as usize, bytes.to_vec()));
        }
    }
    assert_eq!(
        received,
        source
            .iter()
            .map(|&(at, _, bytes)| (at + latency, bytes.to_vec()))
            .collect::<Vec<_>>()
    );
    println!("Verified stereo 2x gain and MIDI channel rotation: Q={q}, block={block}, slack={}, node latency={latency} frames, {} MIDI messages.", timing.slack_quanta, received.len());
    Ok(())
}
