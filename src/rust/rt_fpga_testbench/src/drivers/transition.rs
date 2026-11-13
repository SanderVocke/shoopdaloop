use crate::{bits::Bits, random::Random, sim::Sim};
use rt_fpga_adapter::{looping::TpReply, transition_service::TransitionService};
use serde_json::{Value, json};

pub const ERRORS: &[&str] = &[
    "",
    "invalid_config",
    "capacity",
    "invalid_id",
    "duplicate_id",
    "invalid_state",
    "invalid_frame",
    "missing_sync",
    "exhausted",
    "empty_content",
    "unsupported",
];
pub const MODES: &[&str] = &["stopped", "playing", "recording"];
const OPS: &[&str] = &["stop", "play", "record", "seek"];
pub fn request_service(image: &Value) -> crate::Result<TransitionService> {
    let image = serde_json::from_value(
        json!({"description":image["description"],"limits":image["limits"],"dependency_order":image["dependency_order"]}),
    )?;
    Ok(TransitionService::new(&image)?)
}
pub fn identity(v: &Value) -> u64 {
    v["slot"].as_u64().unwrap() | (v["generation"].as_u64().unwrap() << 32)
}
pub fn decode_identity(v: u64) -> Value {
    json!({"slot":v as u32,"generation":v>>32})
}
pub fn spec(v: &Value) -> Bits {
    let mut b = Bits::zero(640);
    if v.is_null() {
        return b;
    }
    b.set(0, 1, 1)
        .set(1, 32, v["id"]["generation"].as_u64().unwrap());
    if !v["sync_source"].is_null() {
        b.set(33, 1, 1).set(34, 64, identity(&v["sync_source"]));
    }
    b.set(98, 1, u64::from(v["repeat_sync"].as_bool().unwrap()))
        .set(99, 1, u64::from(v["kind"] == "composite"))
        .set(101, 1, u64::from(!v["timeline"].is_null()))
        .set(128, 64, v["length"].as_u64().unwrap());
    b
}
pub fn command(v: &Value) -> Bits {
    let mut b = Bits::zero(640);
    let t = &v["timing"];
    let op = &v["operation"];
    b.set(33, 1, u64::from(t["kind"] == "after_ticks"))
        .set(
            34,
            2,
            OPS.iter().position(|&k| op["kind"] == k).unwrap() as u64,
        )
        .set(36, 1, u64::from(op["retrigger"].as_bool().unwrap_or(false)))
        .set(64, 64, identity(&v["target"]))
        .set(128, 64, v["sequence"].as_u64().unwrap())
        .set(192, 64, v["at"].as_u64().unwrap());
    if !t["source"].is_null() {
        b.set(256, 64, identity(&t["source"]));
    }
    b.set(320, 64, t["skip"].as_u64().unwrap_or(0)).set(
        384,
        64,
        op["position"].as_u64().unwrap_or(0),
    );
    b
}
fn operation(code: u64, position: u64, retrigger: u64) -> Value {
    if code == 1 || code == 3 {
        json!({"kind":OPS[code as usize],"position":position,"retrigger":retrigger != 0})
    } else {
        json!({"kind":OPS[code as usize]})
    }
}
pub fn state(b: &Bits, slot: u64) -> Value {
    if b.get(0, 1) == 0 {
        return Value::Null;
    }
    let mut v = json!({"id":{"slot":slot,"generation":b.get(1,32)},"mode":MODES[b.get(33,2) as usize],"repeat_sync":b.get(35,1) != 0,"last_pulse":if b.get(36,1) != 0 {Some(b.get(512,64))} else {None}});
    for (i, key) in [
        "content_length",
        "position",
        "capture_frames",
        "cycles",
        "content_revision",
        "capture_revision",
        "revision_watermark",
    ]
    .iter()
    .enumerate()
    {
        v[key] = json!(b.get(64 * (i + 1), 64));
    }
    v
}
fn request(b: &Bits, slot: u64) -> Value {
    if b.get(0, 1) == 0 {
        return Value::Null;
    }
    let ticks = b.get(33, 1) != 0;
    let kind = b.get(37, 3);
    let timing = if ticks {
        json!({"kind":"after_ticks","source":decode_identity(b.get(256,64)),"skip":b.get(320,64)})
    } else {
        json!({"kind":"at"})
    };
    let mut outcome = if kind == 0 {
        Value::Null
    } else {
        json!({"kind":(["","applied","superseded","cancelled","failed"][kind as usize]),"frame":b.get(576,64)})
    };
    if kind == 4 {
        outcome["reason"] = json!(ERRORS[b.get(40, 4) as usize]);
    }
    json!({"id":{"slot":slot,"generation":b.get(1,32)},"command":{"target":decode_identity(b.get(64,64)),"sequence":b.get(128,64),"at":b.get(192,64),"timing":timing,"operation":operation(b.get(34,2),b.get(384,64),b.get(36,1))},"effective":b.get(448,64),"remaining_ticks":if ticks && kind == 0 {Some(b.get(512,64))} else {None},"outcome":outcome})
}
fn completion(b: &Bits, slot: u64) -> Value {
    if b.0.iter().all(|&n| n == 0) {
        return Value::Null;
    }
    let r = request(b, slot);
    assert!(!r.is_null() && !r["outcome"].is_null());
    json!({"id":r["id"],"sequence":r["command"]["sequence"],"requested":r["command"]["at"],"effective":r["effective"],"outcome":r["outcome"]})
}
fn event(b: &Bits) -> Value {
    let code = b.get(254, 2);
    let kind = if code == 0 {
        json!({"kind":"transition","value":operation(b.get(192,2),b.get(128,64),b.get(194,1))})
    } else {
        json!({"kind":(["transition","wrap","pulse"][code as usize])})
    };
    json!({"frame":b.get(0,64),"target":decode_identity(b.get(64,64)),"kind":kind})
}
pub fn response(error: u64, value: Value) -> Value {
    if error == 0 {
        json!({"kind":"accepted","value":value})
    } else {
        json!({"kind":"rejected","value":ERRORS[error as usize]})
    }
}

