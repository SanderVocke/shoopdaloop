use super::{config::Config, state::Program};
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
    "error_stage",
    "run_ready",
    "cfg_ready",
    "cfg_response_valid",
    "cfg_response_error",
    "cfg_read_data",
    "queued_jobs",
    "state_busy",
    "state_done",
    "jobs_busy",
    "jobs_done",
    "buffer_ready",
    "audio_cfg_ready",
    "audio_cfg_response_valid",
    "audio_cfg_response_error",
    "audio_cfg_read_data",
    "monitor_valid",
    "monitor_ready",
    "monitor_data",
    "drain_valid",
];
pub struct Audio {
    pub program: Program,
    pub capacities: [u64; 7],
    jobs: std::rc::Rc<std::cell::RefCell<Jobs>>,
    pub buffers: u64,
    pub outputs: u64,
    pub frames: u64,
    pub random: Random,
}
impl Audio {
    pub fn new(sim: &mut Sim<'_>, capacities: [u64; 7], seed: u64) -> Self {
        Self::with_guards(sim, capacities, seed, &["busy"])
    }
    pub fn coordinated(sim: &mut Sim<'_>, capacities: [u64; 7], seed: u64) -> Self {
        Self::with_guards(sim, capacities, seed, &["state_busy", "jobs_busy"])
    }
    fn with_guards(sim: &mut Sim<'_>, capacities: [u64; 7], seed: u64, guards: &[&str]) -> Self {
        sim.agent(crate::protocol::ReadyValid::new(
            "monitor_valid",
            "monitor_ready",
            &["monitor_data"],
        ));
        for guard in guards {
            sim.agent(crate::protocol::Exclusion::new(
                guard,
                &["cfg_ready", "audio_cfg_ready", "run_ready"],
            ));
        }
        sim.agent(crate::protocol::Bound::new("queued_jobs", capacities[2]));
        let jobs = std::rc::Rc::new(std::cell::RefCell::new(Jobs::default()));
        sim.agent(jobs.clone());
        Self {
            program: Program::new(capacities[..3].try_into().unwrap()),
            capacities,
            jobs,
            buffers: 0,
            outputs: 0,
            frames: 0,
            random: Random::new(seed),
        }
    }
    pub fn tick(&mut self, s: &mut Sim<'_>) -> (Snapshot, Snapshot) {
        s.tick(SIGNALS)
    }
    pub fn consumed(&self) -> Vec<Value> {
        self.jobs.borrow().consumed.clone()
    }
    pub fn drained(&self) -> Vec<Value> {
        self.jobs.borrow().drained.clone()
    }
    pub fn clear_jobs(&self) {
        let mut jobs = self.jobs.borrow_mut();
        jobs.consumed.clear();
        jobs.drained.clear();
    }
    pub fn reset(&mut self, s: &mut Sim<'_>) {
        for name in [
            "run_valid",
            "run_instruction_count",
            "run_state_count",
            "run_quantum",
            "run_chunk_frames",
            "run_job_budget",
            "run_buffers",
            "run_inputs",
            "run_outputs",
            "cfg_valid",
            "cfg_state",
            "cfg_write",
            "cfg_address",
            "cfg_write_data",
            "audio_cfg_valid",
            "audio_cfg_space",
            "audio_cfg_write",
            "audio_cfg_address",
            "audio_cfg_write_data",
            "drain_ready",
            "loop_read_ready",
            "loop_response_valid",
            "loop_response_error",
            "loop_response_data",
            "media_fault",
            "media_hint_ready",
        ] {
            if s.model.has(name) {
                s.model.set(name, 0);
            }
        }
        s.model.set_all(&[("reset", 1), ("job_enable", 1)]);
        self.tick(s);
        let (_, after) = self.tick(s);
        assert_eq!(
            after.get("busy") | after.get("queued_jobs") | after.get("buffer_ready"),
            0
        );
        s.model.set("reset", 0);
        self.tick(s);
    }
    pub fn sample_config(
        &mut self,
        s: &mut Sim<'_>,
        space: u64,
        address: u64,
        value: Option<i32>,
        error: bool,
    ) -> i32 {
        Config::new("audio_cfg", "audio_cfg_space")
            .access(
                s,
                space,
                address,
                value.map(|v| Bits::from_u64(u64::from(v as u32))).as_ref(),
                error,
            )
            .get(0, 32) as i32
    }
    pub fn load(&mut self, s: &mut Sim<'_>, case: &Value) {
        self.frames = case["program"]["quantum"].as_u64().unwrap();
        self.buffers = case["buffers"].as_u64().unwrap();
        self.outputs = case["output_ports"].as_u64().unwrap();
        assert!(
            self.frames > 0
                && self.frames <= self.capacities[3]
                && self.buffers <= self.capacities[4]
                && self.outputs <= self.capacities[6]
        );
        self.reset(s);
        self.program.load(s, case);
        for port in 0..self.outputs {
            for f in 0..self.frames {
                self.sample_config(s, 1, port * self.capacities[3] + f, Some(0), false);
            }
        }
    }
    pub fn start(&mut self, s: &mut Sim<'_>) -> Snapshot {
        self.program.run_inputs(s, &[]);
        for _ in 0..100 {
            let (before, after) = self.tick(s);
            if before.get("run_ready") != 0 {
                s.model.set("run_valid", 0);
                return after;
            }
        }
        panic!("audio run admission timeout");
    }
    pub fn read_samples(&mut self, s: &mut Sim<'_>) -> (Vec<Value>, Vec<Vec<i32>>) {
        let ready = s.model.bits("buffer_ready");
        let buffers = (0..self.buffers)
            .map(|b| {
                if ready.get(b as usize, 1) != 0 {
                    json!(
                        (0..self.frames)
                            .map(|f| self.sample_config(
                                s,
                                2,
                                b * self.capacities[3] + f,
                                None,
                                false
                            ))
                            .collect::<Vec<_>>()
                    )
                } else {
                    Value::Null
                }
            })
            .collect();
        let outputs = (0..self.outputs)
            .map(|p| {
                (0..self.frames)
                    .map(|f| self.sample_config(s, 1, p * self.capacities[3] + f, None, false))
                    .collect()
            })
            .collect();
        (buffers, outputs)
    }
    pub fn execute(&mut self, s: &mut Sim<'_>, inputs: &[Vec<i32>], stalls: bool) -> (u64, u64) {
        assert!(
            inputs.len() as u64 <= self.capacities[5]
                && inputs.iter().all(|p| p.len() as u64 == self.frames)
        );
        for (p, data) in inputs.iter().enumerate() {
            for (f, &value) in data.iter().enumerate() {
                self.sample_config(
                    s,
                    0,
                    p as u64 * self.capacities[3] + f as u64,
                    Some(value),
                    false,
                );
            }
        }
        self.clear_jobs();
        s.model.set_all(&[
            ("run_buffers", self.buffers),
            ("run_inputs", inputs.len() as u64),
            ("run_outputs", self.outputs),
            ("job_enable", 1),
            ("drain_ready", 0),
        ]);
        let mut after = self.start(s);
        let mut done = false;
        for index in
            0..200 * self.capacities[2] + 40 * self.capacities[3] * self.capacities[0] + 100
        {
            if after.get("done") != 0 {
                done = true;
                break;
            }
            s.model.set(
                "job_enable",
                u64::from(!stalls || index % 11 == 10 || self.random.yes(4)),
            );
            after = self.tick(s).1;
        }
        assert!(done, "audio execution timeout");
        s.model.set("job_enable", 1);
        assert_eq!(after.get("busy"), 0);
        let result = (after.get("error_stage"), after.get("error_code"));
        assert_eq!(self.tick(s).1.get("done"), 0);
        for _ in 0..12 * self.capacities[2] + 20 {
            if s.model.get("queued_jobs") == 0 {
                break;
            }
            s.model.set("drain_ready", self.random.below(2));
            self.tick(s);
        }
        assert_eq!(s.model.get("queued_jobs"), 0, "remaining-job drain timeout");
        s.model.set("drain_ready", 0);
        self.tick(s);
        assert_eq!(s.model.get("drain_valid"), 0);
        result
    }
    pub fn quantum(&mut self, s: &mut Sim<'_>, step: &Value, stalls: bool) -> Value {
        for write in step["writes"].as_array().unwrap() {
            let address = write["address"].as_u64().unwrap();
            assert!(address < self.program.state_count as u64);
            self.program.configure(
                s,
                true,
                address,
                Some(&codec::state(&write["value"])),
                false,
            );
        }
        let inputs: Vec<Vec<i32>> = serde_json::from_value(step["inputs"].clone()).unwrap();
        let (stage, code) = self.execute(s, &inputs, stalls);
        let error: &[&str] = if stage == 1 {
            &["", "queue_full", "invalid_state"]
        } else {
            assert_eq!(stage, 2);
            &[
                "",
                "invalid_buffer",
                "invalid_shape",
                "unsupported_job",
                "unsupported_job",
                "invalid_view",
                "memory_full",
                "invalid_state",
                "memory_fault",
            ]
        };
        let status = if code == 0 {
            json!({"kind":"completed"})
        } else {
            json!({"kind":"failed","value":{"error":error[code as usize]}})
        };
        let (buffers, outputs) = self.read_samples(s);
        let mut jobs = self.consumed();
        jobs.extend(self.drained());
        json!({"jobs":jobs,"remaining_jobs":self.drained(),"state":self.program.snapshot(s),"buffers":buffers,"outputs":outputs,"stage":if stage == 1 {"state"} else {"jobs"},"status":status})
    }
}

#[derive(Default)]
struct Jobs {
    consumed: Vec<Value>,
    drained: Vec<Value>,
}
impl crate::sim::Agent for Jobs {
    fn sample(&mut self, m: &crate::model::Model<'_>, e: crate::clock::Edges) {
        if !e.rises(0) {
            return;
        }
        if m.get("reset") != 0 {
            self.consumed.clear();
            self.drained.clear();
            return;
        }
        if m.get("monitor_valid") != 0 && m.get("monitor_ready") != 0 {
            let target = if m.get("jobs_busy") != 0 {
                &mut self.consumed
            } else {
                &mut self.drained
            };
            target.push(codec::job(&m.bits("monitor_data")));
        }
    }
}
