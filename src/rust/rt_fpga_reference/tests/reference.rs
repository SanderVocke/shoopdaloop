use rt_fpga_reference::{
    compiler::{compile, Graph, Node, NodeId},
    fixed::{Fixed, Ramp},
    jobs::{Job, JobProcessor, JobQueue},
    state::StateProcessor,
    storage::MediaStore,
    Backend, Error,
};
use shoop_wasm_test_support::shoop_test;

fn fixed(samples: &[f32]) -> Vec<Fixed> {
    samples
        .iter()
        .map(|&sample| Fixed::from_f32(sample))
        .collect()
}
fn render(backend: &mut Backend, input: &[Fixed]) -> Vec<Fixed> {
    let mut output = vec![Fixed::ZERO; input.len()];
    backend.process(&[input], &mut [&mut output]).unwrap();
    output
}

#[shoop_test]
fn state_execution_emits_jobs_without_accessing_or_resolving_samples() {
    let graph = Graph {
        quantum: 32,
        nodes: vec![
            Node::Playback {
                view: rt_fpga_reference::storage::ViewId(999),
                length: 45,
                position: 40,
                playing: true,
            },
            Node::Output {
                input: NodeId(0),
                port: 0,
            },
        ],
    };
    let compiled = compile(&graph).unwrap();
    let mut state = StateProcessor::new(compiled.initial_state);
    let mut queue = JobQueue::new(compiled.program.jobs_per_quantum);
    state.run(&compiled.program, &mut queue).unwrap();
    assert!(matches!(
        queue.iter().next(),
        Some(Job::ReadView {
            position: 40,
            length: 45,
            ..
        })
    ));
    let mut media = MediaStore::new(32).unwrap();
    let mut jobs = JobProcessor::new(32, 2);
    assert_eq!(
        jobs.run(&mut queue, &mut media, &[], &mut []),
        Err(Error::InvalidView)
    );
}

#[shoop_test]
fn recording_preserves_partial_chunks_and_playback_wraps_independently_of_chunk_size() {
    for chunk_frames in [1, 32, 128] {
        let mut media = MediaStore::new(chunk_frames).unwrap();
        let take = media.create_recording();
        let graph = Graph {
            quantum: 3,
            nodes: vec![
                Node::Input { port: 0 },
                Node::Capture {
                    input: NodeId(0),
                    view: take,
                    recording: true,
                },
            ],
        };
        let mut backend = Backend::new(&graph, media).unwrap();
        let samples = fixed(&[0.125, -0.25, 0.5]);
        backend.process(&[&samples], &mut []).unwrap();
        backend.process(&[&samples], &mut []).unwrap();
        backend.set_enabled(NodeId(1), false).unwrap();
        backend.process(&[&fixed(&[1., 1., 1.])], &mut []).unwrap();
        assert_eq!(backend.media.frames(take), Ok(6));
        let graph = Graph {
            quantum: 8,
            nodes: vec![
                Node::Playback {
                    view: take,
                    length: 5,
                    position: 4,
                    playing: true,
                },
                Node::Output {
                    input: NodeId(0),
                    port: 0,
                },
            ],
        };
        let mut backend = Backend::new(&graph, backend.media).unwrap();
        let original = [samples[0], samples[1], samples[2], samples[0], samples[1]];
        for quantum in 0..4 {
            let result = render(&mut backend, &[Fixed::ZERO; 8]);
            for (offset, &sample) in result.iter().enumerate() {
                assert_eq!(sample, original[(4 + quantum * 8 + offset) % 5]);
            }
        }
        backend.set_enabled(NodeId(0), false).unwrap();
        assert_eq!(
            render(&mut backend, &[Fixed::ZERO; 8]),
            vec![Fixed::ZERO; 8]
        );
        backend.seek(NodeId(0), 2).unwrap();
        backend.set_enabled(NodeId(0), true).unwrap();
        assert_eq!(render(&mut backend, &[Fixed::ZERO; 8])[0], original[2]);
    }
}

