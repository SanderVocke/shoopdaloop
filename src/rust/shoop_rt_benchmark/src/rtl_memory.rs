use rt_fpga_testbench::timing::{dram::Snapshot, Timing, Token};
use std::{cell::RefCell, rc::Rc};

pub struct Port {
    pub controller: Rc<RefCell<Box<dyn Timing>>>,
    pub advances_clock: bool,
}
impl Timing for Port {
    fn native_bytes(&self) -> u64 {
        self.controller.borrow().native_bytes()
    }
    fn advance(&mut self) {
        if self.advances_clock {
            self.controller.borrow_mut().advance();
        }
    }
    fn submit(&mut self, address: u64, write: bool) -> Option<Token> {
        self.controller.borrow_mut().submit(address, write)
    }
    fn ready(&self, token: Token) -> bool {
        self.controller.borrow().ready(token)
    }
    fn reset(&mut self) {
        if self.advances_clock {
            self.controller.borrow_mut().reset();
        }
    }
    fn report(&mut self, finalize: bool) -> serde_json::Value {
        self.controller.borrow_mut().report(finalize)
    }
    fn bus_snapshot(&self) -> Option<Snapshot> {
        self.controller.borrow().bus_snapshot()
    }
    fn bus_utilization(&self, start: Snapshot, end: Snapshot) -> Option<serde_json::Value> {
        self.controller.borrow().bus_utilization(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_fpga_testbench::timing::Fixed;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test(no_wasm = "native RTL timing model")]
    fn two_axi_ports_share_one_fabric_clock_and_token_namespace() {
        let controller: Rc<RefCell<Box<dyn Timing>>> =
            Rc::new(RefCell::new(Box::new(Fixed::new(7))));
        let mut audio = Port {
            controller: controller.clone(),
            advances_clock: true,
        };
        let mut midi = Port {
            controller,
            advances_clock: false,
        };
        let a = audio.submit(0x01000000, false).unwrap();
        let b = midi.submit(0x03000000, true).unwrap();
        assert_ne!(a, b);
        for _ in 0..6 {
            audio.advance();
            midi.advance();
            assert!(!audio.ready(a));
            assert!(!midi.ready(b));
        }
        audio.advance();
        midi.advance();
        assert!(midi.ready(a) && audio.ready(b));
        audio.reset();
        midi.reset();
        assert!(!audio.ready(a) && !midi.ready(b));
    }
}
