pub use rt_fpga_testbench::timing::SharedPort as Port;

#[cfg(test)]
mod tests {
    use super::*;
    use rt_fpga_testbench::timing::{Fixed, Timing};
    use shoop_wasm_test_support::shoop_test;
    use std::{cell::RefCell, rc::Rc};

    #[shoop_test(no_wasm = "native RTL timing model")]
    fn ideal_service_is_ready_without_advancing_either_axi_port() {
        let controller: Rc<RefCell<Box<dyn Timing>>> =
            Rc::new(RefCell::new(Box::new(Fixed::new(0))));
        let mut audio = Port {
            controller: controller.clone(),
            advances_clock: true,
        };
        let mut midi = Port {
            controller,
            advances_clock: false,
        };
        let read = audio.submit(0x01000000, false).unwrap();
        let write = midi.submit(0x03000000, true).unwrap();
        assert_ne!(read, write);
        assert!(audio.ready(read) && midi.ready(write));
        assert_eq!(audio.report(false)["delay"], 0);
        audio.reset();
        assert!(!audio.ready(read) && !midi.ready(write));
    }

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
