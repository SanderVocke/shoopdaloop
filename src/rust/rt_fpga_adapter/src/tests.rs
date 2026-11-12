use crate::{adapter::*, image::*};
use rt_runtime::{
    Budgets, Config, Epoch, Error, Frame, NodeId, PreparedConfig,
    graph::{
        self, Catalog, Connection, Endpoint, GraphDescription, PlanDescriptor, PortType,
        PreparedPlan,
    },
    schedule::Action,
    transport::{Lane, Outcome},
};
use shoop_wasm_test_support::shoop_test;
#[derive(Default)]
struct Recording {
    loads: Vec<Image>,
    writes: Vec<(u16, Value)>,
    fail_load: bool,
    fail_write: bool,
    fail_process: bool,
}
impl Device for Recording {
    fn load(&mut self, image: &Image) -> Result<(), Error> {
        if self.fail_load {
            return Err(Error::Disconnected);
        }
        self.loads.push(image.clone());
        Ok(())
    }
    fn write_state(&mut self, address: u16, value: Value) -> Result<(), Error> {
        if self.fail_write {
            return Err(Error::Disconnected);
        }
        self.writes.push((address, value));
        Ok(())
    }
    fn process(&mut self, _: &Image, _: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
        if self.fail_process {
            return Err(Error::Disconnected);
        }
        Ok(vec![vec![0; 32]])
    }
}
fn config() -> PreparedConfig {
    Config {
        max_frames: 32,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 4,
        max_ports: 16,
        max_edges: 16,
        command_slots: 16,
        scheduled_events: 2,
        budgets: Budgets {
            commands: 16,
            publications: 16,
            validation_steps: 4096,
            ..Default::default()
        },
        ..Default::default()
    }
    .prepare()
    .unwrap()
}
fn id(generation: u32) -> NodeId {
    NodeId::from_parts(0, generation).unwrap()
}
fn node(generation: u32, channels: usize) -> Node {
    Node::new(
        id(generation),
        Processor::new(channels, 32, rt_runtime::Q824(16_777_217)),
    )
}
fn submit(c: &mut Controller, r: Request) -> rt_runtime::transport::Ticket {
    c.submit(Lane::Normal, r).map_err(|e| e.error).unwrap()
}
fn graph(config: PreparedConfig) -> PreparedPlan {
    graph::compile(
        &Catalog::new(config, &[node(2, 1).spec()]).unwrap(),
        &GraphDescription {
            nodes: vec![id(2)],
            connections: vec![
                Connection {
                    source: Endpoint::host(PortType::Audio, 0),
                    destination: Endpoint::node(id(2), PortType::Audio, 0),
                },
                Connection {
                    source: Endpoint::node(id(2), PortType::Audio, 0),
                    destination: Endpoint::host(PortType::Audio, 0),
                },
            ],
        },
    )
    .unwrap()
}
fn install(c: &mut Controller) {
    submit(c, Request::Remove { node: id(1) });
    submit(
        c,
        Request::Install {
            prepared: Some(node(2, 1)),
        },
    );
    submit(c, Request::commit(Epoch(2), graph(config())));
}
fn gain(node: NodeId, at: u64, duration: u32) -> Request {
    Request::Schedule {
        at: Frame(at),
        action: Action::Processor {
            node,
            operation: Operation {
                target: rt_runtime::Q824(-1_234_567),
                duration,
            },
        },
        payload: Box::new([]),
    }
}
fn outcomes(c: &mut Controller) -> Vec<Outcome> {
    std::iter::from_fn(|| c.try_completion())
        .map(|v| v.outcome)
        .collect()
}
fn inputs() -> Vec<Vec<i32>> {
    vec![vec![1; 32]]
}

