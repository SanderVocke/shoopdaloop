//! Composition of the sample/program driver and the public media-store driver.
use crate::{
    Result,
    drivers::{
        audio::Audio,
        store::{Geometry, Store},
    },
    memory::Mode,
    sim::Sim,
};
use serde_json::{Value, json};
pub struct Pipeline {
    pub audio: Audio,
    pub store: Store,
}
impl Pipeline {
    pub fn new(
        s: &mut Sim<'_>,
        audio: [u64; 7],
        media: Geometry,
        mode: Mode,
        seed: u64,
    ) -> Result<Self> {
        let audio = Audio::new(s, audio, seed);
        let store = Store::new(s, media, mode, seed + 17, "store_")?;
        s.agent(crate::protocol::Exclusion::new(
            "busy",
            &["store_cmd_ready", "store_cfg_ready"],
        ));
        Ok(Self { audio, store })
    }
    pub fn load(&mut self, s: &mut Sim<'_>, case: &Value) {
        self.store.set(
            s,
            &[
                ("cmd_valid", 0),
                ("cfg_valid", 0),
                ("write_valid", 0),
                ("read_ready", 0),
            ],
        );
        self.audio.load(s, case);
        self.store.load(s, &case["media"]);
    }
    pub fn quantum(&mut self, s: &mut Sim<'_>, step: &Value, stalls: bool) -> Value {
        self.store.memory.borrow_mut().control.stalls = stalls;
        if let Some(actions) = step["media_actions"].as_array() {
            for action in actions {
                let r = self.store.command(s, action, stalls, true, true);
                if r["status"]["kind"] != "completed" {
                    let (buffers, outputs) = self.audio.read_samples(s);
                    return json!({"jobs":[],"remaining_jobs":[],"state":self.audio.program.snapshot(s),"buffers":buffers,"outputs":outputs,"stage":"media","status":r["status"],"media":r["media"]});
                }
            }
        }
        let mut r = self.audio.quantum(s, step, stalls);
        r["media"] = self.store.snapshot(s);
        r
    }
}
