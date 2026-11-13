#![cfg(all(feature = "app", not(target_arch = "wasm32")))]
use rt_fpga_adapter::{
    adapter::Device,
    engine::{Adapter, EngineDevice},
    image::{Image, Value},
    looping::{LoopDevice, MediaCommand, MediaResult, TpReply},
    protocol::put,
};
use rt_runtime::{
    Epoch, Error, Frame, PlannerId,
    graph::{PlanDescriptor, PreparedPlan},
    schedule::Action,
    transport::{Lane, Outcome},
};
use shoop_loops::runtime::{Failure, Operation, Reply};
use shoop_rt_engine::{EngineCommand, EngineControl, external::Consumer};
use shoop_wasm_test_support::shoop_test;
use std::sync::{Arc, Mutex, mpsc};
struct Recording {
    frame: u64,
    calls: Arc<Mutex<Vec<&'static str>>>,
    gate: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    fail_program: bool,
}
impl Default for Recording {
    fn default() -> Self {
        Self {
            frame: 0,
            calls: Arc::default(),
            gate: None,
            fail_program: false,
        }
    }
}
impl Device for Recording {
    fn load(&mut self, _: &Image) -> Result<(), Error> {
        self.calls.lock().unwrap().push("load");
        self.frame = 0;
        Ok(())
    }
    fn write_state(&mut self, _: u16, _: Value) -> Result<(), Error> {
        self.calls.lock().unwrap().push("state");
        Ok(())
    }
    fn process(&mut self, _: &Image, _: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
        self.calls.lock().unwrap().push("process");
        self.frame += 32;
        Ok(vec![vec![0; 32]; 2])
    }
}
impl LoopDevice for Recording {
    fn tp(&mut self, opcode: u8, _: u32, _: [u32; 20]) -> Result<TpReply, Error> {
        let mut words = [0; 26];
        match opcode {
            0 => put(&mut words, 0, 64, self.frame),
            16 => {}
            _ => panic!("unexpected TP command {opcode}"),
        };
        Ok(TpReply { error: 0, words })
    }
    fn media(&mut self, _: MediaCommand) -> Result<MediaResult, Error> {
        panic!("protocol double does not execute media")
    }
    fn chunk_limit(&mut self, _: u32) -> Result<(), Error> {
        Ok(())
    }
}
impl EngineDevice for Recording {
    fn program(&mut self, _: &Image) -> Result<(), Error> {
        self.calls.lock().unwrap().push("program");
        if let Some((ready, release)) = self.gate.take() {
            ready.send(()).unwrap();
            release
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|_| Error::Disconnected)?;
        }
        if self.fail_program {
            Err(Error::InvalidState)
        } else {
            Ok(())
        }
    }
    fn read_state(&mut self, _: u16) -> Result<Value, Error> {
        panic!("no parameter bindings")
    }
    fn scratch(&mut self, _: u16) -> Result<Vec<i32>, Error> {
        panic!("empty graph has no meter")
    }
}
fn prepare(
    device: Recording,
) -> (
    EngineControl,
    Adapter<Recording>,
    rt_runtime::PreparedConfig,
) {
    let mut parts = None;
    let _: Result<shoop_rt_engine::external::Prepared<()>, _> =
        shoop_rt_engine::prepare_with_consumer(
            &rt_fpga_sim::app::description(),
            shoop_rt_protocol::DriverStatus {
                kind: shoop_rt_protocol::DriverKind::Dummy,
                state: shoop_rt_protocol::DriverRunState::Running,
                sample_rate: 48000,
                buffer_frames: 512,
            },
            |p| {
                parts = Some(p);
                Err(Error::InvalidConfig.into())
            },
        );
    let parts = parts.unwrap();
    let config = parts.config;
    let Consumer {
        control, execution, ..
    } = Adapter::prepare(parts, device).unwrap();
    (control, execution, config)
}
fn tick(adapter: &mut Adapter<Recording>) -> Result<(), Error> {
    adapter.process(&[&[0.; 32], &[0.; 32]], &mut [&mut [0.; 32], &mut [0.; 32]])
}
fn submit(control: &mut EngineControl, operation: Operation, at: u64) {
    control
        .submit(
            Lane::Normal,
            EngineCommand::Schedule {
                at: Frame(at),
                action: Action::Planner {
                    planner: PlannerId::from_parts(0, 1).unwrap(),
                    operation,
                },
                payload: Box::new([]),
            },
        )
        .map_err(|e| e.error)
        .unwrap();
}
fn stage(
    control: &mut EngineControl,
    adapter: &mut Adapter<Recording>,
    config: rt_runtime::PreparedConfig,
) -> rt_runtime::graph::activation::StageId {
    control
        .submit(
            Lane::Normal,
            EngineCommand::StageGraph {
                slot: 0,
                expected_epoch: Epoch(0),
                plan: Some(
                    PreparedPlan::from_descriptor(config, PlanDescriptor::default()).unwrap(),
                ),
                id: None,
            },
        )
        .map_err(|e| e.error)
        .unwrap();
    tick(adapter).unwrap();
    let completed = control.try_completion().unwrap();
    assert_eq!(completed.outcome, Outcome::Applied);
    let EngineCommand::StageGraph {
        id: Some(id),
        plan: None,
        ..
    } = completed.request
    else {
        panic!("missing owning graph stage")
    };
    id
}
#[shoop_test]
fn graph_activation_ack_waits_for_the_device_at_the_scheduled_boundary() {
    let (ready, arrived) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut device = Recording::default();
    device.gate = Some((ready, gate));
    let calls = device.calls.clone();
    let (mut control, mut adapter, config) = prepare(device);
    let id = stage(&mut control, &mut adapter, config);
    submit(
        &mut control,
        Operation::GraphActivate {
            id,
            graph_epoch: Epoch(0),
            topology_epoch: Epoch(0),
        },
        64,
    );
    tick(&mut adapter).unwrap();
    assert!(control.try_execution().is_none());
    assert_eq!(
        control.try_completion().unwrap().outcome,
        Outcome::Scheduled {
            effective_frame: Frame(64),
            late_by: 0
        }
    );
    let thread = std::thread::spawn(move || {
        tick(&mut adapter).unwrap();
        adapter
    });
    arrived
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(control.try_execution().is_none());
    release.send(()).unwrap();
    let mut adapter = thread.join().unwrap();
    let result = control.try_execution().unwrap();
    assert_eq!(result.actual_frame, Frame(64));
    assert!(matches!(
        result.operation_result,
        Some(Reply::GraphActivated {
            graph_epoch: Epoch(1),
            ..
        })
    ));
    assert_eq!(adapter.metrics().activations, 1);
    control
        .submit(Lane::Normal, EngineCommand::ReleaseGraph { id })
        .map_err(|e| e.error)
        .unwrap();
    tick(&mut adapter).unwrap();
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    let plan = PreparedPlan::from_descriptor(config, PlanDescriptor::default()).unwrap();
    control
        .submit(
            Lane::Normal,
            EngineCommand::StageGraph {
                slot: 0,
                expected_epoch: Epoch(0),
                plan: Some(plan),
                id: None,
            },
        )
        .map_err(|e| e.error)
        .unwrap();
    tick(&mut adapter).unwrap();
    let returned = control.try_completion().unwrap();
    assert_eq!(returned.outcome, Outcome::Rejected(Error::StaleEpoch));
    assert!(matches!(
        returned.request,
        EngineCommand::StageGraph {
            plan: Some(_),
            id: None,
            ..
        }
    ));
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|&&c| c == "program")
            .count(),
        1
    );
}
#[shoop_test]
fn partial_program_failure_never_activates_and_terminates_future_work() {
    let mut device = Recording::default();
    device.fail_program = true;
    let (mut control, mut adapter, config) = prepare(device);
    let id = stage(&mut control, &mut adapter, config);
    submit(
        &mut control,
        Operation::GraphActivate {
            id,
            graph_epoch: Epoch(0),
            topology_epoch: Epoch(0),
        },
        64,
    );
    submit(&mut control, Operation::GraphStatus { id }, 4096);
    tick(&mut adapter).unwrap();
    assert_eq!(tick(&mut adapter), Err(Error::Disconnected));
    assert_eq!(adapter.metrics().activations, 0);
    assert_eq!(adapter.fault(), Some(Error::Disconnected));
    let results = std::iter::from_fn(|| control.try_execution()).collect::<Vec<_>>();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.actual_frame == Frame(64)
        && r.operation_result == Some(Reply::Failed(Failure::Runtime(Error::Disconnected)))));
}
#[shoop_test]
fn invalid_candidate_and_incomplete_metadata_keep_owners_and_active_hardware_unchanged() {
    let device = Recording::default();
    let calls = device.calls.clone();
    let (mut control, mut adapter, config) = prepare(device);
    let id = stage(&mut control, &mut adapter, config);
    let parts = id.parts();
    let foreign =
        rt_runtime::graph::activation::StageId::from_parts(parts[0] + 1, parts[1], parts[2])
            .unwrap();
    submit(
        &mut control,
        Operation::GraphActivate {
            id: foreign,
            graph_epoch: Epoch(0),
            topology_epoch: Epoch(0),
        },
        32,
    );
    submit(&mut control, Operation::TopologyBegin { slot: 0 }, 32);
    tick(&mut adapter).unwrap();
    let results = std::iter::from_fn(|| control.try_execution()).collect::<Vec<_>>();
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].operation_result,
        Some(Reply::Failed(Failure::Runtime(Error::InvalidId)))
    );
    assert_eq!(
        results[1].operation_result,
        Some(Reply::Failed(Failure::Runtime(Error::InvalidShape)))
    );
    assert_eq!(adapter.metrics().activations, 0);
    assert!(adapter.fault().is_none());
    assert!(!calls.lock().unwrap().contains(&"program"));
    submit(
        &mut control,
        Operation::GraphActivate {
            id,
            graph_epoch: Epoch(0),
            topology_epoch: Epoch(0),
        },
        64,
    );
    tick(&mut adapter).unwrap();
    assert!(matches!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::GraphActivated { .. })
    ));
    let _ = config;
}