#[shoop_test]
fn owning_install_and_scheduling_produce_only_configuration_traffic() {
    let (mut c, mut a) =
        Adapter::prepare(config(), vec![node(1, 1)], Recording::default()).unwrap();
    install(&mut c);
    submit(&mut c, gain(id(2), 0, 0));
    submit(&mut c, gain(id(2), 64, 0));
    a.process(&inputs()).unwrap();
    assert_eq!(a.epoch(), Epoch(3));
    assert_eq!(a.frame(), Frame(32));
    let mut completions = std::iter::from_fn(|| c.try_completion()).collect::<Vec<_>>();
    assert_eq!(completions.len(), 5);
    assert!(completions[0].retired.is_some());
    let Request::Install { prepared } = &mut completions[1].request else {
        panic!("install")
    };
    assert!(prepared.is_none());
    assert_eq!(
        completions[4].outcome,
        Outcome::Scheduled {
            effective_frame: Frame(64),
            late_by: 0
        }
    );
    let first = c.try_execution().unwrap();
    assert_eq!(first.delivered_frame, Some(Frame(0)));
    assert_eq!(first.processor_result, Some(()));
    a.process(&inputs()).unwrap();
    assert!(c.try_execution().is_none());
    a.process(&inputs()).unwrap();
    assert_eq!(c.try_execution().unwrap().actual_frame, Frame(64));
    assert_eq!(a.metrics.compilations, 1);
    assert_eq!(a.device().loads.len(), 1);
    assert_eq!(a.device().writes.len(), 2);
    assert_eq!(
        a.device().writes[0].1,
        Value::Ramp(Ramp {
            start: -1_234_567,
            target: -1_234_567,
            duration: 0,
            elapsed: 0
        })
    );
}
#[shoop_test]
fn unsupported_stale_live_and_untrusted_requests_do_not_modify_device() {
    let (mut c, mut a) =
        Adapter::prepare(config(), vec![node(1, 1)], Recording::default()).unwrap();
    submit(&mut c, Request::Remove { node: id(1) });
    submit(
        &mut c,
        Request::Install {
            prepared: Some(node(2, 2)),
        },
    );
    submit(
        &mut c,
        Request::Install {
            prepared: Some(node(1, 1)),
        },
    );
    submit(
        &mut c,
        Request::Install {
            prepared: Some(node(2, 1)),
        },
    );
    submit(&mut c, Request::commit(Epoch(0), graph(config())));
    let bad = PreparedPlan::from_descriptor(
        config(),
        PlanDescriptor {
            steps: vec![graph::NodeStep {
                node: id(1),
                input_end: 99,
            }],
            connections: vec![],
        },
    )
    .unwrap();
    submit(&mut c, Request::commit(Epoch(2), bad));
    a.service().unwrap();
    assert_eq!(
        outcomes(&mut c),
        vec![
            Outcome::Applied,
            Outcome::Rejected(Error::InvalidParameter),
            Outcome::Rejected(Error::InvalidId),
            Outcome::Applied,
            Outcome::Rejected(Error::StaleEpoch),
            Outcome::Rejected(Error::InvalidId)
        ]
    );
    assert!(a.device().loads.is_empty());
    submit(&mut c, Request::commit(Epoch(2), graph(config())));
    a.service().unwrap();
    outcomes(&mut c);
    for request in [
        gain(id(1), 0, 0),
        gain(id(2), 1, 0),
        gain(id(2), 0, 7),
        Request::ResetMidi,
        Request::commit(Epoch(3), graph(config())),
    ] {
        submit(&mut c, request);
    }
    a.service().unwrap();
    assert_eq!(
        outcomes(&mut c),
        vec![
            Outcome::Rejected(Error::InvalidId),
            Outcome::Rejected(Error::InvalidFrame),
            Outcome::Rejected(Error::InvalidParameter),
            Outcome::Rejected(Error::InvalidParameter),
            Outcome::Rejected(Error::Busy)
        ]
    );
    assert_eq!(a.device().loads.len(), 1);
    assert!(a.device().writes.is_empty());
    assert_eq!(a.epoch(), Epoch(3));
}
#[shoop_test]
fn unread_receipts_and_transport_completions_retain_admission_credits() {
    let (mut c, mut a) =
        Adapter::prepare(config(), vec![node(1, 1)], Recording::default()).unwrap();
    install(&mut c);
    a.service().unwrap();
    outcomes(&mut c);
    let cancelled = submit(&mut c, gain(id(2), 0, 0));
    cancelled.cancel();
    submit(&mut c, gain(id(2), 0, 0));
    submit(&mut c, gain(id(2), 0, 0));
    a.process(&inputs()).unwrap();
    assert_eq!(outcomes(&mut c)[0], Outcome::Cancelled);
    submit(&mut c, gain(id(2), 32, 0));
    a.service().unwrap();
    assert_eq!(outcomes(&mut c), vec![Outcome::Rejected(Error::Full)]);
    assert!(c.try_execution().is_some());
    assert!(c.try_execution().is_some());
    assert!(c.try_execution().is_none());
    submit(&mut c, gain(id(2), 32, 0));
    a.process(&inputs()).unwrap();
    assert_eq!(outcomes(&mut c).len(), 1);
    assert!(c.try_execution().is_some());
    for _ in 0..16 {
        submit(&mut c, Request::ResetMidi);
    }
    let refused = c.submit(Lane::Normal, Request::ResetMidi).err().unwrap();
    assert_eq!(refused.error, Error::Full);
    a.service().unwrap();
    assert_eq!(c.outstanding(), 16);
    assert_eq!(
        c.submit(Lane::Normal, Request::ResetMidi)
            .err()
            .unwrap()
            .error,
        Error::Full
    );
    assert_eq!(outcomes(&mut c).len(), 16);
    assert_eq!(c.outstanding(), 0);
}
#[shoop_test]
fn cancellation_and_device_errors_have_terminal_results() {
    let (mut c, mut a) =
        Adapter::prepare(config(), vec![node(1, 1)], Recording::default()).unwrap();
    install(&mut c);
    a.service().unwrap();
    outcomes(&mut c);
    let future = submit(&mut c, gain(id(2), 64, 0));
    submit(
        &mut c,
        Request::Cancel {
            sequence: future.sequence(),
        },
    );
    a.service().unwrap();
    assert_eq!(outcomes(&mut c).len(), 2);
    let receipt = c.try_execution().unwrap();
    assert_eq!(receipt.result, Err(Error::Cancelled));
    assert_eq!(receipt.delivered_frame, None);
    submit(&mut c, gain(id(2), 0, 0));
    submit(&mut c, gain(id(2), 96, 0));
    a.device_mut().fail_write = true;
    assert_eq!(a.process(&inputs()), Err(Error::Disconnected));
    assert_eq!(c.try_execution().unwrap().result, Err(Error::Disconnected));
    assert_eq!(a.fault(), Some(Error::Disconnected));
    let cancelled = c.try_execution().unwrap();
    assert_eq!(cancelled.result, Err(Error::Cancelled));
    assert_eq!(cancelled.delivered_frame, None);
    submit(&mut c, Request::ResetMidi);
    a.service().unwrap();
    assert_eq!(
        outcomes(&mut c).last(),
        Some(&Outcome::Rejected(Error::Disconnected))
    );
    let (mut c, mut a) = Adapter::prepare(
        config(),
        vec![node(1, 1)],
        Recording {
            fail_load: true,
            ..Default::default()
        },
    )
    .unwrap();
    install(&mut c);
    a.service().unwrap();
    assert_eq!(
        outcomes(&mut c).last(),
        Some(&Outcome::Rejected(Error::Disconnected))
    );
    assert_eq!(a.epoch(), Epoch(2));
    assert!(a.image().is_none());
}

