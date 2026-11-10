//! Inspect the state program and FIFO before running the sample processor.
use rt_fpga_reference::{
    compiler::{Graph, Node, NodeId},
    fixed::Fixed,
    storage::MediaStore,
    Backend,
};

fn main() {
    let mut media = MediaStore::new(32).unwrap();
    let take = media.create_recording();
    let graph = Graph {
        quantum: 32,
        nodes: vec![
            Node::Input { port: 0 },
            Node::Gain {
                input: NodeId(0),
                gain: Fixed::from_f32(0.5),
            },
            Node::Capture {
                input: NodeId(1),
                view: take,
                recording: true,
            },
            Node::Output {
                input: NodeId(1),
                port: 0,
            },
        ],
    };
    let mut backend = Backend::new(&graph, media).unwrap();
    println!("State program:\n{:#?}", backend.program);
    let input: Vec<_> = (0..32)
        .map(|frame| Fixed::from_f32(frame as f32 / 64.))
        .collect();
    let mut output = vec![Fixed::ZERO; 32];
    backend
        .state
        .run(&backend.program, &mut backend.queue)
        .unwrap();
    println!("Jobs:\n{:#?}", backend.queue.iter().collect::<Vec<_>>());
    backend
        .jobs
        .run(
            &mut backend.queue,
            &mut backend.media,
            &[&input],
            &mut [&mut output],
        )
        .unwrap();
    backend.media.seal(take).unwrap();

    let graph = Graph {
        quantum: 32,
        nodes: vec![
            Node::Playback {
                view: take,
                length: 32,
                position: 0,
                playing: true,
            },
            Node::Output {
                input: NodeId(0),
                port: 0,
            },
        ],
    };
    let mut playback = Backend::new(&graph, backend.media).unwrap();
    let mut replay = vec![Fixed::ZERO; 32];
    playback.process(&[], &mut [&mut replay]).unwrap();
    assert_eq!(output, replay);
    println!("Recorded and replayed {} frames exactly.", replay.len());
}