#[shoop_test]
fn playback_and_capture_share_a_view_in_either_signal_order() {
    for quantum in [1, 4] {
        for capture_first in [false, true] {
            let mut media = MediaStore::new(3).unwrap();
            let view = media.create_recording();
            let capture_node = Node::Capture {
                input: NodeId(0),
                view,
                recording: true,
            };
            let playback_node = Node::Playback {
                view,
                length: quantum as u64,
                position: 0,
                playing: false,
            };
            let (capture, playback) = if capture_first {
                (NodeId(1), NodeId(2))
            } else {
                (NodeId(2), NodeId(1))
            };
            let graph = Graph {
                quantum,
                nodes: vec![
                    Node::Input { port: 0 },
                    if capture_first {
                        capture_node.clone()
                    } else {
                        playback_node.clone()
                    },
                    if capture_first {
                        playback_node
                    } else {
                        capture_node
                    },
                    Node::Output {
                        input: playback,
                        port: 0,
                    },
                ],
            };
            let mut backend = Backend::new(&graph, media).unwrap();
            let program = backend.program.clone();
            let input = fixed(&[0.125, 0.25, 0.5, 0.75][..quantum]);
            let silence = vec![Fixed::ZERO; quantum];
            assert_eq!(render(&mut backend, &input), silence);
            backend.set_enabled(capture, false).unwrap();
            backend.set_enabled(playback, true).unwrap();
            assert_eq!(render(&mut backend, &silence), input);
            assert_eq!(backend.media.frames(view), Ok(quantum));

            backend.set_enabled(capture, true).unwrap();
            let appended = vec![Fixed::ONE; quantum];
            assert_eq!(render(&mut backend, &appended), input);
            assert_eq!(backend.media.frames(view), Ok(2 * quantum));
            backend.set_enabled(capture, false).unwrap();
            assert_eq!(render(&mut backend, &silence), input);
            let mut recorded = vec![Fixed::ZERO; 2 * quantum];
            backend
                .media
                .read_loop(view, 0, (2 * quantum) as u64, &mut recorded)
                .unwrap();
            assert_eq!(recorded, [input, appended].concat());
            assert_eq!(backend.program, program);
        }
    }
}

#[shoop_test]
fn view_reads_require_existing_recorded_ranges() {
    let mut media = MediaStore::new(2).unwrap();
    let view = media.create_recording();
    let mut output = [Fixed::ZERO; 4];
    assert_eq!(
        media.read_loop(view, 0, 1, &mut output),
        Err(Error::InvalidView)
    );
    let samples = fixed(&[0.125, 0.25, 0.5]);
    media.append(view, &samples).unwrap();
    for (position, length) in [(0, 0), (0, 4), (3, 3)] {
        assert_eq!(
            media.read_loop(view, position, length, &mut output),
            Err(Error::InvalidView)
        );
    }
    media.read_loop(view, 2, 3, &mut output).unwrap();
    assert_eq!(output, [samples[2], samples[0], samples[1], samples[2]]);
    media.append(view, &[Fixed::ONE]).unwrap();
    media.read_loop(view, 3, 4, &mut output).unwrap();
    assert_eq!(output, [Fixed::ONE, samples[0], samples[1], samples[2]]);
}

#[shoop_test]
fn fanout_mixing_and_ramps_have_explicit_sample_semantics() {
    let graph = Graph {
        quantum: 3,
        nodes: vec![
            Node::Input { port: 0 },
            Node::Gain {
                input: NodeId(0),
                gain: Fixed::ONE,
            },
            Node::Gain {
                input: NodeId(0),
                gain: Fixed::from_f32(-0.5),
            },
            Node::Mix {
                left: NodeId(1),
                right: NodeId(2),
            },
            Node::Output {
                input: NodeId(3),
                port: 0,
            },
        ],
    };
    let mut backend = Backend::new(&graph, MediaStore::new(32).unwrap()).unwrap();
    backend.set_gain(NodeId(1), Fixed::ZERO, 5).unwrap();
    let ramp = Ramp::new(Fixed::ONE, Fixed::ZERO, 5);
    for quantum in 0..4 {
        let output = render(&mut backend, &[Fixed::ONE; 3]);
        for (offset, sample) in output.iter().enumerate() {
            assert_eq!(
                *sample,
                ramp.at((quantum * 3 + offset) as u32)
                    .saturating_add(Fixed::from_f32(-0.5))
            );
        }
    }
}

