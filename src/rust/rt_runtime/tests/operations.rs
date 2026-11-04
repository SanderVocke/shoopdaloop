mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    processing::{
        self, Block, CommandQueue, OperationContext, Processor, Quantum, Runtime, State, Tick,
    },
    processor::ProcessorOperation,
    resource::ResourceStore,
    Epoch, Error, Frame, NodeId,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[derive(Clone, Copy, Default)]
struct Set {
    gain: f32,
    epoch: Option<Epoch>,
}
impl ProcessorOperation for Set {
    type Result = f32;
}
struct Gain(f32);
impl<S: State> Processor<S, Set> for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..Default::default()
        }
    }
    fn operation(
        &mut self,
        op: Set,
        context: OperationContext<'_>,
        _: &mut S,
    ) -> Result<f32, Error> {
        if !op.gain.is_finite() {
            return Err(Error::InvalidParameter);
        }
        if op.epoch.is_some_and(|e| e != context.graph_epoch) {
            return Err(Error::StaleEpoch);
        }
        let old = self.0;
        self.0 = op.gain;
        Ok(old)
    }
    fn process(&mut self, b: &mut Block<'_>, _: &mut S) -> Result<(), Error> {
        let (i, o) = b.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (i, o) in i.iter().zip(o) {
            *o = i * self.0;
        }
        Ok(())
    }
}
fn id() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
fn config(q: usize) -> processing::PreparedConfig {
    processing::Config {
        quantum: Quantum::new(q).unwrap(),
        max_host_frames: 256,
        capacities: rt_runtime::Config {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 0,
            midi_outputs: 0,
            command_slots: 8,
            budgets: rt_runtime::Budgets {
                commands: 2,
                ..Default::default()
            },
            ..Default::default()
        },
    }
    .prepare()
    .unwrap()
}
fn graph(active: bool) -> GraphDescription {
    let endpoints = if active {
        vec![
            Endpoint::host(PortType::Audio, 0),
            Endpoint::node(id(), PortType::Audio, 0),
            Endpoint::host(PortType::Audio, 0),
        ]
    } else {
        vec![
            Endpoint::host(PortType::Audio, 0),
            Endpoint::host(PortType::Audio, 0),
        ]
    };
    GraphDescription {
        nodes: if active { vec![id()] } else { vec![] },
        connections: endpoints
            .windows(2)
            .map(|w| Connection {
                source: w[0],
                destination: w[1],
            })
            .collect(),
    }
}

