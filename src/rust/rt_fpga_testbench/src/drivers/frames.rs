//! Audio frame strobes shared by continuous and autonomous-loader benches.
use crate::{bits::Bits, sim::Sim};
pub struct Frames {
    pub ticks: u64,
    pub period_ps: u64,
}
impl Frames {
    pub fn pulse(&self, s: &mut Sim<'_>, input: Option<&Bits>) -> (u64, Bits) {
        self.pulse_observe(s, input, |_| ())
    }
    pub fn pulse_observe(
        &self,
        s: &mut Sim<'_>,
        input: Option<&Bits>,
        record: impl FnOnce(u64),
    ) -> (u64, Bits) {
        s.edge(1, false);
        s.model.set("frame_prepare", 1);
        s.edge(1, true);
        s.edge(1, false);
        s.model.set("frame_prepare", 0);
        s.edge(1, true);
        s.edge(1, false);
        if let Some(input) = input {
            s.model.set_bits("rx_samples", input);
        }
        s.model.set("frame_tick", 1);
        s.edge(1, true);
        let time = s.ps();
        let output = s.model.bits("tx_samples");
        record(time);
        s.edge(1, false);
        s.model.set("frame_tick", 0);
        for _ in 0..self.ticks - 3 {
            s.edge(1, true);
        }
        (time, output)
    }
    pub fn ns(&self, s: &mut Sim<'_>, ns: u64) {
        let until = s.ps() + ns * 1000;
        while s.ps() < until {
            s.step();
        }
    }
    pub fn settle(&self, s: &mut Sim<'_>, clocks: u64) {
        let until = s.ps() + clocks * self.period_ps;
        while s.ps() < until {
            s.step();
        }
    }
}
