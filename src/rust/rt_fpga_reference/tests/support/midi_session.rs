pub use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{BufferId, JobProcessor},
    midi::*,
    pipeline::Pipeline,
    records::Identity,
    state::{Instruction, PlannedTransition, Program},
    storage::MediaStore,
    transitions::*,
};
pub fn msg(bytes: &[u8]) -> Message {
    Message::managed(bytes).unwrap()
}
pub fn event(offset: u32, bytes: &[u8]) -> InputEvent {
    InputEvent {
        input: 0,
        offset,
        message: msg(bytes),
    }
}
pub fn id(slot: usize) -> Identity {
    Identity {
        slot: slot as u32,
        generation: 1,
    }
}
pub fn limits(loops: u16) -> SessionLimits {
    SessionLimits {
        store: StoreLimits {
            block_events: 8,
            blocks: 128,
            max_events: 128,
            views: 64,
            snapshots: 64,
            retirements: 32,
        },
        loops,
        inputs: 1,
        destinations: 2,
        input_events_per_quantum: 64,
        history_frames: 128,
        maintenance_steps: 8,
    }
}
pub struct Fixture {
    pub pipeline: Pipeline,
    pub jobs: JobProcessor,
    pub audio: MediaStore,
    pub midi: MidiSession,
    pub q: u32,
    pub emitted: Vec<OutputEvent>,
    pub input: Vec<Fixed>,
    pub output: Vec<Fixed>,
    pub sequence: u64,
    pub capacity: usize,
}
impl Fixture {
    pub fn new(q: u32, loops: u16) -> Self {
        Self::with_limits(q, limits(loops))
    }
    pub fn with_limits(q: u32, limits: SessionLimits) -> Self {
        let plan = TransitionPlan::compile(
            PlanDescription {
                quantum: q,
                loops: (0..usize::from(limits.loops))
                    .map(|i| LoopSpec {
                        id: id(i),
                        kind: LoopKind::Primitive,
                        length: 0,
                        sync_source: None,
                        repeat_sync: false,
                        timeline: None,
                    })
                    .collect(),
            },
            TransitionLimits {
                loop_slots: u32::from(limits.loops),
                events: 512,
            },
        )
        .unwrap();
        let program = Program {
            sync_instructions: vec![],
            instructions: vec![
                Instruction::ProcessInput {
                    port: 0,
                    destination: BufferId(0),
                },
                Instruction::ProcessOutput {
                    source: BufferId(0),
                    port: 0,
                },
            ],
            quantum: q,
            chunk_frames: 1,
            jobs_per_quantum: 2,
        };
        let mut midi = MidiSession::prepare(limits).unwrap();
        for i in 0..limits.loops {
            midi.bind(
                i,
                Binding {
                    loop_id: id(usize::from(i)),
                    input: 0,
                    destination: i / 16,
                    channels: 1 << (i % 16),
                },
            )
            .unwrap();
        }
        Self {
            pipeline: Pipeline::prepare(plan, program, vec![], 2).unwrap(),
            jobs: JobProcessor::new(q as usize, 1),
            audio: MediaStore::new(1).unwrap(),
            midi,
            q,
            emitted: Vec::with_capacity(65536),
            input: vec![Fixed(17); q as usize * 2],
            output: vec![Fixed::ZERO; q as usize * 2],
            sequence: 0,
            capacity: usize::MAX,
        }
    }
    pub fn command(&mut self, slot: usize, operation: Operation) {
        self.sequence += 1;
        self.pipeline
            .write_plans(
                id(slot),
                [
                    Some(PlannedTransition {
                        sequence: self.sequence,
                        operation,
                        on_pulse: false,
                        effective: self.pipeline.state().frame(),
                        remaining_ticks: 0,
                    }),
                    None,
                ],
            )
            .unwrap();
    }
    pub fn try_run_frames(
        &mut self,
        events: &[InputEvent],
        frames: usize,
    ) -> Result<SessionReport, ProcessError> {
        let cap = self.capacity;
        let emitted = &mut self.emitted;
        let mut accepted = 0;
        let result = self.pipeline.process_midi_session(
            &mut self.jobs,
            &mut self.audio,
            &[&self.input[..frames]],
            &mut [&mut self.output[..frames]],
            frames,
            &mut self.midi,
            events,
            &mut |e| {
                if accepted == cap {
                    return Err(Error::Capacity);
                }
                accepted += 1;
                emitted.push(e);
                Ok(())
            },
        );
        result
    }
    pub fn run_frames(&mut self, events: &[InputEvent], frames: usize) -> SessionReport {
        let result = self.try_run_frames(events, frames).unwrap();
        assert_eq!(&self.output[..frames], &self.input[..frames]);
        result
    }
    pub fn run(&mut self, events: &[InputEvent]) -> SessionReport {
        self.run_frames(events, self.q as usize)
    }
}
