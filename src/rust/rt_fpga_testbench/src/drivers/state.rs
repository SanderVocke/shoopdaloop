use super::config::Config;
use crate::{
    bits::Bits,
    codec,
    random::Random,
    sim::{Sim, Snapshot},
};
use serde_json::{Value, json};

pub const SIGNALS: &[&str] = &[
    "busy",
    "done",
    "error_code",
    "instruction_pc",
    "run_ready",
    "cfg_ready",
    "cfg_response_valid",
    "cfg_response_error",
    "cfg_read_data",
    "job_valid",
    "job_ready",
    "job_data",
    "queued_jobs",
];
pub struct State {
    pub image: Program,
    pub random: Random,
    pub run_cycles: u64,
}
impl State {
    pub fn new(sim: &mut Sim<'_>, depths: [u64; 3], seed: u64) -> Self {
        sim.agent(crate::protocol::ReadyValid::new(
            "job_valid",
            "job_ready",
            &["job_data"],
        ));
        sim.agent(crate::protocol::Exclusion::new(
            "busy",
            &["cfg_ready", "run_ready", "job_valid"],
        ));
        sim.agent(crate::protocol::Bound::new("queued_jobs", depths[2]));
        Self {
            image: Program::new(depths),
            random: Random::new(seed),
            run_cycles: 0,
        }
    }
    pub fn tick(&mut self, sim: &mut Sim<'_>) -> (Snapshot, Snapshot) {
        sim.tick(SIGNALS)
    }
    pub fn reset(&mut self, sim: &mut Sim<'_>) {
        for name in [
            "run_valid",
            "run_instruction_count",
            "run_state_count",
            "run_quantum",
            "run_chunk_frames",
            "run_job_budget",
            "cfg_valid",
            "cfg_state",
            "cfg_write",
            "cfg_address",
            "cfg_write_data",
            "job_ready",
            "loop_read_ready",
            "loop_response_valid",
            "loop_response_error",
            "loop_response_data",
        ] {
            sim.model.set(name, 0);
        }
        sim.model.set("reset", 1);
        self.tick(sim);
        let (_, after) = self.tick(sim);
        assert_eq!(
            after.get("busy")
                | after.get("queued_jobs")
                | after.get("cfg_response_valid")
                | after.get("job_valid"),
            0
        );
        sim.model.set("reset", 0);
        self.tick(sim);
    }
    pub fn load(&mut self, sim: &mut Sim<'_>, case: &Value) {
        self.reset(sim);
        self.image.load(sim, case);
    }
    pub fn start(&mut self, sim: &mut Sim<'_>, overrides: &[(&str, u64)]) -> Snapshot {
        self.run_inputs(sim, overrides);
        for _ in 0..100 {
            let (before, after) = self.tick(sim);
            if before.get("run_ready") != 0 {
                sim.model.set("run_valid", 0);
                return after;
            }
        }
        panic!("state run admission timeout");
    }
    pub fn execute(&mut self, sim: &mut Sim<'_>, overrides: &[(&str, u64)]) -> u64 {
        sim.model.set("job_ready", 0);
        let mut after = self.start(sim, overrides);
        let accepted = sim.cycles;
        for _ in 0..20 * self.depths[0] + 20 {
            if after.get("done") != 0 {
                self.run_cycles = sim.cycles - accepted;
                sim.model.set("job_ready", 0);
                assert_eq!(after.get("busy"), 0);
                let status = after.get("error_code");
                if status == 0 {
                    assert_eq!(
                        after.get("instruction_pc"),
                        self.program["instructions"].as_array().unwrap().len() as u64
                    );
                }
                let (_, cleared) = self.tick(sim);
                assert_eq!(cleared.get("done"), 0);
                assert_eq!(cleared.get("error_code"), status);
                return status;
            }
            sim.model.set("job_ready", self.random.below(2));
            after = self.tick(sim).1;
        }
        panic!("state execution timeout");
    }
    pub fn drain(&mut self, sim: &mut Sim<'_>) -> Vec<Value> {
        let count = sim.model.get("queued_jobs");
        let mut jobs = Vec::new();
        sim.model.set("job_ready", 0);
        for _ in 0..3 {
            assert_eq!(self.tick(sim).1.get("queued_jobs"), count);
        }
        for cycle in 0..12 * self.depths[2] + 30 {
            sim.model
                .set("job_ready", u64::from(cycle % 7 == 6 || self.random.yes(3)));
            let (before, after) = self.tick(sim);
            let accepted = before.get("job_valid") != 0 && before.get("job_ready") != 0;
            if accepted {
                jobs.push(codec::job(before.bits("job_data")));
            }
            assert_eq!(
                after.get("queued_jobs"),
                before.get("queued_jobs") - u64::from(accepted)
            );
            if after.get("queued_jobs") == 0 {
                break;
            }
        }
        assert_eq!(jobs.len() as u64, count, "lost/extra FIFO jobs");
        sim.model.set("job_ready", 1);
        for _ in 0..5 {
            let (before, after) = self.tick(sim);
            assert_eq!(before.get("job_valid") | after.get("job_valid"), 0);
        }
        sim.model.set("job_ready", 0);
        jobs
    }
}

