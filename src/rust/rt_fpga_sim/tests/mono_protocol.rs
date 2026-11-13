#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_adapter::{
    adapter::Device,
    image::{Image, Value},
    looping::{self, LoopAdapter, LoopDevice, MediaCommand, MediaResult, Request, TpReply},
    protocol::put,
};
use rt_fpga_sim::mono;
use rt_runtime::{
    Epoch, Error, Frame,
    graph::{self, Catalog},
    schedule::Action,
    transport::{Lane, Outcome},
};
use shoop_loops::runtime::{Failure, Operation, Reply};
use shoop_wasm_test_support::shoop_test;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
#[derive(Clone, Debug)]
enum Call {
    Load(Image),
    Limit(u32),
    Write(u16, Value),
    Tp(u8, u32, [u32; 20]),
    Process,
}
struct Canned {
    calls: Arc<Mutex<Vec<Call>>>,
    replies: VecDeque<(u8, Result<TpReply, Error>)>,
    gate: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    fail_load: bool,
}
impl Canned {
    fn new() -> Self {
        Self {
            calls: Arc::default(),
            replies: VecDeque::new(),
            gate: None,
            fail_load: false,
        }
    }
    fn reply(&mut self, opcode: u8, words: [u32; 26]) {
        self.replies
            .push_back((opcode, Ok(TpReply { error: 0, words })));
    }
    fn stopped_quantum(&mut self, end: u64) {
        self.reply(16, [0; 26]);
        self.reply(12, stopped());
        self.reply(12, stopped());
        let mut status = [0; 26];
        put(&mut status, 0, 64, end);
        self.reply(0, status);
    }
}
fn stopped() -> [u32; 26] {
    let mut words = [0; 26];
    put(&mut words, 0, 1, 1);
    put(&mut words, 1, 32, 1);
    words
}
impl Device for Canned {
    fn load(&mut self, image: &Image) -> Result<(), Error> {
        self.calls.lock().unwrap().push(Call::Load(image.clone()));
        if self.fail_load {
            Err(Error::Disconnected)
        } else {
            Ok(())
        }
    }
    fn write_state(&mut self, a: u16, v: Value) -> Result<(), Error> {
        self.calls.lock().unwrap().push(Call::Write(a, v));
        Ok(())
    }
    fn process(&mut self, _: &Image, _: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
        self.calls.lock().unwrap().push(Call::Process);
        Ok(vec![vec![0; 32]])
    }
}
impl LoopDevice for Canned {
    fn tp(&mut self, opcode: u8, index: u32, words: [u32; 20]) -> Result<TpReply, Error> {
        self.calls
            .lock()
            .unwrap()
            .push(Call::Tp(opcode, index, words));
        if opcode == 16 {
            if let Some((arrived, release)) = self.gate.take() {
                arrived.send(()).unwrap();
                release
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| Error::Disconnected)?;
            }
        }
        let (expected, reply) = self.replies.pop_front().expect("unexpected device request");
        assert_eq!(opcode, expected);
        reply
    }
    fn media(&mut self, _: MediaCommand) -> Result<MediaResult, Error> {
        panic!("protocol double does not execute media")
    }
    fn chunk_limit(&mut self, limit: u32) -> Result<(), Error> {
        self.calls.lock().unwrap().push(Call::Limit(limit));
        Ok(())
    }
}
fn prepare(device: Canned) -> (looping::Controller, LoopAdapter<Canned>) {
    let p = mono::prepare().unwrap();
    let plan = graph::compile(
        &Catalog::new(
            mono::config(),
            &p.nodes.iter().map(|n| n.spec()).collect::<Vec<_>>(),
        )
        .unwrap(),
        &mono::graph(),
    )
    .unwrap();
    let (mut control, adapter) =
        LoopAdapter::prepare(mono::config(), p.startup, p.nodes, device).unwrap();
    control
        .submit(Lane::Normal, Request::commit(Epoch(0), plan))
        .map_err(|e| e.error)
        .unwrap();
    (control, adapter)
}
fn submit(control: &mut looping::Controller, operation: Operation<rt_runtime::Q824>, at: u64) {
    let planner = rt_runtime::PlannerId::from_parts(0, 1).unwrap();
    control
        .submit(
            Lane::Normal,
            Request::Schedule {
                at: Frame(at),
                action: Action::Planner { planner, operation },
                payload: Box::new([]),
            },
        )
        .map_err(|e| e.error)
        .unwrap();
}
#[shoop_test]
fn queued_query_waits_for_real_boundary_ack_and_gain_changes_write_only_bound_state() {
    let (arrived, wait) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let mut device = Canned::new();
    device.gate = Some((arrived, blocked));
    for end in [32, 64, 96] {
        device.stopped_quantum(end);
    }
    let calls = device.calls.clone();
    let (mut control, mut adapter) = prepare(device);
    submit(
        &mut control,
        Operation::State {
            target: mono::loop_id(),
        },
        0,
    );
    control
        .submit(
            Lane::Normal,
            Request::Schedule {
                at: Frame(64),
                action: Action::Processor {
                    node: mono::node_id(2),
                    operation: looping::Operation {
                        target: rt_runtime::Q824(-17),
                        duration: 0,
                    },
                },
                payload: Box::new([]),
            },
        )
        .map_err(|e| e.error)
        .unwrap();
    let worker = std::thread::spawn(move || {
        adapter.process(&[vec![123; 32]]).unwrap();
        adapter
    });
    wait.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(control.try_execution().is_none());
    let completions = std::iter::from_fn(|| control.try_completion()).collect::<Vec<_>>();
    assert_eq!(completions.len(), 3);
    assert_eq!(completions[0].outcome, Outcome::Applied);
    release.send(()).unwrap();
    let mut adapter = worker.join().unwrap();
    assert!(matches!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::State(_))
    ));
    adapter.process(&[vec![0; 32]]).unwrap();
    assert!(control.try_execution().is_none());
    adapter.process(&[vec![0; 32]]).unwrap();
    assert_eq!(control.try_execution().unwrap().actual_frame, Frame(64));
    assert_eq!(adapter.metrics.compilations, 1);
    assert_eq!(adapter.metrics.gain_writes, 1);
    let calls = calls.lock().unwrap();
    let images = calls
        .iter()
        .filter_map(|c| if let Call::Load(i) = c { Some(i) } else { None })
        .collect::<Vec<_>>();
    assert_eq!(images.len(), 1);
    let address = images[0].gains[0].ramps[0];
    assert!(
        calls
            .iter()
            .any(|c| matches!(c,Call::Write(a,Value::Ramp(r)) if *a==address && r.target==-17))
    );
    assert!(calls.iter().any(|c| matches!(c, Call::Limit(128))));
    assert_eq!(
        calls.iter().filter(|c| matches!(c, Call::Process)).count(),
        3
    );
    assert!(
        calls
            .iter()
            .any(|c| matches!(c,Call::Tp(16,0,words) if *words==[0;20]))
    );
}
#[shoop_test]
fn unsupported_and_stale_requests_are_refused_without_reconfiguration() {
    let mut device = Canned::new();
    device.stopped_quantum(32);
    let calls = device.calls.clone();
    let (mut c, mut a) = prepare(device);
    a.service().unwrap();
    c.try_completion().unwrap();
    let planner = rt_runtime::PlannerId::from_parts(0, 2).unwrap();
    c.submit(
        Lane::Normal,
        Request::Schedule {
            at: Frame(0),
            action: Action::Planner {
                planner,
                operation: Operation::State {
                    target: mono::loop_id(),
                },
            },
            payload: Box::new([]),
        },
    )
    .map_err(|e| e.error)
    .unwrap();
    submit(
        &mut c,
        Operation::State {
            target: mono::loop_id(),
        },
        1,
    );
    submit(
        &mut c,
        Operation::Export {
            target: mono::loop_id(),
            selection: shoop_loops::runtime::Selection::Recording,
        },
        0,
    );
    submit(
        &mut c,
        Operation::State {
            target: shoop_loops::LoopId::from_parts(0, 2).unwrap(),
        },
        0,
    );
    a.process(&[vec![0; 32]]).unwrap();
    let outcomes = std::iter::from_fn(|| c.try_completion())
        .map(|c| c.outcome)
        .collect::<Vec<_>>();
    assert_eq!(outcomes[0], Outcome::Rejected(Error::InvalidId));
    assert_eq!(outcomes[1], Outcome::Rejected(Error::InvalidFrame));
    assert_eq!(outcomes[2], Outcome::Rejected(Error::InvalidParameter));
    assert!(matches!(
        c.try_execution().unwrap().operation_result,
        Some(Reply::Failed(Failure::Timeline(
            shoop_loops::Error::InvalidId
        )))
    ));
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| matches!(c, Call::Load(_)))
            .count(),
        1
    );
}
#[shoop_test]
fn device_faults_return_terminal_results_and_reject_later_work() {
    let mut device = Canned::new();
    device.replies.push_back((16, Err(Error::Disconnected)));
    let (mut c, mut a) = prepare(device);
    submit(
        &mut c,
        Operation::State {
            target: mono::loop_id(),
        },
        0,
    );
    submit(
        &mut c,
        Operation::State {
            target: mono::loop_id(),
        },
        96,
    );
    assert_eq!(a.process(&[vec![0; 32]]), Err(Error::Disconnected));
    assert_eq!(a.fault(), Some(Error::Disconnected));
    let results = std::iter::from_fn(|| c.try_execution()).collect::<Vec<_>>();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].result, Err(Error::Disconnected));
    assert!(matches!(
        results[1].operation_result,
        Some(Reply::Failed(Failure::Runtime(Error::Disconnected)))
    ));
    submit(
        &mut c,
        Operation::State {
            target: mono::loop_id(),
        },
        0,
    );
    a.service().unwrap();
    assert_eq!(
        std::iter::from_fn(|| c.try_completion())
            .last()
            .unwrap()
            .outcome,
        Outcome::Rejected(Error::Disconnected)
    );
    let mut device = Canned::new();
    device.fail_load = true;
    let (mut c, mut a) = prepare(device);
    a.service().unwrap();
    assert_eq!(
        c.try_completion().unwrap().outcome,
        Outcome::Rejected(Error::Disconnected)
    );
    assert_eq!(a.epoch(), Epoch(0));
    assert!(a.image().is_none());
}

