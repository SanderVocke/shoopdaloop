//! Settle known future work, or abandon it explicitly after callbacks stop.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{compile, Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::{
    Disposition, FinishContext, OperationContext, PlanContext, Planner, PlannerSpec,
};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::{
    Currentness, Description, Format, Request, ResourceStore, Snapshot, UploadId,
};
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Completion, Lane, Outcome, Ticket};
use rt_runtime::{
    Budgets, Config, Error, Frame, NodeId, PlannerId, ResourceId, Revision, Sequence,
};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
fn node() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
#[derive(Clone, Copy)]
enum Op {
    Wait,
    Cancel(Sequence),
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reply {
    Done,
    Cancelled,
}
struct Ledger {
    resource: ResourceId,
    snapshot: Option<Snapshot>,
    upload: Option<UploadId>,
    waits_delivered: usize,
    cancellations: usize,
}
impl Planner for Ledger {
    type Decision = ();
    type Observation = ();
    type Operation = Op;
    type Result = Reply;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 0,
            observation_capacity: 0,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[()] {
        &[]
    }
    fn plan(&mut self, context: PlanContext<'_, Op, Reply>) -> Result<Frame, Error> {
        Ok(context.ceiling)
    }
    fn finish(&mut self, context: FinishContext<'_, (), Op, Reply>) -> Result<(), Error> {
        assert!(context.observations.is_empty());
        Ok(())
    }
    fn recovery_safe(op: Op) -> bool {
        matches!(op, Op::Cancel(_))
    }
    fn operation(
        &mut self,
        op: Op,
        mut context: OperationContext<'_, Op, Reply>,
    ) -> Result<Disposition<Reply>, Error> {
        match op {
            Op::Wait => self.waits_delivered += 1,
            Op::Cancel(sequence) => {
                context.receipts.cancel_queued(sequence, Reply::Cancelled)?;
                self.cancellations += 1;
            }
        }
        Ok(Disposition::Complete(Reply::Done))
    }
}
struct Gain {
    value: Box<[f32]>,
    drops: Arc<AtomicUsize>,
}
impl Drop for Gain {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}
impl Processor<Ledger> for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Ledger>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        output[0] = input[0] * self.value[0];
        Ok(())
    }
}
fn submit(
    control: &mut Control<Gain, Ledger>,
    lane: Lane,
    command: Command<Gain, Ledger>,
) -> Ticket {
    control
        .submit(lane, command)
        .unwrap_or_else(|refusal| panic!("unexpected refusal: {}", refusal.error))
}
fn render(runtime: &mut GraphRuntime<Gain, Ledger>, frames: usize) -> Result<f32, Error> {
    let mut output = [99.0];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames,
                inputs: &[&[1.0]],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })?;
    Ok(output[0])
}
fn apply(
    control: &mut Control<Gain, Ledger>,
    runtime: &mut GraphRuntime<Gain, Ledger>,
    lane: Lane,
    command: Command<Gain, Ledger>,
) -> (Completion<Command<Gain, Ledger>, Retired<Gain>>, f32) {
    let ticket = submit(control, lane, command);
    let sample = render(runtime, 1).unwrap();
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    (completion, sample)
}
fn schedule(at: Frame, operation: Op) -> Command<Gain, Ledger> {
    Command::Schedule {
        at,
        action: Action::Planner {
            planner: planner(),
            operation,
        },
        payload: Box::default(),
    }
}
fn run(graceful: bool) -> Result<(), Error> {
    let config = Config {
        max_frames: 1,
        max_nodes: 1,
        max_ports: 4,
        max_edges: 4,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        command_slots: 2,
        scheduled_events: 1,
        resource_slots: 4,
        resource_pages: 8,
        page_bytes: 16,
        budgets: Budgets {
            commands: 2,
            events: 1,
            transfer_bytes: 16,
            validation_steps: 128,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let mut store = ResourceStore::new(config)?;
    let resource = store.create()?;
    let old = store.begin_upload(
        resource,
        Revision(0),
        Description {
            format: Format::Audio { channels: 1 },
            frames: 1,
            bytes: 4,
        },
    )?;
    store.begin_callback();
    store.write_upload(old, 0, &0.5f32.to_le_bytes())?;
    store.finish_upload(old)?;
    store.commit(old)?;
    let snapshot = store.snapshot(resource, Frame(0))?;
    let upload = store.begin_upload(
        resource,
        Revision(1),
        Description {
            format: Format::Audio { channels: 1 },
            frames: 8,
            bytes: 32,
        },
    )?;
    store.begin_callback();
    store.write_upload(upload, 0, &0.25f32.to_le_bytes())?;
    assert_eq!(store.usage().reserved_pages, 1);
    let ledger = Ledger {
        resource,
        snapshot: Some(snapshot),
        upload: Some(upload),
        waits_delivered: 0,
        cancellations: 0,
    };
    let drops = Arc::new(AtomicUsize::new(0));
    let gain = Gain {
        value: vec![1.0].into_boxed_slice(),
        drops: Arc::clone(&drops),
    };
    let graph = GraphDescription {
        nodes: vec![node()],
        connections: vec![
            Connection {
                source: Endpoint::host(PortType::Audio, 0),
                destination: Endpoint::node(node(), PortType::Audio, 0),
            },
            Connection {
                source: Endpoint::node(node(), PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, 0),
            },
        ],
    };
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node(), gain)],
        ledger,
        store,
        &graph,
        &[],
    )?;
    let (completion, sample) = apply(
        &mut control,
        &mut runtime,
        Lane::Normal,
        schedule(Frame(100), Op::Wait),
    );
    assert_eq!(sample, 1.0);
    assert!(matches!(
        completion.outcome,
        Outcome::Scheduled {
            effective_frame: Frame(100),
            late_by: 0
        }
    ));
    let future = completion.sequence;
    drop(completion);
    assert!(control.try_execution().is_none());
    let at = runtime.next_frame();
    assert_eq!(render(&mut runtime, 0)?, 99.0);
    assert_eq!(runtime.next_frame(), at);
    assert!(control.try_execution().is_none());
    if graceful {
        let (completion, sample) = apply(
            &mut control,
            &mut runtime,
            Lane::Recovery,
            schedule(at, Op::Cancel(future)),
        );
        assert_eq!(sample, 1.0);
        assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
        let cancel = completion.sequence;
        drop(completion);
        let mut seen = [false; 2];
        for _ in 0..2 {
            let execution = control.try_execution().unwrap();
            assert_eq!(execution.result, Ok(()));
            assert_eq!(execution.actual_frame, at);
            if execution.sequence == future {
                assert_eq!(execution.delivered_frame, None);
                assert_eq!(execution.operation_result, Some(Reply::Cancelled));
                seen[0] = true;
            } else {
                assert_eq!(execution.sequence, cancel);
                assert_eq!(execution.delivered_frame, Some(at));
                assert_eq!(execution.operation_result, Some(Reply::Done));
                seen[1] = true;
            }
        }
        assert_eq!(seen, [true; 2]);
    }
    let empty = compile(runtime.catalog(), &GraphDescription::default())?;
    let expected = runtime.epoch();
    let ticket = submit(&mut control, Lane::Normal, Command::commit(expected, empty));
    assert_eq!(render(&mut runtime, 1)?, 0.0);
    assert_eq!(runtime.epoch(), expected.checked_next()?);
    let queued = if graceful {
        let completion = control.try_completion().unwrap();
        assert_eq!(completion.sequence, ticket.sequence());
        assert_eq!(completion.outcome, Outcome::Applied);
        assert!(matches!(&completion.retired, Some(Retired::Plan(plan)) if plan.contains(node())));
        drop(completion);
        let (completion, sample) = apply(
            &mut control,
            &mut runtime,
            Lane::Normal,
            Command::Remove { node: node() },
        );
        assert_eq!(sample, 0.0);
        assert_eq!(completion.outcome, Outcome::Applied);
        assert!(matches!(&completion.retired, Some(Retired::Processor(_))));
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        drop(completion);
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert_eq!(control.outstanding(), 0);
        None
    } else {
        let queued = submit(
            &mut control,
            Lane::Normal,
            Command::resource(Request::Write {
                upload,
                offset: 4,
                bytes: vec![0; 4].into_boxed_slice(),
            }),
        );
        assert_eq!(control.outstanding(), 2);
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        Some(queued)
    };
    let stopped_frame = runtime.next_frame();
    let (mut ledger, mut store) = runtime.into_planner_and_resources();
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert!(!control.connected());
    assert_eq!(ledger.waits_delivered, 0);
    assert_eq!(ledger.cancellations, usize::from(graceful));
    if !graceful {
        let completion = control.try_completion().unwrap();
        assert_eq!(completion.sequence, ticket.sequence());
        assert_eq!(completion.outcome, Outcome::Applied);
        assert!(matches!(&completion.retired, Some(Retired::Plan(_))));
        drop(completion);
        assert_eq!(control.outstanding(), 1);
    }
    assert!(control.try_completion().is_none());
    assert!(control.try_execution().is_none());
    let refusal = control
        .submit(Lane::Normal, Command::ResetMidi)
        .err()
        .unwrap();
    assert_eq!(refusal.error, Error::Disconnected);
    assert!(matches!(refusal.value, Command::ResetMidi));
    assert!(stopped_frame < Frame(100));
    let snapshot = ledger.snapshot.take().unwrap();
    assert_eq!(store.currentness(snapshot.id)?, Currentness::Current);
    let mut cursor = store.cursor(snapshot.id)?;
    let mut bytes = [0; 4];
    assert_eq!(
        assert_no_heap(|| store.render_io().read(&mut cursor, &mut bytes))?,
        4
    );
    assert_eq!(bytes, 0.5f32.to_le_bytes());
    assert_eq!(
        store.finish_upload(ledger.upload.unwrap()),
        Err(Error::Incomplete)
    );
    store.cancel_upload(ledger.upload.take().unwrap())?;
    store.release_snapshot(snapshot.id)?;
    store.remove(ledger.resource)?;
    store.begin_callback();
    store.reclaim();
    assert_eq!(store.usage().reserved_pages, 0);
    assert_eq!(store.usage().free_pages, 8);
    assert_eq!(store.usage().free_versions, 4);
    assert_eq!(store.usage().free_snapshots, 4);
    assert!(ledger.upload.is_none() && ledger.snapshot.is_none());
    drop((queued, ticket, control, ledger, store, drops));
    Ok(())
}
// Shutdown: settle owned work while callbacks run in the graceful branch; abandon it explicitly in the other.
fn main() -> Result<(), Error> {
    run(true)?;
    run(false)?;
    println!("shutdown owners: explicit future cancellation versus stopped abandonment, retained publication and complete store cleanup verified");
    Ok(())
}