#[shoop_test]
fn candidate_cancel_reuses_the_slot_but_rejects_the_previous_generation() {
    use shoop_loops::runtime::NodePairStatus;
    let (mut control, mut adapter, _) = prepare(Recording::default());
    submit(
        &mut control,
        Operation::NodePairBegin {
            slot: 0,
            changes: 1,
        },
        0,
    );
    tick(&mut adapter).unwrap();
    let Some(Reply::NodePairStatus {
        id,
        status: NodePairStatus::Receiving { .. },
    }) = control.try_execution().unwrap().operation_result
    else {
        panic!("receiving candidate")
    };
    submit(&mut control, Operation::NodePairRecycle { id }, 32);
    submit(&mut control, Operation::NodePairCancel { id }, 32);
    tick(&mut adapter).unwrap();
    assert_eq!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::Failed(Failure::Runtime(Error::InvalidState)))
    );
    assert_eq!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::NodePairStatus {
            id,
            status: NodePairStatus::Idle
        })
    );
    submit(
        &mut control,
        Operation::NodePairBegin {
            slot: 0,
            changes: 1,
        },
        64,
    );
    tick(&mut adapter).unwrap();
    let Some(Reply::NodePairStatus { id: new, .. }) =
        control.try_execution().unwrap().operation_result
    else {
        panic!("new candidate")
    };
    assert_ne!(new, id);
    submit(&mut control, Operation::NodePairCancel { id }, 96);
    tick(&mut adapter).unwrap();
    assert_eq!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::Failed(Failure::Runtime(Error::InvalidId)))
    );
    assert_eq!(adapter.metrics().activations, 0);
}
#[shoop_test]
fn normal_queue_pressure_preserves_recovery_and_ticket_cancellation() {
    let (mut control, mut adapter, config) = prepare(Recording::default());
    let id = rt_runtime::graph::activation::StageId::from_parts(u32::MAX, 0, 1).unwrap();
    let make = |at| EngineCommand::Schedule {
        at: Frame(at),
        action: Action::Planner {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            operation: Operation::GraphStatus { id },
        },
        payload: Box::new([]),
    };
    let mut tickets = vec![];
    loop {
        match control.submit(Lane::Normal, make(4096)) {
            Ok(ticket) => tickets.push(ticket),
            Err(error) => {
                assert_eq!(error.error, Error::Full);
                assert!(matches!(error.value, EngineCommand::Schedule { .. }));
                break;
            }
        }
    }
    assert!(tickets.len() > 1);
    tickets[0].cancel();
    control
        .submit(Lane::Recovery, make(0))
        .map_err(|e| e.error)
        .unwrap();
    let mut cancelled = false;
    let mut scheduled = 0;
    for _ in 0..tickets.len().div_ceil(config.limits().budgets.commands) + 2 {
        tick(&mut adapter).unwrap();
        while let Some(c) = control.try_completion() {
            match c.outcome {
                Outcome::Cancelled => cancelled = true,
                Outcome::Scheduled { .. } => scheduled += 1,
                Outcome::Rejected(Error::Capacity) => {}
                other => panic!("{other:?}"),
            }
        }
    }
    assert!(cancelled);
    assert!(scheduled > 1);
    assert_eq!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::Failed(Failure::Runtime(Error::InvalidId)))
    );
    assert!(control.try_execution().is_none());
    let target = tickets[1].sequence();
    control
        .submit(
            Lane::Recovery,
            EngineCommand::Schedule {
                at: adapter.frame(),
                action: Action::Planner {
                    planner: PlannerId::from_parts(0, 1).unwrap(),
                    operation: Operation::Cancel { sequence: target },
                },
                payload: Box::new([]),
            },
        )
        .map_err(|e| e.error)
        .unwrap();
    tick(&mut adapter).unwrap();
    let results = std::iter::from_fn(|| control.try_execution()).collect::<Vec<_>>();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.operation_result
        == Some(Reply::Cancelled {
            sequence: target,
            delivered: false
        })));
    assert_eq!(adapter.metrics().activations, 0);
}
