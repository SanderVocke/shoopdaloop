//! A queued ready/valid source, serviced independently of the scenario driver.
use crate::{
    bits::Bits,
    clock::Edges,
    model::Model,
    sim::{Agent, Sim},
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
pub struct Source {
    valid: String,
    ready: String,
    data: String,
    pub queue: VecDeque<Bits>,
    pub enabled: bool,
    pub sent: u64,
    pub stalls: u64,
}
impl Source {
    pub fn attach(
        s: &mut Sim<'_>,
        valid: &str,
        ready: &str,
        data: &str,
        queue: Vec<Bits>,
    ) -> Rc<RefCell<Self>> {
        let source = Rc::new(RefCell::new(Self {
            valid: valid.into(),
            ready: ready.into(),
            data: data.into(),
            queue: queue.into(),
            enabled: true,
            sent: 0,
            stalls: 0,
        }));
        s.agent(source.clone());
        source
    }
}
impl Agent for Source {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            m.set(
                &self.valid,
                u64::from(self.enabled && !self.queue.is_empty()),
            );
            if self.enabled
                && let Some(data) = self.queue.front()
            {
                m.set_bits(&self.data, data);
            }
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("reset") == 0 && self.enabled && !self.queue.is_empty() {
            if m.get(&self.ready) != 0 {
                self.queue.pop_front();
                self.sent += 1;
            } else {
                self.stalls += 1;
            }
        }
    }
}
pub fn control_packet(sequence: u64, kind: u64, data: &Bits, opcode: u64, index: u64) -> Bits {
    let mut packet = data.resized(640);
    packet
        .set(640, 32, index)
        .set(672, 8, opcode)
        .set(680, 2, kind)
        .set(682, 64, sequence);
    packet
}