#[shoop_test]
fn empty_graph_lowers_silence_and_lowerer_rejects_bad_ranges() {
    let (mut c, mut a) = Adapter::prepare(config(), vec![], Recording::default()).unwrap();
    let plan = graph::compile(
        &Catalog::new(config(), &[]).unwrap(),
        &GraphDescription::default(),
    )
    .unwrap();
    submit(&mut c, Request::commit(Epoch(0), plan));
    a.service().unwrap();
    assert_eq!(outcomes(&mut c), vec![Outcome::Applied]);
    let image = a.image().unwrap();
    assert_eq!(
        image.program.instructions,
        vec![
            Instruction::ProcessInput {
                port: 0,
                destination: 0
            },
            Instruction::ProcessSilence { destination: 1 },
            Instruction::ProcessOutput { source: 1, port: 0 }
        ]
    );
    let bad = PlanDescriptor {
        steps: vec![graph::NodeStep {
            node: id(1),
            input_end: 99,
        }],
        connections: vec![],
    };
    let nodes = [crate::compiler::NodeDescriptor {
        id: Identity {
            slot: 0,
            generation: 1,
        },
        channels: 1,
        operation: NodeKind::Gain {
            channels: 1,
            initial: 1,
        },
    }];
    assert!(crate::compiler::lower(&bad, &nodes, image.clone()).is_err());
}

#[shoop_test]
fn commit_completion_waits_for_stalled_device_activation() {
    use std::sync::mpsc;
    struct Stalled {
        arrived: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl Device for Stalled {
        fn load(&mut self, _: &Image) -> Result<(), Error> {
            self.arrived.send(()).map_err(|_| Error::Disconnected)?;
            self.release
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|_| Error::Disconnected)
        }
        fn write_state(&mut self, _: u16, _: Value) -> Result<(), Error> {
            Ok(())
        }
        fn process(&mut self, _: &Image, _: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
            Ok(vec![vec![0; 32]])
        }
    }
    let (arrived, wait) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let (mut c, mut a) = Adapter::prepare(
        config(),
        vec![node(1, 1)],
        Stalled {
            arrived,
            release: blocked,
        },
    )
    .unwrap();
    install(&mut c);
    let worker = std::thread::spawn(move || {
        a.service().unwrap();
        a
    });
    wait.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(outcomes(&mut c), vec![Outcome::Applied, Outcome::Applied]);
    assert_eq!(c.outstanding(), 1);
    release.send(()).unwrap();
    let a = worker.join().unwrap();
    assert_eq!(outcomes(&mut c), vec![Outcome::Applied]);
    assert_eq!(a.epoch(), Epoch(3));
}