/// Program/configuration transactions shared by state, sample and media pipelines.
pub struct Program {
    pub depths: [u64; 3],
    pub program: Value,
    pub state_count: usize,
}
impl Program {
    pub fn new(depths: [u64; 3]) -> Self {
        Self {
            depths,
            program: Value::Null,
            state_count: 0,
        }
    }
    pub fn configure(
        &mut self,
        sim: &mut Sim<'_>,
        state: bool,
        address: u64,
        value: Option<&Bits>,
        error: bool,
    ) -> Bits {
        Config::new("cfg", "cfg_state").access(sim, u64::from(state), address, value, error)
    }
    pub fn load(&mut self, sim: &mut Sim<'_>, case: &Value) {
        assert_eq!(case["queue_capacity"].as_u64().unwrap(), self.depths[2]);
        self.program = case["program"].clone();
        self.state_count = case["initial_state"]["memory"].as_array().unwrap().len();
        assert!(
            self.program["instructions"].as_array().unwrap().len() as u64 <= self.depths[0]
                && self.state_count as u64 <= self.depths[1]
        );
        for (i, v) in self.program["instructions"]
            .as_array()
            .unwrap()
            .clone()
            .iter()
            .enumerate()
        {
            let value = codec::instruction(v).unwrap();
            self.configure(sim, false, i as u64, Some(&value), false);
            assert_eq!(
                self.configure(sim, false, i as u64, None, false).get(0, 64),
                value.get(0, 64),
                "instruction readback"
            );
            assert_eq!(
                self.configure(sim, false, i as u64, None, false)
                    .get(64, 32),
                value.get(64, 32),
                "instruction high readback"
            );
        }
        for (i, v) in case["initial_state"]["memory"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let value = codec::state(v);
            self.configure(sim, true, i as u64, Some(&value), false);
            assert_eq!(
                self.configure(sim, true, i as u64, None, false)
                    .array::<5>(),
                value.array::<5>(),
                "state readback"
            );
        }
    }
    pub fn snapshot(&mut self, sim: &mut Sim<'_>) -> Value {
        let memory: Vec<_> = (0..self.state_count)
            .map(|i| codec::decode_state(&self.configure(sim, true, i as u64, None, false)))
            .collect();
        json!({"memory":memory})
    }
    pub fn run_inputs(&self, sim: &mut Sim<'_>, overrides: &[(&str, u64)]) {
        sim.model.set_all(&[
            ("run_valid", 1),
            (
                "run_instruction_count",
                self.program["instructions"].as_array().unwrap().len() as u64,
            ),
            ("run_state_count", self.state_count as u64),
            ("run_quantum", self.program["quantum"].as_u64().unwrap()),
            (
                "run_chunk_frames",
                self.program["chunk_frames"].as_u64().unwrap(),
            ),
            (
                "run_job_budget",
                self.program["jobs_per_quantum"].as_u64().unwrap(),
            ),
        ]);
        sim.model.set_all(overrides);
    }
}
impl std::ops::Deref for State {
    type Target = Program;
    fn deref(&self) -> &Self::Target {
        &self.image
    }
}
impl std::ops::DerefMut for State {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.image
    }
}
