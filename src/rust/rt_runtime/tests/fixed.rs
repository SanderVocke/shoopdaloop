mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    fixed::{self, Block, BufferedHost, Processor, Quantum, Runtime, State, Tick},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    resource::ResourceStore,
    Error, Frame, NodeId,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[derive(Default)]
struct Clock {
    callbacks: usize,
    ticks: usize,
    next: u64,
    fail_at: Option<u64>,
}
impl State for Clock {
    fn begin_callback(&mut self, _: Frame, _: &mut ResourceStore) -> Result<(), Error> {
        self.callbacks += 1;
        Ok(())
    }
    fn begin_tick(&mut self, t: Tick, _: &mut ResourceStore) -> Result<(), Error> {
        assert_eq!(t.frame, Frame(self.next));
        if self.fail_at == Some(t.frame.0) {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
    fn end_tick(&mut self, t: Tick, _: &mut ResourceStore) -> Result<(), Error> {
        self.ticks += 1;
        self.next = t.end()?.0;
        Ok(())
    }
}
struct Gain(f32);
impl Processor<Clock> for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
    fn process(&mut self, b: &mut Block<'_>, _: &mut Clock) -> Result<(), Error> {
        let (input, output) = b.audio.pair(0, 0).unwrap();
        for (i, o) in input.iter().zip(output) {
            *o = *i * self.0;
        }
        {
            for index in 0..b.midi_inputs[0].len() {
                let (at, packet) = b.midi_inputs[0].get(index).unwrap();
                b.midi_outputs[0].push(at, b.tick.quantum.frames(), packet)?;
            }
            Ok(())
        }
    }
}
fn config(q: usize) -> fixed::PreparedConfig {
    fixed::Config {
        quantum: Quantum::new(q).unwrap(),
        max_host_frames: 256,
        capacities: rt_runtime::Config {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 1,
            midi_outputs: 1,
            ..Default::default()
        },
    }
    .prepare()
    .unwrap()
}
fn runtime(q: usize, clock: Clock) -> Runtime<Gain, Clock> {
    let c = config(q);
    let a = NodeId::from_parts(9, 1).unwrap();
    let b = NodeId::from_parts(2, 1).unwrap();
    let mut connections = vec![];
    for kind in [PortType::Audio, PortType::Midi] {
        for (source, destination) in [
            (Endpoint::host(kind, 0), Endpoint::node(a, kind, 0)),
            (Endpoint::node(a, kind, 0), Endpoint::node(b, kind, 0)),
            (Endpoint::node(b, kind, 0), Endpoint::host(kind, 0)),
            (Endpoint::host(kind, 0), Endpoint::host(kind, 0)),
        ] {
            connections.push(Connection {
                source,
                destination,
            });
        }
    }
    Runtime::prepare(
        c,
        vec![(b, Gain(0.5)), (a, Gain(4.))],
        clock,
        ResourceStore::new(c.resources()).unwrap(),
        &GraphDescription {
            nodes: vec![b, a],
            connections,
        },
    )
    .unwrap()
}

#[shoop_test]
fn alignment_is_checked_and_never_executes_early() {
    for q in [16, 32, 64, 128] {
        let quantum = Quantum::new(q).unwrap();
        for f in 0..1024u64 {
            let expected = f.div_ceil(q as u64) * q as u64;
            assert_eq!(quantum.ceil(Frame(f)), Ok(Frame(expected)));
            assert_eq!(
                quantum.duration_ticks(expected.max(q as u64)),
                Ok(expected.max(q as u64) / q as u64)
            );
        }
        assert_eq!(quantum.ceil(Frame(u64::MAX)), Err(Error::InvalidFrame));
        assert_eq!(
            quantum.effective(Frame(1), Frame(4 * q as u64)),
            Ok(Frame(4 * q as u64))
        );
        assert_eq!(
            quantum.duration_ticks(q as u64 + 1),
            Err(Error::InvalidFrame)
        );
    }
    for q in [0, 1, 15, 33, 256] {
        assert_eq!(Quantum::new(q), Err(Error::InvalidConfig));
    }
}

#[shoop_test]
fn prebound_dag_and_midi_match_scalar_values_for_every_quantum() {
    for q in [16, 32, 64, 128] {
        let mut r = runtime(q, Clock::default());
        let input: Vec<_> = (0..256).map(|x| x as f32).collect();
        let mut output = vec![-9.; 256];
        let mut mi = vec![MidiBuffer::new(256, 1024).unwrap()];
        let mut mo = vec![MidiBuffer::new(512, 2048).unwrap()];
        for i in [0, q - 1, q, 255] {
            mi[0].push(i as u32, 256, &[0x90, 60, 100]).unwrap();
        }
        assert_no_heap(|| {
            r.process(
                AudioBlock {
                    frame: Frame(0),
                    frames: 256,
                    inputs: &[&input],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &mi,
                    outputs: &mut mo,
                },
            )
        })
        .unwrap();
        assert!(output.iter().zip(&input).all(|(o, i)| *o == *i * 3.));
        assert_eq!(r.state().callbacks, 1);
        assert_eq!(r.state().ticks, 256 / q);
        assert_eq!(r.diagnostics().edges, 8 * 256 / q);
        assert_eq!(r.diagnostics().nodes, 2 * 256 / q);
        assert_eq!(mo[0].len(), 8);
        for (i, at) in [0, q - 1, q, 255].into_iter().enumerate() {
            assert_eq!(mo[0].get(i * 2).unwrap().0, at as u32);
            assert_eq!(mo[0].get(i * 2 + 1).unwrap().0, at as u32);
        }
    }
}

#[shoop_test]
fn buffered_audio_and_midi_are_partition_invariant_at_constant_latency() {
    for q in [16, 32, 64, 128] {
        let mut reference = None;
        for partitions in [
            &[256usize][..],
            &[1, 3, 17, 5, 71, 2, 29][..],
            &[32, 64, 128][..],
        ] {
            let mut host = BufferedHost::prepare(runtime(q, Clock::default())).unwrap();
            let mut all_audio = vec![];
            let mut all_midi = vec![];
            let mut at = 0;
            let mut p = 0;
            while at < 1024 {
                let frames = partitions[p % partitions.len()].min(1024 - at);
                p += 1;
                let input: Vec<_> = (at..at + frames).map(|i| i as f32 + 1.).collect();
                let mut output = vec![0.; frames];
                let mut mi = vec![MidiBuffer::new(256, 1024).unwrap()];
                let mut mo = vec![MidiBuffer::new(512, 2048).unwrap()];
                for offset in 0..frames {
                    if (at + offset) % 13 == 0 {
                        mi[0].push(offset as u32, frames, &[0x90, 60, 100]).unwrap();
                    }
                }
                assert_no_heap(|| {
                    host.process(
                        AudioBlock {
                            frame: Frame(at as u64),
                            frames,
                            inputs: &[&input],
                            outputs: &mut [&mut output],
                        },
                        MidiBlock {
                            inputs: &mi,
                            outputs: &mut mo,
                        },
                    )
                })
                .unwrap();
                all_audio.extend(output);
                for i in 0..mo[0].len() {
                    all_midi.push(at + mo[0].get(i).unwrap().0 as usize);
                }
                at += frames;
            }
            for (i, value) in all_audio.iter().enumerate() {
                assert_eq!(*value, if i < q { 0. } else { (i - q + 1) as f32 * 3. });
            }
            let expected: Vec<_> = (0..1024 - q)
                .filter(|i| i % 13 == 0)
                .flat_map(|i| [i + q, i + q])
                .collect();
            assert_eq!(all_midi, expected);
            assert_eq!(host.latency_frames(), q);
            assert_eq!(host.next_frame(), Frame(1024));
            assert_eq!(host.runtime().next_frame(), Frame(1024));
            if let Some((audio, midi)) = &reference {
                assert_eq!(&all_audio, audio);
                assert_eq!(&all_midi, midi)
            } else {
                reference = Some((all_audio, all_midi));
            }
        }
    }
}

#[shoop_test]
fn invalid_blocks_are_inert_and_execution_faults_are_terminal() {
    let mut r = runtime(
        32,
        Clock {
            fail_at: Some(32),
            ..Default::default()
        },
    );
    let input = [1.; 128];
    let mut output = [9.; 128];
    let mi = vec![MidiBuffer::new(4, 16).unwrap()];
    let mut mo = vec![MidiBuffer::new(4, 16).unwrap()];
    assert_eq!(
        r.process(
            AudioBlock {
                frame: Frame(0),
                frames: 33,
                inputs: &[&input],
                outputs: &mut [&mut output]
            },
            MidiBlock {
                inputs: &mi,
                outputs: &mut mo
            }
        ),
        Err(Error::InvalidFrame)
    );
    assert_eq!(r.state().callbacks, 0);
    assert_eq!(r.next_frame(), Frame(0));
    assert_eq!(
        assert_no_heap(|| r.process(
            AudioBlock {
                frame: Frame(0),
                frames: 128,
                inputs: &[&input],
                outputs: &mut [&mut output]
            },
            MidiBlock {
                inputs: &mi,
                outputs: &mut mo
            }
        )),
        Err(Error::InvalidState)
    );
    assert_eq!(output, [0.; 128]);
    assert_eq!(r.state().ticks, 1);
    assert_eq!(
        r.process(
            AudioBlock {
                frame: r.next_frame(),
                frames: 32,
                inputs: &[&input],
                outputs: &mut [&mut output]
            },
            MidiBlock {
                inputs: &mi,
                outputs: &mut mo
            }
        ),
        Err(Error::InvalidState)
    );
    assert_eq!(r.state().callbacks, 1);
    let mut reset_sink = MidiBuffer::new(1, 3).unwrap();
    let mut resets = 0;
    while r.midi_recovery_pending(0).unwrap() {
        reset_sink.clear();
        resets += assert_no_heap(|| r.drain_midi_recovery(0, &mut reset_sink, 32)).unwrap();
    }
    assert_eq!(resets, rt_runtime::midi::ResetState::EVENTS);
    assert_no_heap(|| {
        r.process(
            AudioBlock {
                frame: Frame(u64::MAX),
                frames: 0,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
}

#[shoop_test]
fn command_credits_survive_unread_results_and_cancellation_uses_reserved_capacity() {
    let (mut control, mut queue) =
        fixed::command_channel::<u32, u32>(Quantum::default(), 2).unwrap();
    let first = control.submit(Frame(100), 1).unwrap();
    let second = control.submit(Frame(1), 2).unwrap();
    assert_eq!(control.submit(Frame(0), 3), Err(Error::Full));
    let cancel = control.cancel(first).unwrap();
    assert_no_heap(|| queue.begin_callback(Frame(0))).unwrap();
    assert_eq!(control.submit(Frame(0), 3), Err(Error::Full));
    assert_no_heap(|| queue.dispatch(Frame(0), Ok)).unwrap();
    assert_no_heap(|| queue.dispatch(Frame(32), Ok)).unwrap();
    assert_eq!(queue.pending(), 0);
    let a = control.try_receipt().unwrap();
    assert_eq!(a.ticket, first);
    assert_eq!(a.result, Err(Error::Cancelled));
    let b = control.try_receipt().unwrap();
    assert_eq!(b.ticket, cancel);
    assert_eq!(b.command, None);
    let c = control.try_receipt().unwrap();
    assert_eq!(c.ticket, second);
    assert_eq!(c.requested, Frame(1));
    assert_eq!(c.effective, Some(Frame(32)));
    assert_eq!(c.actual, Frame(32));
    assert_eq!(control.outstanding(), 0);
    let next = control.submit(Frame(0), 4).unwrap();
    assert_no_heap(|| queue.begin_callback(Frame(64))).unwrap();
    assert_no_heap(|| queue.dispatch(Frame(64), Ok)).unwrap();
    let c = control.try_receipt().unwrap();
    assert_eq!(c.ticket, next);
    assert_eq!(c.effective, Some(Frame(64)));
    let (mut other, _other_queue) =
        fixed::command_channel::<u32, u32>(Quantum::default(), 2).unwrap();
    assert_eq!(other.cancel(next), Err(Error::InvalidId));
}

#[shoop_test]
fn scheduled_commands_are_ordered_without_blocking_earlier_deadlines() {
    let (mut control, mut queue) =
        fixed::command_channel::<u32, u32>(Quantum::default(), 8).unwrap();
    control.submit(Frame(96), 3).unwrap();
    control.submit(Frame(1), 1).unwrap();
    control.submit(Frame(32), 2).unwrap();
    control.submit(Frame(u64::MAX), 4).unwrap();
    queue.begin_callback(Frame(0)).unwrap();
    let rejected = control.try_receipt().unwrap();
    assert_eq!(rejected.result, Err(Error::InvalidFrame));
    let mut seen = Vec::with_capacity(3);
    assert_no_heap(|| {
        for t in [0, 32, 64, 96] {
            queue
                .dispatch(Frame(t), |c| {
                    seen.push(c);
                    Ok(c)
                })
                .unwrap();
        }
    });
    assert_eq!(seen, [1, 2, 3]);
}

#[shoop_test]
fn host_copy_budget_is_not_renewed_by_internal_ticks() {
    use rt_runtime::resource::{Description, Format, UploadId};
    struct Writes {
        upload: UploadId,
        offset: usize,
    }
    impl State for Writes {
        fn begin_tick(&mut self, _: Tick, store: &mut ResourceStore) -> Result<(), Error> {
            store.write_upload(self.upload, self.offset, &[0; 16])?;
            self.offset += 16;
            Ok(())
        }
    }
    struct Never;
    impl Processor<Writes> for Never {
        fn ports(&self) -> PortCounts {
            PortCounts::default()
        }
        fn process(&mut self, _: &mut Block<'_>, _: &mut Writes) -> Result<(), Error> {
            Err(Error::InvalidState)
        }
    }
    let c = fixed::Config {
        quantum: Quantum::new(16).unwrap(),
        max_host_frames: 128,
        capacities: rt_runtime::Config {
            audio_inputs: 0,
            audio_outputs: 0,
            midi_inputs: 0,
            midi_outputs: 0,
            page_bytes: 32,
            budgets: rt_runtime::Budgets {
                transfer_bytes: 64,
                validation_steps: 128,
                ..Default::default()
            },
            ..Default::default()
        },
    }
    .prepare()
    .unwrap();
    let mut store = ResourceStore::new(c.resources()).unwrap();
    let resource = store.create().unwrap();
    let upload = store
        .begin_upload(
            resource,
            rt_runtime::Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 32,
                bytes: 128,
            },
        )
        .unwrap();
    let mut r = Runtime::<Never, _>::prepare(
        c,
        vec![],
        Writes { upload, offset: 0 },
        store,
        &GraphDescription {
            nodes: vec![],
            connections: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        assert_no_heap(|| r.process(
            AudioBlock {
                frame: Frame(0),
                frames: 128,
                inputs: &[],
                outputs: &mut []
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut []
            }
        )),
        Err(Error::Budget)
    );
    assert_eq!(r.state().offset, 64);
    assert_eq!(r.next_frame(), Frame(64));
}

#[shoop_test]
fn command_ingress_budget_does_not_renew_at_ticks_and_cancellation_can_wait_for_admission() {
    let (mut control, mut queue) =
        fixed::command_channel_with_budget::<u32, u32>(Quantum::default(), 3, 1).unwrap();
    control.submit(Frame(0), 1).unwrap();
    control.submit(Frame(0), 2).unwrap();
    let target = control.submit(Frame(256), 3).unwrap();
    let cancellation = control.cancel(target).unwrap();
    queue.begin_callback(Frame(0)).unwrap();
    let mut seen = Vec::with_capacity(2);
    assert_no_heap(|| {
        for frame in [0, 32, 64, 96] {
            queue
                .dispatch(Frame(frame), |v| {
                    seen.push(v);
                    Ok(v)
                })
                .unwrap();
        }
    });
    assert_eq!(seen, [1]);
    assert_eq!(queue.pending(), 0);
    queue.begin_callback(Frame(128)).unwrap();
    assert_no_heap(|| {
        for frame in [128, 160, 192, 224] {
            queue
                .dispatch(Frame(frame), |v| {
                    seen.push(v);
                    Ok(v)
                })
                .unwrap();
        }
    });
    assert_eq!(seen, [1, 2]);
    queue.begin_callback(Frame(256)).unwrap();
    queue.dispatch(Frame(256), Ok).unwrap();
    let mut receipts = Vec::new();
    while let Some(r) = control.try_receipt() {
        receipts.push(r);
    }
    assert_eq!(receipts.len(), 4);
    assert!(receipts
        .iter()
        .any(|r| r.ticket == target && r.result == Err(Error::Cancelled)));
    assert!(receipts
        .iter()
        .any(|r| r.ticket == cancellation && r.result == Ok(0)));
}

#[shoop_test]
fn quiescent_shutdown_settles_unread_admitted_and_unadmitted_requests_exactly_once() {
    let (mut control, mut queue) =
        fixed::command_channel_with_budget::<u32, u32>(Quantum::default(), 4, 1).unwrap();
    let done = control.submit(Frame(0), 10).unwrap();
    let admitted = control.submit(Frame(96), 20).unwrap();
    let ingress = control.submit(Frame(128), 30).unwrap();
    let target = control.submit(Frame(256), 40).unwrap();
    let cancel = control.cancel(target).unwrap();
    queue.begin_callback(Frame(0)).unwrap();
    queue.dispatch(Frame(0), Ok).unwrap();
    queue.begin_callback(Frame(32)).unwrap();
    assert_eq!(control.outstanding(), 5);
    let (mut other, _) = fixed::command_channel::<u32, u32>(Quantum::default(), 1).unwrap();
    assert_eq!(queue.shutdown(&mut other, Frame(32)), Err(Error::InvalidId));
    assert_eq!(
        queue.shutdown(&mut control, Frame(33)),
        Err(Error::InvalidFrame)
    );
    assert_no_heap(|| queue.shutdown(&mut control, Frame(32))).unwrap();
    assert_no_heap(|| queue.shutdown(&mut control, Frame(32))).unwrap();
    assert_eq!(queue.pending(), 0);
    assert_eq!(control.submit(Frame(32), 50), Err(Error::Disconnected));
    assert_eq!(control.cancel(target), Err(Error::Disconnected));
    assert_eq!(queue.begin_callback(Frame(32)), Err(Error::Disconnected));
    assert_eq!(queue.dispatch(Frame(32), Ok), Err(Error::Disconnected));
    let mut receipts = Vec::new();
    while let Some(r) = control.try_receipt() {
        receipts.push(r);
    }
    assert_eq!(receipts.len(), 5);
    assert_eq!(control.outstanding(), 0);
    for (ticket, value, effective) in [
        (done, Some(10), Some(Frame(0))),
        (admitted, Some(20), Some(Frame(96))),
        (ingress, Some(30), None),
        (target, Some(40), None),
        (cancel, None, None),
    ] {
        let matches: Vec<_> = receipts.iter().filter(|r| r.ticket == ticket).collect();
        assert_eq!(matches.len(), 1);
        let r = matches[0];
        assert_eq!(r.command, value);
        assert_eq!(r.effective, effective);
        assert_eq!(r.actual, if ticket == done { Frame(0) } else { Frame(32) });
        assert_eq!(
            r.result,
            if ticket == done {
                Ok(10)
            } else {
                Err(Error::Disconnected)
            }
        );
    }
}

#[shoop_test]
fn live_routes_change_topological_order_without_replacing_processors_or_losing_midi() {
    use rt_runtime::Epoch;
    struct Live;
    impl State for Live {
        fn validate_activation(&self, graph: &GraphDescription) -> Result<(), Error> {
            if graph.nodes.contains(&NodeId::from_parts(2, 1)?) {
                Ok(())
            } else {
                Err(Error::Busy)
            }
        }
    }
    struct Counter {
        gain: f32,
        offset: f32,
        calls: u32,
    }
    impl Processor<Live> for Counter {
        fn ports(&self) -> PortCounts {
            <Gain as Processor<Clock>>::ports(&Gain(1.))
        }
        fn process(&mut self, b: &mut Block<'_>, _: &mut Live) -> Result<(), Error> {
            self.calls += 1;
            let (input, output) = b.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
            for (i, o) in input.iter().zip(output) {
                *o = self.gain * i + self.offset + self.calls as f32;
            }
            for i in 0..b.midi_inputs[0].len() {
                let (at, packet) = b.midi_inputs[0].get(i).ok_or(Error::InvalidEvent)?;
                b.midi_outputs[0].push(at, b.tick.quantum.frames(), packet)?;
            }
            Ok(())
        }
    }
    let a = NodeId::from_parts(9, 1).unwrap();
    let b = NodeId::from_parts(2, 1).unwrap();
    let graph = |order: &[NodeId]| {
        let mut connections = vec![];
        for kind in [PortType::Audio, PortType::Midi] {
            let mut source = Endpoint::host(kind, 0);
            for &node in order {
                connections.push(Connection {
                    source,
                    destination: Endpoint::node(node, kind, 0),
                });
                source = Endpoint::node(node, kind, 0);
            }
            connections.push(Connection {
                source,
                destination: Endpoint::host(kind, 0),
            });
        }
        GraphDescription {
            nodes: order.to_vec(),
            connections,
        }
    };
    for q in [16, 32, 64, 128] {
        let c = config(q);
        let original = graph(&[a, b]);
        let reversed = graph(&[b, a]);
        let mut r = Runtime::prepare(
            c,
            vec![
                (
                    b,
                    Counter {
                        gain: 0.5,
                        offset: 100.,
                        calls: 0,
                    },
                ),
                (
                    a,
                    Counter {
                        gain: 4.,
                        offset: 10.,
                        calls: 0,
                    },
                ),
            ],
            Live,
            ResourceStore::new(c.resources()).unwrap(),
            &original,
        )
        .unwrap();
        let compiler = r.graph_compiler();
        let mut control = r.prepare_graph_changes().unwrap();
        let ticket = control
            .submit(Frame(1), Epoch(0), compiler.compile(&reversed).unwrap())
            .unwrap();
        let mut input = vec![1.; 2 * q];
        let mut output = vec![0.; 2 * q];
        let mut mi = [MidiBuffer::new(16, 128).unwrap()];
        let mut mo = [MidiBuffer::new(16, 128).unwrap()];
        mi[0].push((q - 1) as u32, 2 * q, &[0x90, 60, 90]).unwrap();
        mi[0].push((q + 3) as u32, 2 * q, &[0x80, 60, 0]).unwrap();
        let render = |r: &mut Runtime<Counter, Live>,
                      input: &[f32],
                      output: &mut [f32],
                      mi: &[MidiBuffer],
                      mo: &mut [MidiBuffer],
                      frames| {
            assert_no_heap(|| {
                r.process(
                    AudioBlock {
                        frame: r.next_frame(),
                        frames,
                        inputs: &[input],
                        outputs: &mut [output],
                    },
                    MidiBlock {
                        inputs: mi,
                        outputs: mo,
                    },
                )
            })
            .unwrap();
        };
        render(&mut r, &input, &mut output, &mi, &mut mo, 2 * q);
        assert_eq!(&output[..q], vec![108.5; q]);
        assert_eq!(&output[q..], vec![422.; q]);
        assert_eq!(mo[0].len(), 2);
        for i in 0..2 {
            assert_eq!(mo[0].get(i), mi[0].get(i));
        }
        assert_eq!(r.epoch(), Epoch(1));
        let refused = control
            .submit(
                r.next_frame(),
                Epoch(1),
                compiler.compile(&original).unwrap(),
            )
            .unwrap_err();
        assert_eq!(refused.error, Error::Full);
        let receipt = control.try_receipt().unwrap();
        assert_eq!(receipt.ticket, ticket);
        assert_eq!(receipt.requested, Frame(1));
        assert_eq!(receipt.effective, Some(Frame(q as u64)));
        assert_eq!(receipt.actual, Frame(q as u64));
        assert_eq!(receipt.result, Ok(Epoch(1)));
        assert_eq!(receipt.graph.description(), &original);
        mi[0].clear();
        input.fill(2.);
        for (description, expected, result, expected_sample) in [
            (original.clone(), Epoch(0), Err(Error::StaleEpoch), 429.),
            (graph(&[a]), Epoch(1), Err(Error::Busy), 434.),
            (graph(&[b]), Epoch(1), Ok(Epoch(2)), 106.),
            (original.clone(), Epoch(2), Ok(Epoch(3)), 117.5),
        ] {
            control
                .submit(
                    r.next_frame(),
                    expected,
                    compiler.compile(&description).unwrap(),
                )
                .unwrap();
            render(&mut r, &input, &mut output, &mi, &mut mo, q);
            assert_eq!(&output[..q], vec![expected_sample; q]);
            let receipt = control.try_receipt().unwrap();
            assert_eq!(receipt.result, result);
            if result.is_err() {
                assert_eq!(receipt.graph.description(), &description);
            }
        }
        assert_eq!(r.epoch(), Epoch(3));
        let other = runtime(q, Clock::default());
        let foreign = other.graph_compiler().compile(&graph(&[a, b])).unwrap();
        assert_eq!(
            control
                .submit(r.next_frame(), r.epoch(), foreign)
                .unwrap_err()
                .error,
            Error::InvalidId
        );
        assert_no_heap(|| r.shutdown_graph_changes(&mut control)).unwrap();
    }
}

#[shoop_test]
fn graph_ingress_is_snapshotted_once_even_when_control_submits_during_a_callback() {
    use rt_runtime::Epoch;
    struct StateWithControl {
        control: Option<fixed::GraphControl>,
        candidate: Option<fixed::PreparedGraph>,
    }
    impl State for StateWithControl {
        fn validate_activation(&self, _: &GraphDescription) -> Result<(), Error> {
            Ok(())
        }
        fn begin_tick(&mut self, _: Tick, _: &mut ResourceStore) -> Result<(), Error> {
            if let Some(graph) = self.candidate.take() {
                if let Err(rejected) =
                    self.control
                        .as_mut()
                        .unwrap()
                        .submit(Frame(0), Epoch(0), graph)
                {
                    self.candidate = Some(rejected.graph);
                    return Err(rejected.error);
                }
            }
            Ok(())
        }
    }
    struct Never;
    impl Processor<StateWithControl> for Never {
        fn ports(&self) -> PortCounts {
            PortCounts::default()
        }
        fn process(&mut self, _: &mut Block<'_>, _: &mut StateWithControl) -> Result<(), Error> {
            Err(Error::InvalidState)
        }
    }
    let c = config(32);
    let mut r = Runtime::<Never, _>::prepare(
        c,
        vec![],
        StateWithControl {
            control: None,
            candidate: None,
        },
        ResourceStore::new(c.resources()).unwrap(),
        &GraphDescription {
            nodes: vec![],
            connections: vec![Connection {
                source: Endpoint::host(PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, 0),
            }],
        },
    )
    .unwrap();
    let control = r.prepare_graph_changes().unwrap();
    let candidate = control
        .compiler()
        .compile(&GraphDescription::default())
        .unwrap();
    r.state_mut().control = Some(control);
    r.state_mut().candidate = Some(candidate);
    let input = [1.; 128];
    let mut output = [9.; 128];
    let mi = [MidiBuffer::new(16, 128).unwrap()];
    let mut mo = [MidiBuffer::new(16, 128).unwrap()];
    for (frames, expected) in [
        (31, Err(Error::InvalidFrame)),
        (128, Ok(())),
        (0, Ok(())),
        (32, Ok(())),
    ] {
        let epoch = r.epoch();
        assert_eq!(
            assert_no_heap(|| r.process(
                AudioBlock {
                    frame: r.next_frame(),
                    frames,
                    inputs: &[&input],
                    outputs: &mut [&mut output]
                },
                MidiBlock {
                    inputs: &mi,
                    outputs: &mut mo
                }
            )),
            expected
        );
        if frames == 128 {
            assert_eq!(output, [1.; 128]);
            assert_eq!(r.epoch(), Epoch(0));
            assert!(r
                .state_mut()
                .control
                .as_mut()
                .unwrap()
                .try_receipt()
                .is_none());
        } else if frames == 32 {
            assert_eq!(output[..32], [0.; 32]);
            assert_eq!(r.epoch(), Epoch(1));
        } else {
            assert_eq!(r.epoch(), epoch);
        }
    }
    let receipt = r
        .state_mut()
        .control
        .as_mut()
        .unwrap()
        .try_receipt()
        .unwrap();
    assert_eq!(receipt.requested, Frame(0));
    assert_eq!(receipt.effective, Some(Frame(128)));
    assert_eq!(receipt.actual, Frame(128));
}

#[shoop_test]
fn graph_shutdown_returns_owning_candidates_after_faults_and_before_admission() {
    use rt_runtime::Epoch;
    for admitted in [false, true] {
        let mut r = runtime(
            32,
            Clock {
                fail_at: Some(0),
                ..Default::default()
            },
        );
        let mut control = r.prepare_graph_changes().unwrap();
        let graph = control
            .compiler()
            .compile(&GraphDescription::default())
            .unwrap();
        let ticket = control.submit(Frame(1024), Epoch(0), graph).unwrap();
        if admitted {
            let mi = [MidiBuffer::new(16, 128).unwrap()];
            let mut mo = [MidiBuffer::new(16, 128).unwrap()];
            assert_eq!(
                assert_no_heap(|| r.process(
                    AudioBlock {
                        frame: Frame(0),
                        frames: 32,
                        inputs: &[&[0.; 32]],
                        outputs: &mut [&mut [0.; 32]]
                    },
                    MidiBlock {
                        inputs: &mi,
                        outputs: &mut mo
                    }
                )),
                Err(Error::InvalidState)
            );
        }
        assert_no_heap(|| r.shutdown_graph_changes(&mut control)).unwrap();
        assert_no_heap(|| r.shutdown_graph_changes(&mut control)).unwrap();
        let receipt = control.try_receipt().unwrap();
        assert_eq!(receipt.ticket, ticket);
        assert_eq!(receipt.result, Err(Error::Disconnected));
        assert_eq!(
            receipt.effective,
            if admitted { Some(Frame(1024)) } else { None }
        );
        assert_eq!(receipt.actual, Frame(0));
        assert_eq!(receipt.graph.description(), &GraphDescription::default());
        let graph = control
            .compiler()
            .compile(&GraphDescription::default())
            .unwrap();
        assert_eq!(
            control.submit(Frame(0), Epoch(0), graph).unwrap_err().error,
            Error::Disconnected
        );
    }
}