pub struct Transition {
    pub depths: [u64; 3],
    pub busy_signal: &'static str,
    pub ids: Vec<u64>,
    pub requests: u64,
    random: Random,
    pub last_cycles: u64,
    pub phase_cycles: Vec<u64>,
    pub service: Option<TransitionService>,
    pub service_timings: Option<Vec<Value>>,
}
impl Transition {
    pub fn new(depths: [u64; 3]) -> Self {
        Self {
            depths,
            busy_signal: "busy",
            ids: Vec::new(),
            requests: 0,
            random: Random::new(20261110),
            last_cycles: 0,
            phase_cycles: Vec::new(),
            service: None,
            service_timings: None,
        }
    }
    pub fn reset(&mut self, s: &mut Sim<'_>) {
        self.service = None;
        s.model.set_all(&[
            ("reset", 1),
            ("command_valid", 0),
            ("command_opcode", 0),
            ("command_index", 0),
            ("command_data", 0),
            ("response_ready", 0),
        ]);
        if s.model.has("loop_read_valid") {
            s.model
                .set_all(&[("loop_read_valid", 0), ("loop_read_slot", 0)]);
        }
        s.idle(3);
        assert_eq!(
            s.model.get("command_ready")
                | s.model.get("response_valid")
                | s.model.get(self.busy_signal),
            0
        );
        s.model.set("reset", 0);
        s.idle(1);
    }
    pub fn command(
        &mut self,
        s: &mut Sim<'_>,
        opcode: u64,
        index: u64,
        data: &Bits,
        stall: Option<u64>,
        intrude: bool,
    ) -> (u64, Bits) {
        if matches!(opcode, 4..=6 | 13) && self.service.is_some() {
            let mut service = self.service.take().unwrap();
            let mut words = [0; 20];
            let count = data.0.len().min(words.len());
            words[..count].copy_from_slice(&data.0[..count]);
            let start = self
                .service_timings
                .as_ref()
                .map(|_| std::time::Instant::now());
            let prepared = service.begin(opcode as u8, index as u32, words);
            if let Some(start) = start {
                let ns = start.elapsed().as_nanos() as u64;
                self.service_timings
                    .as_mut()
                    .unwrap()
                    .push(json!({"kind":"begin","ns":ns}));
            }
            let reply = match prepared {
                Err(error) => {
                    self.last_cycles = 0;
                    (u64::from(error), Bits::zero(832))
                }
                Ok(transfer) => {
                    let (error, value) = self.raw_command(
                        s,
                        u64::from(transfer.opcode),
                        u64::from(transfer.index),
                        &Bits(transfer.words.to_vec()),
                        stall,
                        intrude,
                    );
                    let mut words = [0; 26];
                    words.copy_from_slice(&value.0);
                    let start = self
                        .service_timings
                        .as_ref()
                        .map(|_| std::time::Instant::now());
                    let reply = service
                        .finish(TpReply {
                            error: error as u8,
                            words,
                        })
                        .expect("shared request service response");
                    if let Some(start) = start {
                        let ns = start.elapsed().as_nanos() as u64;
                        self.service_timings
                            .as_mut()
                            .unwrap()
                            .push(json!({"kind":"finish","ns":ns}));
                    }
                    (u64::from(reply.error), Bits(reply.words.to_vec()))
                }
            };
            self.service = Some(service);
            return reply;
        }
        let reply = self.raw_command(s, opcode, index, data, stall, intrude);
        if matches!(opcode, 0 | 9 | 11 | 16) {
            if let Some(service) = &mut self.service {
                let mut words = [0; 26];
                words.copy_from_slice(&reply.1.0);
                service.observe(
                    opcode as u8,
                    &TpReply {
                        error: reply.0 as u8,
                        words,
                    },
                );
            }
        }
        reply
    }
    pub fn raw_command(
        &mut self,
        s: &mut Sim<'_>,
        opcode: u64,
        index: u64,
        data: &Bits,
        stall: Option<u64>,
        intrude: bool,
    ) -> (u64, Bits) {
        s.model.eval();
        assert_ne!(
            s.model.get("command_ready"),
            0,
            "command failed to become ready"
        );
        s.model.set_all(&[
            ("command_valid", 1),
            ("command_opcode", opcode),
            ("command_index", index),
            ("response_ready", 0),
        ]);
        s.model.set_bits("command_data", data);
        s.idle(1);
        let accepted = s.cycles;
        assert_ne!(s.model.get(self.busy_signal), 0);
        assert_eq!(s.model.get("command_ready"), 0);
        s.model.set_all(&[
            ("command_valid", u64::from(intrude)),
            ("command_opcode", 255),
            ("command_index", u32::MAX as u64),
        ]);
        s.model.set_bits("command_data", &Bits(vec![u32::MAX; 20]));
        if intrude {
            s.idle(2);
            assert_eq!(s.model.get("command_ready"), 0);
            s.model.set("command_valid", 0);
        }
        s.until(10_000_000, "transition response", |m| {
            m.get("response_valid") != 0
        });
        self.last_cycles = s.cycles - accepted;
        assert_eq!(s.model.get(self.busy_signal), 0);
        let status = s.model.get("response_error");
        let value = s.model.bits("response_data");
        assert_eq!(s.model.get("command_ready"), 0);
        let stall = stall.unwrap_or_else(|| self.random.below(4));
        for _ in 0..stall {
            s.idle(1);
            assert_ne!(s.model.get("response_valid"), 0);
            assert_eq!(s.model.get("response_error"), status);
            assert_eq!(
                s.model.bits("response_data"),
                value,
                "response changed under backpressure"
            );
        }
        s.model.set("response_ready", 1);
        s.idle(1);
        s.model.set("response_ready", 0);
        assert_eq!(s.model.get("response_valid"), 0);
        assert_ne!(s.model.get("command_ready"), 0);
        if status != 0 {
            assert!(value.0.iter().all(|&n| n == 0), "error leaked stale data");
        }
        (status, value)
    }
    pub fn ok(&mut self, s: &mut Sim<'_>, opcode: u64, index: u64, data: &Bits) -> Bits {
        let (status, value) = self.command(s, opcode, index, data, None, false);
        assert_eq!(status, 0, "opcode {opcode}: {}", ERRORS[status as usize]);
        value
    }
    pub fn load(&mut self, s: &mut Sim<'_>, image: &Value) {
        for (i, key) in ["loop_slots", "pending_requests", "events"]
            .iter()
            .enumerate()
        {
            assert!(image["limits"][key].as_u64().unwrap() <= self.depths[i]);
        }
        self.requests = image["limits"]["pending_requests"].as_u64().unwrap();
        let loops = image["description"]["loops"].as_array().unwrap();
        self.ids = loops
            .iter()
            .map(|v| v["id"]["slot"].as_u64().unwrap())
            .collect();
        self.ids.sort();
        self.reset(s);
        for i in 0..image["limits"]["loop_slots"].as_u64().unwrap() {
            let v = loops
                .iter()
                .find(|v| v["id"]["slot"] == i)
                .unwrap_or(&Value::Null);
            self.ok(s, 1, i, &spec(v));
        }
        let order = image["dependency_order"].as_array().unwrap();
        for (i, id) in order.iter().enumerate() {
            self.ok(
                s,
                2,
                i as u64,
                &Bits::from_u64(id["slot"].as_u64().unwrap()),
            );
        }
        let data = Bits(vec![
            image["limits"]["loop_slots"].as_u64().unwrap() as u32,
            self.requests as u32,
            image["limits"]["events"].as_u64().unwrap() as u32,
            order.len() as u32,
            image["description"]["quantum"].as_u64().unwrap() as u32,
            rt_fpga_adapter::transition_service::wire::ABI,
        ]);
        self.ok(s, 3, 0, &data);
        self.service = Some(request_service(image).expect("prepared request service"));
    }
    pub fn status(&mut self, s: &mut Sim<'_>) -> Value {
        let b = self.ok(s, 0, 0, &Bits::default());
        json!({"frame":b.get(0,64),"fault":b.get(64,4),"phase":b.get(128,2),"configured":b.get(136,1) != 0,"events":b.get(192,32),"request_checks":b.get(416,32),"follower_visits":b.get(448,32),"pulse_deliveries":b.get(480,32)})
    }
    pub fn states(&mut self, s: &mut Sim<'_>) -> Vec<Value> {
        self.ids
            .clone()
            .iter()
            .map(|&id| state(&self.ok(s, 12, id, &Bits::default()), id))
            .collect()
    }
    pub fn requests(&mut self, s: &mut Sim<'_>) -> Vec<Value> {
        (0..self.requests)
            .map(|id| request(&self.ok(s, 13, id, &Bits::default()), id))
            .collect()
    }
    pub fn collect(&mut self, s: &mut Sim<'_>, id: &Value) -> Value {
        let (error, b) = self.command(
            s,
            6,
            id["slot"].as_u64().unwrap(),
            &Bits::from_u64(id["generation"].as_u64().unwrap()),
            None,
            false,
        );
        json!({"request":id,"result":response(error,if error == 0 { completion(&b,id["slot"].as_u64().unwrap()) } else {Value::Null})})
    }
    pub fn controls(&mut self, s: &mut Sim<'_>, step: &Value) -> Value {
        let frame = self.status(s)["frame"].clone();
        let mut collected = Vec::new();
        if step["collect_completed"].as_bool().unwrap() {
            for request in self.requests(s) {
                if !request.is_null() && !request["outcome"].is_null() {
                    collected.push(self.collect(s, &request["id"]));
                }
            }
        }
        for id in step["collect"].as_array().unwrap() {
            collected.push(self.collect(s, id));
        }
        let cancelled: Vec<_> = step["cancel"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                let (e, b) = self.command(
                    s,
                    5,
                    id["slot"].as_u64().unwrap(),
                    &Bits::from_u64(id["generation"].as_u64().unwrap()),
                    None,
                    false,
                );
                response(e, json!(b.get(0, 1) != 0))
            })
            .collect();
        let submitted: Vec<_> = step["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let (e, b) = self.command(s, 4, 0, &command(c), None, false);
                response(e, decode_identity(b.get(0, 64)))
            })
            .collect();
        json!({"frame":frame,"collected":collected,"cancelled":cancelled,"submitted":submitted})
    }
    pub fn events(&mut self, s: &mut Sim<'_>) -> Vec<Value> {
        let count = self.status(s)["events"].as_u64().unwrap();
        (0..count)
            .map(|i| event(&self.ok(s, 15, i, &Bits::default())))
            .collect()
    }
    pub fn snapshot_step(&mut self, s: &mut Sim<'_>, mut result: Value, error: u64) -> Value {
        result["processed"] = response(error, Value::Null);
        result["states"] = json!(self.states(s));
        result["requests"] = json!(self.requests(s));
        result["events"] = json!(self.events(s));
        result["next_frame"] = self.status(s)["frame"].clone();
        result
    }
    pub fn step(&mut self, s: &mut Sim<'_>, step: &Value) -> Value {
        let result = self.controls(s, step);
        let (error, _) = self.command(s, 9, 0, &Bits::default(), None, false);
        self.phase_cycles.push(self.last_cycles);
        self.snapshot_step(s, result, error)
    }
}