#[shoop_test]
fn typed_operations_reach_inactive_nodes_before_publication_and_share_callback_ingress() {
    struct Domain {
        queue: CommandQueue<u32, u32>,
        sum: u32,
    }
    impl State for Domain {
        fn validate_activation(&self, _: &GraphDescription) -> Result<(), Error> {
            Ok(())
        }
        fn begin_callback_with_budget(
            &mut self,
            frame: Frame,
            _: &mut ResourceStore,
            budget: usize,
        ) -> Result<(), Error> {
            self.queue
                .begin_callback_with_budget(frame, budget)
                .map(|_| ())
        }
        fn begin_tick(&mut self, tick: Tick, _: &mut ResourceStore) -> Result<(), Error> {
            let sum = &mut self.sum;
            self.queue.dispatch(tick.frame, |v| {
                *sum += v;
                Ok(*sum)
            })
        }
    }
    for q in [16, 32, 64, 128] {
        let c = config(q);
        let (mut domain, queue) = processing::command_channel(c.quantum(), 8).unwrap();
        for value in [1, 2, 3, 4] {
            domain.submit(Frame(0), value).unwrap();
        }
        let mut r = Runtime::prepare(
            c,
            vec![(id(), Gain(1.))],
            Domain { queue, sum: 0 },
            ResourceStore::new(c.resources()).unwrap(),
            &graph(false),
        )
        .unwrap();
        let mut ops = r.prepare_operations(8, 1).unwrap();
        let mut routes = r.prepare_graph_changes().unwrap();
        routes
            .submit(
                Frame(q as u64),
                Epoch(0),
                routes.compiler().compile(&graph(true)).unwrap(),
            )
            .unwrap();
        let first = ops
            .submit(
                Frame(1),
                id(),
                Set {
                    gain: 2.,
                    epoch: Some(Epoch(0)),
                },
            )
            .unwrap();
        let second = ops
            .submit(
                Frame(0),
                id(),
                Set {
                    gain: 3.,
                    epoch: Some(Epoch(1)),
                },
            )
            .unwrap();
        assert_eq!(
            ops.submit(Frame(0), NodeId::from_parts(1, 1).unwrap(), Set::default()),
            Err(Error::InvalidId)
        );
        let input = vec![1.; 2 * q];
        let mut output = vec![0.; 2 * q];
        for (frames, sum, gain) in [(2 * q, 1, 2.), (q, 3, 3.), (q, 10, 3.)] {
            assert_no_heap(|| {
                r.process(
                    AudioBlock {
                        frame: r.next_frame(),
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
            assert_eq!(r.state().sum, sum);
            if frames == 2 * q {
                assert_eq!(&output[..q], vec![1.; q]);
                assert_eq!(&output[q..], vec![gain; q]);
            } else {
                assert_eq!(&output[..q], vec![gain; q]);
            }
        }
        let a = ops.try_receipt().unwrap();
        let b = ops.try_receipt().unwrap();
        assert_eq!(a.ticket, first);
        assert_eq!(a.actual, Frame(q as u64));
        assert_eq!(a.result, Ok(1.));
        assert_eq!(b.ticket, second);
        assert_eq!(b.effective, Some(Frame(2 * q as u64)));
        assert_eq!(b.result, Ok(2.));
        while domain.try_receipt().is_some() {}
        assert_eq!(domain.outstanding(), 0);
        ops.submit(
            r.next_frame(),
            id(),
            Set {
                gain: f32::NAN,
                epoch: None,
            },
        )
        .unwrap();
        assert_no_heap(|| {
            r.process(
                AudioBlock {
                    frame: r.next_frame(),
                    frames: q,
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
        assert_eq!(
            ops.try_receipt().unwrap().result,
            Err(Error::InvalidParameter)
        );
        assert_eq!(&output[..q], vec![3.; q]);
        assert_eq!(r.diagnostics().fault, None);
        let pending = ops.submit(Frame(4096), id(), Set::default()).unwrap();
        assert_no_heap(|| r.shutdown_operations(&mut ops)).unwrap();
        assert_no_heap(|| r.shutdown_operations(&mut ops)).unwrap();
        let r = ops.try_receipt().unwrap();
        assert_eq!(r.ticket, pending);
        assert_eq!(r.result, Err(Error::Disconnected));
        assert_eq!(ops.outstanding(), 0);
    }
}

#[shoop_test]
fn buffered_host_keeps_typed_operation_times_on_the_core_clock() {
    for q in [16, 32, 64, 128] {
        let c = config(q);
        let mut r = Runtime::prepare(
            c,
            vec![(id(), Gain(1.))],
            (),
            ResourceStore::new(c.resources()).unwrap(),
            &graph(true),
        )
        .unwrap();
        let mut ops = r.prepare_operations(8, 1).unwrap();
        ops.submit(
            Frame(1),
            id(),
            Set {
                gain: 2.,
                epoch: None,
            },
        )
        .unwrap();
        let mut host = processing::BufferedHost::prepare(r).unwrap();
        let input = [1.; 256];
        let mut output = [0.; 256];
        let mut host_frame = 0;
        for frames in [1, q - 1, q + 3, q - 3] {
            assert_no_heap(|| {
                host.process(
                    AudioBlock {
                        frame: Frame(host_frame as u64),
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
            for (offset, &sample) in output[..frames].iter().enumerate() {
                let f = host_frame + offset;
                assert_eq!(
                    sample,
                    if f < q {
                        0.
                    } else if f < 2 * q {
                        1.
                    } else {
                        2.
                    }
                );
            }
            host_frame += frames;
        }
        let receipt = ops.try_receipt().unwrap();
        assert_eq!(receipt.requested, Frame(1));
        assert_eq!(receipt.effective, Some(Frame(q as u64)));
        assert_eq!(receipt.actual, Frame(q as u64));
        assert_eq!(host.next_frame(), Frame(3 * q as u64));
        assert_eq!(host.runtime().next_frame(), Frame(3 * q as u64));
    }
}
