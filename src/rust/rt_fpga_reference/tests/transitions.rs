use rt_fpga_reference::{
    compiler::{compile, Graph, Node, NodeId},
    fixed::Fixed,
    jobs::{BufferId, JobProcessor, Memory},
    pipeline::{Pipeline, PipelineError},
    storage::{MediaStore, ViewId},
    transitions::*,
    Error,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn id(slot: u32) -> LoopId {
    LoopId {
        slot,
        generation: 7,
    }
}
fn plan(loops: usize, q: u32, length: u64) -> TransitionPlan {
    TransitionPlan::compile(
        PlanDescription {
            quantum: q,
            loops: (0..loops)
                .map(|slot| LoopSpec {
                    id: id(slot as u32),
                    kind: LoopKind::Primitive,
                    length,
                    sync_source: (loops > 1).then(|| id((slot as u32 + 1) % loops as u32)),
                    repeat_sync: false,
                    timeline: None,
                })
                .collect(),
        },
        TransitionLimits {
            loop_slots: loops as u32,
            pending_requests: (2 * loops) as u32,
            events: (4 * loops) as u32,
        },
    )
    .unwrap()
}
fn command(sequence: u64, operation: Operation) -> Command {
    Command {
        target: id(0),
        sequence,
        at: 0,
        timing: Timing::At,
        operation,
    }
}
fn playing() -> Operation {
    Operation::Play {
        position: 0,
        retrigger: true,
    }
}
fn pipeline() -> (Pipeline, JobProcessor) {
    let compiled = compile(&Graph {
        quantum: 16,
        chunk_frames: 16,
        nodes: vec![
            Node::LogicalPlayback {
                loop_id: id(0),
                view: ViewId(0),
            },
            Node::Output {
                input: NodeId(0),
                port: 0,
            },
            Node::Input { port: 0 },
            Node::Output {
                input: NodeId(2),
                port: 1,
            },
            Node::LogicalCapture {
                loop_id: id(0),
                input: NodeId(2),
                view: ViewId(0),
            },
        ],
    })
    .unwrap();
    let p = Pipeline::prepare(plan(1, 16, 0), compiled.program, compiled.initial_state, 8).unwrap();
    (p, JobProcessor::new(16, 8))
}

#[shoop_test]
fn indexed_sync_cycles_across_mask_words_run_without_copying_or_heap_work() {
    let mut engine = TransitionProcessor::prepare(plan(130, 16, 32));
    for slot in 0..130 {
        engine
            .submit(Command {
                target: id(slot),
                ..command(u64::from(slot) + 1, playing())
            })
            .unwrap();
    }
    assert_no_heap(|| {
        for _ in 0..40 {
            engine.process().unwrap();
        }
    });
    assert_eq!(engine.frame(), 640);
    assert_eq!(engine.states().count(), 130);
    assert!(engine
        .states()
        .all(|state| state.position == 32 && state.cycles == 19));
}

#[shoop_test]
fn cancellation_completion_collection_and_generations_survive_in_place_processing() {
    let mut engine = TransitionProcessor::prepare(plan(1, 16, 32));
    let first = engine.submit(command(1, playing())).unwrap();
    assert!(engine.cancel(first).unwrap());
    assert!(matches!(
        engine.take_completion(first).unwrap().unwrap().outcome,
        Outcome::Cancelled { frame: 0 }
    ));
    let second = engine.submit(command(2, playing())).unwrap();
    assert_eq!(second.slot, first.slot);
    assert!(second.generation > first.generation);
    assert_eq!(engine.cancel(first), Err(TransitionError::InvalidId));
    engine.process().unwrap();
    assert_eq!(engine.state(id(0)).unwrap().position, 16);
    assert!(matches!(
        engine.take_completion(second).unwrap().unwrap().outcome,
        Outcome::Applied { frame: 0 }
    ));
}

#[shoop_test]
fn recording_views_are_created_and_reused_without_host_preparation() {
    let (mut p, mut jobs) = pipeline();
    let mut media = MediaStore::with_chunk_limit(16, 4).unwrap();
    let mut a = [Fixed(1); 16];
    let mut b = [Fixed(1); 16];
    let first = p.submit(command(1, Operation::Record)).unwrap();
    p.process(
        &mut jobs,
        &mut media,
        &[&[Fixed(123); 16]],
        &mut [&mut a, &mut b],
    )
    .unwrap();
    assert_eq!(media.frames(ViewId(0)).unwrap(), 16);
    p.collect(first).unwrap();
    let retained = media.slice(ViewId(0), 0, 16).unwrap();
    let second = p.submit(command(2, Operation::Record)).unwrap();
    p.process(
        &mut jobs,
        &mut media,
        &[&[Fixed(456); 16]],
        &mut [&mut a, &mut b],
    )
    .unwrap();
    p.collect(second).unwrap();
    assert_eq!(media.snapshot().views.len(), 2);
    p.submit(command(3, playing())).unwrap();
    p.process(
        &mut jobs,
        &mut media,
        &[&[Fixed::ZERO; 16]],
        &mut [&mut a, &mut b],
    )
    .unwrap();
    assert_eq!(a, [Fixed(456); 16]);
    media.read_loop(retained, 0, 16, &mut a).unwrap();
    assert_eq!(a, [Fixed(123); 16]);
    assert_no_heap(|| {
        p.process(
            &mut jobs,
            &mut media,
            &[&[Fixed::ZERO; 16]],
            &mut [&mut a, &mut b],
        )
        .unwrap()
    });
}

#[shoop_test]
fn exhaustion_after_a_successful_quantum_silences_the_entire_host_block_and_latches() {
    let (mut p, mut jobs) = pipeline();
    let mut media = MediaStore::with_chunk_limit(16, 1).unwrap();
    p.submit(command(1, Operation::Record)).unwrap();
    let mut a = [Fixed(1); 32];
    let mut b = [Fixed(1); 32];
    let error = PipelineError::Jobs(Error::MemoryFull);
    assert_eq!(
        p.process_block(
            &mut jobs,
            &mut media,
            &[&[Fixed(99); 32]],
            &mut [&mut a, &mut b],
            32
        ),
        Err(error)
    );
    assert_eq!(p.transitions().frame(), 16);
    assert_eq!(a, [Fixed::ZERO; 32]);
    assert_eq!(b, [Fixed::ZERO; 32]);
    assert_eq!(jobs.buffer(BufferId(2)).unwrap(), &[Fixed(99); 16]);
    assert_eq!(p.fault(), Some(error));
    let snapshot = media.snapshot();
    assert_eq!(
        p.submit(command(2, playing())),
        Err(TransitionError::InvalidState)
    );
    a.fill(Fixed(1));
    b.fill(Fixed(1));
    assert_eq!(
        p.process_block(
            &mut jobs,
            &mut media,
            &[&[Fixed(88); 32]],
            &mut [&mut a, &mut b],
            32
        ),
        Err(error)
    );
    assert_eq!(media.snapshot(), snapshot);
    assert_eq!(a, [Fixed::ZERO; 32]);
    assert_eq!(b, [Fixed::ZERO; 32]);
    let (mut reset, mut reset_jobs) = pipeline();
    let mut reset_media = MediaStore::new(16).unwrap();
    reset.submit(command(1, Operation::Record)).unwrap();
    reset
        .process_block(
            &mut reset_jobs,
            &mut reset_media,
            &[&[Fixed(77); 32]],
            &mut [&mut a, &mut b],
            32,
        )
        .unwrap();
    assert_eq!(reset.transitions().frame(), 32);
    assert_eq!(b, [Fixed(77); 32]);
}

struct FaultMemory {
    late: bool,
}
impl Memory for FaultMemory {
    fn chunk_frames(&self) -> usize {
        16
    }
    fn begin_capture(&mut self, _: ViewId) -> Result<(), Error> {
        Err(Error::MemoryFull)
    }
    fn read(&mut self, _: ViewId, _: u64, _: u64, _: &mut [Fixed]) -> Result<(), Error> {
        Ok(())
    }
    fn write(&mut self, _: ViewId, _: &[Fixed]) -> Result<(), Error> {
        Ok(())
    }
    fn fault(&self) -> Option<Error> {
        self.late.then_some(Error::MemoryFull)
    }
}

#[shoop_test]
fn descriptor_exhaustion_is_fatal_and_partial_output_is_suppressed() {
    let (mut p, mut jobs) = pipeline();
    let mut memory = FaultMemory { late: false };
    p.submit(command(1, Operation::Record)).unwrap();
    let mut a = [Fixed(1); 16];
    let mut b = [Fixed(1); 16];
    assert_eq!(
        p.process(
            &mut jobs,
            &mut memory,
            &[&[Fixed(42); 16]],
            &mut [&mut a, &mut b]
        ),
        Err(PipelineError::Jobs(Error::MemoryFull))
    );
    assert_eq!(a, [Fixed::ZERO; 16]);
    assert_eq!(b, [Fixed::ZERO; 16]);
    assert_eq!(p.transitions().frame(), 0);
    assert_eq!(jobs.buffer(BufferId(2)).unwrap(), &[Fixed(42); 16]);
}

#[shoop_test]
fn late_memory_fault_is_reported_on_the_next_call_without_processing_more_audio() {
    let (mut p, mut jobs) = pipeline();
    let mut memory = FaultMemory { late: false };
    let mut a = [Fixed(1); 16];
    let mut b = [Fixed(1); 16];
    p.process(
        &mut jobs,
        &mut memory,
        &[&[Fixed(42); 16]],
        &mut [&mut a, &mut b],
    )
    .unwrap();
    assert_eq!(b, [Fixed(42); 16]);
    memory.late = true;
    assert_eq!(
        p.process(
            &mut jobs,
            &mut memory,
            &[&[Fixed(77); 16]],
            &mut [&mut a, &mut b]
        ),
        Err(PipelineError::Jobs(Error::MemoryFull))
    );
    assert_eq!(p.transitions().frame(), 16);
    assert_eq!(a, [Fixed::ZERO; 16]);
    assert_eq!(b, [Fixed::ZERO; 16]);
}

#[shoop_test]
fn malformed_block_is_rejected_without_faulting_the_engine() {
    let (mut p, mut jobs) = pipeline();
    let mut media = MediaStore::new(16).unwrap();
    let mut a = [Fixed(1); 15];
    assert_eq!(
        p.process_block(&mut jobs, &mut media, &[], &mut [&mut a], 15),
        Err(PipelineError::Jobs(Error::InvalidShape))
    );
    assert_eq!(p.fault(), None);
    assert_eq!(p.transitions().frame(), 0);
    let mut a = [Fixed(1); 16];
    let mut b = [Fixed(1); 16];
    p.process(
        &mut jobs,
        &mut media,
        &[&[Fixed(42); 16]],
        &mut [&mut a, &mut b],
    )
    .unwrap();
    assert_eq!(b, [Fixed(42); 16]);
}

#[shoop_test]
fn invalid_capture_binding_does_not_attempt_to_allocate_missing_descriptor_ids() {
    let mut media = MediaStore::new(16).unwrap();
    assert_eq!(
        Memory::begin_capture(&mut media, ViewId(usize::MAX)),
        Err(Error::InvalidView)
    );
    assert!(media.snapshot().views.is_empty());
    Memory::begin_capture(&mut media, ViewId(0)).unwrap();
    assert_eq!(media.snapshot().views.len(), 1);
}