#[shoop_test]
fn compiler_schedules_forward_references_and_rejects_invalid_graphs() {
    let graph = Graph {
        quantum: 1,
        nodes: vec![
            Node::Output {
                input: NodeId(1),
                port: 0,
            },
            Node::Input { port: 0 },
        ],
    };
    let mut backend = Backend::new(&graph, MediaStore::new(1).unwrap()).unwrap();
    assert_eq!(render(&mut backend, &[Fixed::ONE]), vec![Fixed::ONE]);
    for nodes in [
        vec![Node::Gain {
            input: NodeId(0),
            gain: Fixed::ONE,
        }],
        vec![Node::Output {
            input: NodeId(99),
            port: 0,
        }],
        vec![
            Node::Input { port: 0 },
            Node::Output {
                input: NodeId(0),
                port: 0,
            },
            Node::Output {
                input: NodeId(0),
                port: 0,
            },
        ],
    ] {
        assert!(matches!(
            compile(&Graph { quantum: 1, nodes }),
            Err(Error::InvalidGraph)
        ));
    }
}

#[shoop_test]
fn fifo_refusal_does_not_advance_state_and_unconsumed_jobs_are_not_overwritten() {
    let graph = Graph {
        quantum: 32,
        nodes: vec![Node::Playback {
            view: rt_fpga_reference::storage::ViewId(0),
            length: 100,
            position: 0,
            playing: true,
        }],
    };
    let compiled = compile(&graph).unwrap();
    let mut transitions =
        rt_fpga_reference::transitions::TransitionProcessor::new(&compiled.bindings);
    let mut state = StateProcessor::new(compiled.initial_state);
    assert_eq!(
        state.run(&compiled.program, &mut JobQueue::new(0)),
        Err(Error::QueueFull)
    );
    let mut queue = JobQueue::new(1);
    transitions.begin(&mut state).unwrap();
    state.run(&compiled.program, &mut queue).unwrap();
    assert!(matches!(
        queue.iter().next(),
        Some(Job::ReadView { position: 0, .. })
    ));
    assert_eq!(
        state.run(&compiled.program, &mut queue),
        Err(Error::QueueFull)
    );
    queue.pop();
    transitions
        .finish(&mut state, compiled.program.quantum)
        .unwrap();
    transitions.begin(&mut state).unwrap();
    state.run(&compiled.program, &mut queue).unwrap();
    assert!(matches!(
        queue.iter().next(),
        Some(Job::ReadView { position: 32, .. })
    ));
}

#[shoop_test]
fn fixed_point_saturates_and_extreme_ramps_do_not_overflow() {
    assert_eq!(Fixed(i32::MAX).saturating_add(Fixed::ONE), Fixed(i32::MAX));
    assert_eq!(
        Fixed(i32::MIN).multiply(Fixed::from_f32(-1.)),
        Fixed(i32::MAX)
    );
    let ramp = Ramp::new(Fixed(i32::MIN), Fixed(i32::MAX), u32::MAX);
    assert_eq!(ramp.at(0), Fixed(i32::MIN));
    assert_eq!(ramp.at(u32::MAX), Fixed(i32::MAX));
    assert_eq!(ramp.at(1 << 31), Fixed(0));
}

