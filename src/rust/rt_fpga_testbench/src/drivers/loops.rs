//! Coordinator transactions compose the same transition, program, audio and store drivers.
use crate::{
    Result,
    bits::Bits,
    codec,
    drivers::{
        audio::Audio,
        pipeline::Pipeline,
        store::{Geometry, Store},
        transition::{self, Transition},
    },
    memory::Mode,
    sim::Sim,
};
use serde_json::{Value, json};
pub struct Loops {
    pub pipeline: Pipeline,
    pub transition: Transition,
    pub case: Value,
    pub stalls: bool,
    pending: Vec<Value>,
}
impl Loops {
    pub fn new(
        s: &mut Sim<'_>,
        depths: [u64; 10],
        g: Geometry,
        mode: Mode,
        seed: u64,
    ) -> Result<Self> {
        let audio = Audio::coordinated(s, depths[3..].try_into().unwrap(), seed);
        let store = Store::new(s, g, mode, seed + 17, "store_")?;
        let mut transition = Transition::new(depths[..3].try_into().unwrap());
        transition.busy_signal = "transition_busy";
        for guard in ["state_busy", "jobs_busy"] {
            s.agent(crate::protocol::Exclusion::new(
                guard,
                &["store_cmd_ready", "store_cfg_ready"],
            ));
        }
        Ok(Self {
            pipeline: Pipeline { audio, store },
            transition,
            case: Value::Null,
            stalls: true,
            pending: Vec::new(),
        })
    }
    pub fn initialize(&mut self, s: &mut Sim<'_>, case: &Value, image: &Value) {
        for n in [
            "run_valid",
            "cfg_valid",
            "audio_cfg_valid",
            "store_cmd_valid",
            "store_cfg_valid",
            "store_write_valid",
            "store_read_ready",
            "drain_ready",
        ] {
            s.model.set(n, 0);
        }
        s.model.set("job_enable", 1);
        self.transition.load(s, image);
        let audio = &mut self.pipeline.audio;
        audio.frames = case["program"]["quantum"].as_u64().unwrap();
        audio.buffers = case["buffers"].as_u64().unwrap();
        audio.outputs = case["output_ports"].as_u64().unwrap();
        assert!(
            audio.frames <= audio.capacities[3]
                && audio.buffers <= audio.capacities[4]
                && audio.outputs <= audio.capacities[6]
        );
        audio.program.load(s, case);
        self.pipeline.store.load(s, &case["media"]);
        self.case = case.clone();
        self.pending.clear();
    }
    pub fn step(&mut self, s: &mut Sim<'_>, step: &Value, snapshot_media: bool) -> Value {
        let control = self.transition.controls(s, &step["transition"]);
        let audio = &mut self.pipeline.audio;
        let faulted = s.model.get("faulted") != 0;
        if !faulted {
            for w in step["writes"].as_array().unwrap() {
                audio.program.configure(
                    s,
                    true,
                    w["address"].as_u64().unwrap(),
                    Some(&codec::state(&w["value"])),
                    false,
                );
            }
        }
        assert_eq!(step["work"]["kind"], "audio");
        let inputs: Vec<Vec<i32>> =
            serde_json::from_value(step["work"]["value"]["inputs"].clone()).unwrap();
        for (port, data) in inputs.iter().enumerate() {
            for (i, &v) in data.iter().enumerate() {
                audio.sample_config(
                    s,
                    0,
                    port as u64 * audio.capacities[3] + i as u64,
                    Some(v),
                    false,
                );
            }
        }
        let mut emitted = Vec::new();
        let mut remaining = self.pending.clone();
        if !faulted {
            self.pipeline.store.memory.borrow_mut().control.stalls = self.stalls;
            s.model.set_all(&[
                ("arena_base", self.pipeline.store.base),
                ("run_buffers", audio.buffers),
                ("run_inputs", inputs.len() as u64),
                ("run_outputs", audio.outputs),
            ]);
            audio.clear_jobs();
            audio.program.run_inputs(s, &[]);
            let mut admitted = false;
            for _ in 0..1_000_000 {
                if s.tick(&["run_ready"]).0.get("run_ready") != 0 {
                    admitted = true;
                    break;
                }
            }
            assert!(admitted, "coordinator admission timeout");
            s.model.set("run_valid", 0);
            let mut finished = false;
            for _ in 0..1_000_000 {
                if s.model.get("done") != 0 {
                    finished = true;
                    break;
                }
                s.model
                    .set("job_enable", u64::from(!self.stalls || audio.random.yes(5)));
                s.idle(1);
            }
            assert!(finished, "coordinator completion timeout");
            s.model.set("job_enable", 1);
            if matches!(s.model.get("error_stage"), 2 | 3) {
                s.model.set("drain_ready", 1);
                s.until(2000, "failed job drain", |m| m.get("queued_jobs") == 0);
                s.model.set("drain_ready", 0);
                remaining = audio.drained();
                self.pending = remaining.clone();
            }
            emitted = audio.consumed();
            emitted.extend(audio.drained());
        }
        let stage = s.model.get("error_stage");
        let code = s.model.get("error_code");
        let error = match stage {
            0 => Value::Null,
            1 => json!({"stage":"transition","error":transition::ERRORS[code as usize]}),
            2 => {
                json!({"stage":"state","error":(["","queue_full","invalid_state"][code as usize])})
            }
            3 => {
                json!({"stage":"jobs","error":(["","invalid_buffer","invalid_shape","invalid_state","invalid_state","invalid_view","memory_full","invalid_state","memory_fault"][code as usize])})
            }
            _ => panic!("unknown coordinator stage {stage}"),
        };
        let state = audio.program.snapshot(s);
        let (buffers, outputs) = audio.read_samples(s);
        let transition =
            self.transition
                .snapshot_step(s, control, if stage == 1 { code } else { 0 });
        let media = if snapshot_media && !(stage == 3 && code == 8) {
            self.pipeline.store.snapshot(s)
        } else {
            Value::Null
        };
        let status = if error.is_null() {
            json!({"kind":"completed"})
        } else {
            json!({"kind":"failed","value":{"error":error["error"]}})
        };
        json!({"transition":transition,"jobs":emitted,"state":state,"error":error,"audio":{"jobs":emitted,"remaining_jobs":remaining,"state":state,"buffers":buffers,"outputs":outputs,"stage":"jobs","status":status,"media":media}})
    }
    pub fn collect_completed(&mut self, s: &mut Sim<'_>) -> Vec<Value> {
        self.transition
            .requests(s)
            .into_iter()
            .filter(|r| !r.is_null() && !r["outcome"].is_null())
            .map(|r| self.transition.collect(s, &r["id"]))
            .collect()
    }
    pub fn submit(&mut self, s: &mut Sim<'_>, command: &Value) -> (u64, Bits) {
        self.transition
            .command(s, 4, 0, &transition::command(command), Some(0), false)
    }
}
