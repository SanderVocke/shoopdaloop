use super::*;
use crate::assembly::{LoopControl, LoopRuntime};
use crate::{graph_transfer::tests::transfer, graph_wire::Client, guarded, scenario};
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use shoop_loops::{Timing, Transition};
use std::sync::{atomic::AtomicU32, Arc};

fn tick(engine: &mut LoopRuntime<Mix>) {
    let mut left = [0.; 1];
    let mut right = [0.; 1];
    let input = [MidiBuffer::new(2, 6).unwrap()];
    let mut output = [MidiBuffer::new(256, 1024).unwrap()];
    guarded(|| {
        engine.process(
            AudioBlock {
                frame: engine.next_frame(),
                frames: 1,
                inputs: &[&[0.], &[0.]],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
    })
    .unwrap();
}
fn dispose(control: LoopControl<Mix>, engine: LoopRuntime<Mix>) {
    drop(control);
    let mut stopped = engine.into_stopped();
    stopped.cleanup().unwrap();
    for _ in 0..512 {
        stopped.resources().begin_callback();
        stopped.resources().reclaim();
    }
    let usage = stopped.resource_usage();
    assert_eq!(
        (usage.free_pages, usage.free_versions, usage.free_snapshots),
        (256, 16, 16)
    );
    assert_eq!(stopped.media_usage().references, 0);
}

#[test]
fn local_transport_refusal_returns_the_candidate_and_next_bank_generation_without_rendering() {
    let (mut control, mut engine, target) =
        scenario::prepare_with_fault(0, Arc::new(AtomicU32::new(0)));
    engine.prepare_graph_staging(2).unwrap();
    let plan = control.compile_graph(&scenario::routes(false)).unwrap();
    let mut graphs = Graphs::new(7, plan.config()).unwrap();
    let id = graphs.pool.id(0).unwrap();
    transfer(&mut graphs.pool, id, Epoch(1), &plan);
    for _ in 0..8 {
        guarded(|| {
            control.submit(
                Lane::Normal,
                Frame(0),
                DomainOperation::Transition {
                    target,
                    timing: Timing::At,
                    transition: Transition::Stop,
                },
            )
        })
        .unwrap_or_else(|e| panic!("fill: {:?}", e.error));
    }
    let mut client = Client::new(7).unwrap();
    let bytes = guarded(|| {
        client.prepare(
            Operation::Stage {
                bank: id,
                slot: 0,
                epoch: Epoch(1),
            },
            false,
        )
    })
    .unwrap();
    guarded(|| graphs.ingest(&mut control, &bytes, 0)).unwrap();
    let packet = guarded(|| graphs.next_response()).unwrap().unwrap();
    let response = guarded(|| client.receive(&packet)).unwrap();
    assert_eq!(response.status, Status::Rejected);
    assert_eq!(response.error, 8);
    assert_eq!(response.runtime_sequence, 0);
    let bank = response.bank.unwrap();
    assert_eq!(
        bank.id,
        Id {
            slot: 0,
            generation: 2
        }
    );
    assert_eq!(
        bank.capacity,
        Counts {
            steps: 4,
            edges: 16,
            bindings: 4
        }
    );
    assert_eq!(graphs.pool.phase(bank.id), Ok(graph_transfer::Phase::Idle));
    assert_eq!(
        graphs.pool.plan(bank.id).unwrap().descriptor(),
        plan.descriptor()
    );
    assert_eq!(graphs.outstanding(), 0);
    assert_eq!(graphs.retained(), 0);
    assert_eq!(client.outstanding(), 0);
    assert!(!graphs.fatal);
    assert!(graphs.orphan.is_none());
    assert_eq!(engine.next_frame(), Frame(0));
    dispose(control, engine);
}

#[test]
fn unexpected_owning_completion_is_returned_intact_without_recycling_or_fabricating_a_reply() {
    let (mut control, mut engine, _) = scenario::prepare_with_fault(0, Arc::new(AtomicU32::new(0)));
    engine.prepare_graph_staging(2).unwrap();
    let plan = control.compile_graph(&scenario::routes(false)).unwrap();
    let mut graphs = Graphs::new(7, plan.config()).unwrap();
    let id = graphs.pool.id(0).unwrap();
    transfer(&mut graphs.pool, id, Epoch(1), &plan);
    let mut client = Client::new(7).unwrap();
    let bytes = client
        .prepare(
            Operation::Stage {
                bank: id,
                slot: 0,
                epoch: Epoch(1),
            },
            true,
        )
        .unwrap();
    guarded(|| graphs.ingest(&mut control, &bytes, 0)).unwrap();
    tick(&mut engine);
    let mut completion = guarded(|| control.try_completion()).unwrap();
    assert_eq!(completion.outcome, Outcome::Cancelled);
    let Command::StageGraph {
        expected_epoch,
        plan: Some(owner),
        ..
    } = &mut completion.request
    else {
        panic!("cancelled owner");
    };
    let pointer = owner.descriptor().steps.as_ptr();
    let storage = owner.storage_bytes().unwrap();
    *expected_epoch = Epoch(2);
    let completion = guarded(|| graphs.completion(completion)).err().unwrap();
    let Command::StageGraph {
        plan: Some(owner), ..
    } = &completion.request
    else {
        panic!("quarantined owner");
    };
    assert_eq!(owner.descriptor().steps.as_ptr(), pointer);
    assert_eq!(owner.storage_bytes().unwrap(), storage);
    assert_eq!(graphs.pool.phase(id), Ok(graph_transfer::Phase::Loaned));
    assert_eq!(graphs.outstanding(), 1);
    assert_eq!(graphs.retained(), 0);
    assert_eq!(client.outstanding(), 1);
    assert!(guarded(|| graphs.next_response()).unwrap().is_none());
    drop(completion);
    dispose(control, engine);
}