#[shoop_test]
fn one_node_program_handles_record_stop_play_seek_and_ramps_without_recompilation() {
    let mut media = MediaStore::new(32).unwrap();
    let view = media.create_recording();
    let graph = Graph {
        quantum: 4,
        nodes: vec![
            Node::Playback {
                view,
                length: 8,
                position: 0,
                playing: false,
            },
            Node::Input { port: 0 },
            Node::Gain {
                input: NodeId(0),
                gain: Fixed::ONE,
            },
            Node::Capture {
                input: NodeId(1),
                view,
                recording: true,
            },
            Node::Output {
                input: NodeId(2),
                port: 0,
            },
        ],
    };
    let mut backend = Backend::new(&graph, media).unwrap();
    let program = backend.program.clone();
    let address = backend.program.instructions.as_ptr();
    let input = fixed(&[0.125, 0.25, 0.5, 0.75]);
    assert_eq!(render(&mut backend, &input), vec![Fixed::ZERO; 4]);
    render(&mut backend, &input);
    backend.set_enabled(NodeId(3), false).unwrap();
    backend.set_enabled(NodeId(0), true).unwrap();
    assert_eq!(render(&mut backend, &[Fixed::ZERO; 4]), input);
    assert_eq!(backend.media.frames(view), Ok(8));
    backend.set_enabled(NodeId(0), false).unwrap();
    assert_eq!(
        render(&mut backend, &[Fixed::ZERO; 4]),
        vec![Fixed::ZERO; 4]
    );
    backend.seek(NodeId(0), 2).unwrap();
    backend.set_gain(NodeId(2), Fixed::ZERO, 3).unwrap();
    backend.set_enabled(NodeId(0), true).unwrap();
    let output = render(&mut backend, &[Fixed::ZERO; 4]);
    assert_eq!(output[0], input[2]);
    assert_eq!(output[3], Fixed::ZERO);
    assert_eq!(backend.program, program);
    assert_eq!(backend.program.instructions.as_ptr(), address);
}

#[shoop_test]
fn transitions_follow_source_clocks_in_both_signal_orders_and_state_execution_cannot_advance_them()
{
    use rt_fpga_reference::{compiler::Binding, transitions::OnWrap};
    for source_first in [false, true] {
        let mut media = MediaStore::new(32).unwrap();
        let target_view = media.insert(&fixed(&[0.125, 0.25, 0.5, 0.75])).unwrap();
        let source_view = media.insert(&[Fixed::ONE; 4]).unwrap();
        let (source, target) = if source_first {
            (NodeId(0), NodeId(1))
        } else {
            (NodeId(1), NodeId(0))
        };
        let target_node = Node::Playback {
            view: target_view,
            length: 4,
            position: 0,
            playing: false,
        };
        let source_node = Node::Playback {
            view: source_view,
            length: 4,
            position: 0,
            playing: true,
        };
        let graph = Graph {
            quantum: 2,
            nodes: vec![
                if source_first {
                    source_node.clone()
                } else {
                    target_node.clone()
                },
                if source_first {
                    target_node
                } else {
                    source_node
                },
                Node::Output {
                    input: target,
                    port: 0,
                },
            ],
        };
        let compiled = compile(&graph).unwrap();
        let Binding::Playback { position, .. } = compiled.bindings[target.0] else {
            panic!("playback binding");
        };
        let mut backend = Backend::new(&graph, media).unwrap();
        let program = backend.program.clone();
        backend
            .transitions
            .on_wrap(OnWrap {
                source,
                target,
                enabled: true,
            })
            .unwrap();
        assert_eq!(
            render(&mut backend, &[Fixed::ZERO; 2]),
            vec![Fixed::ZERO; 2]
        );
        assert_eq!(
            render(&mut backend, &[Fixed::ZERO; 2]),
            vec![Fixed::ZERO; 2]
        );
        assert_eq!(
            render(&mut backend, &[Fixed::ZERO; 2]),
            fixed(&[0.125, 0.25])
        );
        assert_eq!(backend.transitions.frame(), 6);
        assert_eq!(backend.state.read(position).unwrap().word().unwrap(), 2);
        assert_eq!(
            backend
                .transitions
                .finish(&mut backend.state, backend.program.quantum),
            Err(Error::InvalidState)
        );
        for _ in 0..2 {
            backend
                .state
                .run(&backend.program, &mut backend.queue)
                .unwrap();
            assert_eq!(backend.transitions.frame(), 6);
            assert_eq!(backend.state.read(position).unwrap().word().unwrap(), 2);
            while backend.queue.pop().is_some() {}
        }
        assert_eq!(backend.program, program);
    }
}