#[shoop_test]
fn startup_and_live_edit_refusals_preserve_configuration_and_owners() {
    let mut prepared = mono::prepare().unwrap();
    prepared.startup.binding.gain = rt_runtime::Q824(1);
    let device = Canned::new();
    let calls = device.calls.clone();
    assert!(matches!(
        LoopAdapter::prepare(mono::config(), prepared.startup, prepared.nodes, device),
        Err(Error::InvalidParameter)
    ));
    assert!(calls.lock().unwrap().is_empty());

    let mut prepared = mono::prepare().unwrap();
    prepared.startup.max_chunks = 129;
    assert!(matches!(
        LoopAdapter::prepare(
            mono::config(),
            prepared.startup,
            prepared.nodes,
            Canned::new()
        ),
        Err(Error::InvalidConfig)
    ));

    let prepared = mono::prepare().unwrap();
    let catalog = Catalog::new(
        mono::config(),
        &prepared.nodes.iter().map(|n| n.spec()).collect::<Vec<_>>(),
    )
    .unwrap();
    let device = Canned::new();
    let calls = device.calls.clone();
    let (mut control, mut adapter) =
        LoopAdapter::prepare(mono::config(), prepared.startup, prepared.nodes, device).unwrap();
    let incomplete = graph::compile(
        &catalog,
        &graph::GraphDescription {
            nodes: vec![mono::node_id(0)],
            connections: vec![],
        },
    )
    .unwrap();
    control
        .submit(Lane::Normal, Request::commit(Epoch(0), incomplete))
        .map_err(|e| e.error)
        .unwrap();
    adapter.service().unwrap();
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Rejected(Error::InvalidShape));
    assert!(matches!(
        completion.request,
        Request::Commit { plan: Some(_), .. }
    ));
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(adapter.epoch(), Epoch(0));

    let plan = || graph::compile(&catalog, &mono::graph()).unwrap();
    control
        .submit(Lane::Normal, Request::commit(Epoch(1), plan()))
        .map_err(|e| e.error)
        .unwrap();
    control
        .submit(Lane::Normal, Request::commit(Epoch(0), plan()))
        .map_err(|e| e.error)
        .unwrap();
    adapter.service().unwrap();
    assert_eq!(
        control.try_completion().unwrap().outcome,
        Outcome::Rejected(Error::StaleEpoch)
    );
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    control
        .submit(
            Lane::Normal,
            Request::Remove {
                node: mono::node_id(0),
            },
        )
        .map_err(|e| e.error)
        .unwrap();
    control
        .submit(Lane::Normal, Request::commit(Epoch(1), plan()))
        .map_err(|e| e.error)
        .unwrap();
    adapter.service().unwrap();
    assert_eq!(
        control.try_completion().unwrap().outcome,
        Outcome::Rejected(Error::Busy)
    );
    let refused = control.try_completion().unwrap();
    assert_eq!(refused.outcome, Outcome::Rejected(Error::Busy));
    assert!(matches!(
        refused.request,
        Request::Commit { plan: Some(_), .. }
    ));
    assert_eq!(adapter.metrics.compilations, 1);
    let calls = calls.lock().unwrap();
    let images = calls
        .iter()
        .filter_map(|call| match call {
            Call::Load(image) => Some(image),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(images.len(), 1);
    let image = images[0];
    assert_eq!(image.transition.description.loops.len(), 1);
    assert_eq!(
        image.transition.description.loops[0].id.slot,
        mono::loop_id().slot()
    );
    assert_eq!(
        image.transition.description.loops[0].id.generation,
        mono::loop_id().generation()
    );
    assert_eq!(image.media.views.len(), 1);
    assert_eq!(image.loop_views[0].views, [0]);
    assert!(
        image
            .program
            .instructions
            .iter()
            .any(|instruction| matches!(
                instruction,
                rt_fpga_adapter::image::Instruction::ProcessLogicalCapture { view, .. } if image.initial_state.memory[*view as usize] == Value::Word(0)
            ))
    );
    assert!(
        image
            .program
            .instructions
            .iter()
            .any(|instruction| matches!(
                instruction,
                rt_fpga_adapter::image::Instruction::ProcessLogicalPlayback { view, .. } if image.initial_state.memory[*view as usize] == Value::Word(0)
            ))
    );
}
