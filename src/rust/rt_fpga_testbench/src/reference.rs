//! Fixture adapters for independent functional component models.
use rt_fpga_reference::{
    Error,
    fixed::Fixed,
    jobs::{BufferId, Job, JobProcessor, JobQueue, Memory},
    records::{LoopRecord, SettledRecords},
    state::{Address, Program, StateProcessor},
    storage::ViewId,
};
use serde_json::{Value, json};

fn processor(case: &Value) -> StateProcessor {
    StateProcessor::new(serde_json::from_value(case["initial_state"]["memory"].clone()).unwrap())
}
fn program(case: &Value) -> Program {
    serde_json::from_value(case["program"].clone()).unwrap()
}
pub fn state(case: &Value, records: &[LoopRecord]) -> (u64, Value, Vec<Value>) {
    let mut state = processor(case);
    let mut queue = JobQueue::new(case["queue_capacity"].as_u64().unwrap() as usize);
    let status = match state.run_with_records(&program(case), &mut queue, SettledRecords(records)) {
        Ok(()) => 0,
        Err(Error::QueueFull) => 1,
        Err(_) => 2,
    };
    (
        status,
        json!({"memory":state.values()}),
        queue
            .iter()
            .map(|job| serde_json::to_value(job).unwrap())
            .collect(),
    )
}

struct NoMedia;
impl Memory for NoMedia {
    fn supports_media(&self) -> bool {
        false
    }
    fn chunk_frames(&self) -> usize {
        1
    }
    fn begin_capture(&mut self, _: ViewId) -> Result<(), Error> {
        Err(Error::InvalidView)
    }
    fn read(&mut self, _: ViewId, _: u64, _: u64, _: &mut [Fixed]) -> Result<(), Error> {
        Err(Error::InvalidView)
    }
    fn write(&mut self, _: ViewId, _: &[Fixed]) -> Result<(), Error> {
        Err(Error::InvalidView)
    }
}

pub struct Audio {
    state: StateProcessor,
    program: Program,
    jobs: JobProcessor,
    buffers: usize,
    previous_outputs: Vec<Vec<Fixed>>,
    capacity: usize,
}
impl Audio {
    pub fn new(case: &Value) -> Self {
        let program = program(case);
        let buffers = case["buffers"].as_u64().unwrap() as usize;
        Self {
            state: processor(case),
            jobs: JobProcessor::with_history_slots(
                program.quantum as usize,
                buffers,
                case["history_slots"].as_u64().unwrap_or(buffers as u64) as usize,
            ),
            program: program.clone(),
            buffers,
            previous_outputs: vec![
                vec![Fixed::ZERO; program.quantum as usize];
                case["output_ports"].as_u64().unwrap() as usize
            ],
            capacity: case["queue_capacity"].as_u64().unwrap() as usize,
        }
    }
    pub fn quantum(&mut self, step: &Value, records: &[LoopRecord]) -> Value {
        self.quantum_with_memory(step, records, &mut NoMedia)
    }
    pub fn quantum_with_memory(
        &mut self,
        step: &Value,
        records: &[LoopRecord],
        memory: &mut impl Memory,
    ) -> Value {
        for write in step["writes"].as_array().unwrap() {
            self.state
                .write(
                    Address(write["address"].as_u64().unwrap() as usize),
                    serde_json::from_value(write["value"].clone()).unwrap(),
                )
                .unwrap();
        }
        let inputs: Vec<Vec<Fixed>> = serde_json::from_value(step["inputs"].clone()).unwrap();
        let mut outputs = self.previous_outputs.clone();
        let mut queue = JobQueue::new(self.capacity);
        let state_result =
            self.state
                .run_with_records(&self.program, &mut queue, SettledRecords(records));
        let all_jobs: Vec<Job> = queue.iter().copied().collect();
        let (stage, result) = if state_result.is_err() {
            ("state", state_result)
        } else {
            (
                "jobs",
                self.jobs.run(
                    &mut queue,
                    memory,
                    &inputs.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                    &mut outputs
                        .iter_mut()
                        .map(Vec::as_mut_slice)
                        .collect::<Vec<_>>(),
                ),
            )
        };
        let status = match result {
            Ok(()) => json!({"kind":"completed"}),
            Err(error) => json!({"kind":"failed","value":{"error":match error {
                Error::InvalidBuffer => "invalid_buffer",
                Error::InvalidShape => "invalid_shape",
                Error::QueueFull => "queue_full",
                Error::InvalidView => "invalid_view",
                Error::MemoryFull => "memory_full",
                Error::UnsupportedJob => "unsupported_job",
                _ => "invalid_state",
            }}}),
        };
        self.previous_outputs = outputs.clone();
        let buffers: Vec<_> = (0..self.buffers)
            .map(|i| self.jobs.buffer(BufferId(i)).ok())
            .collect();
        json!({"jobs":all_jobs,"remaining_jobs":queue.iter().collect::<Vec<_>>(),
            "state":{"memory":self.state.values()},"buffers":buffers,"outputs":outputs,
            "stage":stage,"status":status})
    }
}
