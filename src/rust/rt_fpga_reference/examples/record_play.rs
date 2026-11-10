//! Record and play through the same immutable node program.
use rt_fpga_reference::{
    compiler::{Graph, Node, NodeId},
    fixed::Fixed,
    storage::MediaStore,
    Backend,
};

fn main() {
    let mut media = MediaStore::new(32).unwrap();
    let take = media.create_recording().unwrap();
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
            Node::Playback {
                view: take,
                length: 32,
                position: 0,
                playing: false,
            },
            Node::Output {
                input: NodeId(1),
                port: 0,
            },
            Node::Output {
                input: NodeId(3),
                port: 1,
            },
        ],
    };
    let mut backend = Backend::new(&graph, media).unwrap();
    let program = backend.program.clone();
    println!("Persistent node program:\n{program:#?}");
    let input: Vec<_> = (0..32)
        .map(|frame| Fixed::from_f32(frame as f32 / 64.))
        .collect();
    let mut monitor = vec![Fixed::ZERO; 32];
    let mut playback = vec![Fixed::ZERO; 32];
    backend.transitions.begin(&mut backend.state).unwrap();
    backend
        .state
        .run(&backend.program, &mut backend.queue)
        .unwrap();
    println!(
        "Recording jobs:\n{:#?}",
        backend.queue.iter().collect::<Vec<_>>()
    );
    backend
        .jobs
        .run(
            &mut backend.queue,
            &mut backend.media,
            &[&input],
            &mut [&mut monitor, &mut playback],
        )
        .unwrap();
    backend
        .transitions
        .finish(&mut backend.state, backend.program.quantum)
        .unwrap();
    let recorded = monitor.clone();
    backend.set_enabled(NodeId(2), false).unwrap();
    backend.set_enabled(NodeId(3), true).unwrap();
    backend.transitions.begin(&mut backend.state).unwrap();
    backend
        .state
        .run(&backend.program, &mut backend.queue)
        .unwrap();
    println!(
        "Playback jobs:\n{:#?}",
        backend.queue.iter().collect::<Vec<_>>()
    );
    backend
        .jobs
        .run(
            &mut backend.queue,
            &mut backend.media,
            &[&input],
            &mut [&mut monitor, &mut playback],
        )
        .unwrap();
    backend
        .transitions
        .finish(&mut backend.state, backend.program.quantum)
        .unwrap();
    assert_eq!(playback, recorded);
    assert_eq!(backend.program, program);
    println!(
        "Recorded and replayed {} frames without changing the program.",
        playback.len()
    );
}
