use rt_fpga_rtl::{
    host::{Contract, Error, Mailbox, RegisterBus, registers::*},
    model::Design,
    sim::Sim,
};
use std::cell::RefCell;
#[allow(dead_code)]
#[path = "../../../../../src/rust/rt_fpga_rtl/tests/support/host_axi.rs"]
mod host_axi;

struct FaultAtPush<'a, 'b>(&'a RefCell<Sim<'b>>);
impl RegisterBus for FaultAtPush<'_, '_> {
    type Error = u64;
    fn read(&mut self, offset: u32) -> Result<u32, u64> {
        host_axi::Axi(self.0).read(offset)
    }
    fn write(&mut self, offset: u32, value: u32) -> Result<(), u64> {
        if offset == REG_REQUEST_PUSH {
            let mut sim = self.0.borrow_mut();
            sim.model.set("fault", 1);
            sim.idle(1);
        }
        host_axi::Axi(self.0).write(offset, value)
    }
}
fn main() {
    let root = rt_fpga_rtl::root();
    let rtl = root.join("fpga/host_bridge/rtl");
    let compiled = Design::new(
        "host_mailbox",
        vec![rtl.join("host_registers.sv"), rtl.join("host_mailbox.sv")],
    )
    .build(&root.join("work/restart-mmio-race"))
    .unwrap();
    let mut model = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    model.model.set_all(&[("reset", 1), ("epoch", 7)]);
    model.idle(2);
    model.model.set("reset", 0);
    model.idle(2);
    let sim = RefCell::new(model);
    let mut mailbox = Mailbox::open(
        FaultAtPush(&sim),
        Contract {
            clock_hz: 100_000_000,
            arena_base: 0x30000000,
            arena_bytes: 0x08000000,
        },
    )
    .unwrap();
    assert_eq!(mailbox.submit(1, 1, &[]), Err(Error::Bus(2)));
    assert_eq!(sim.borrow().model.get("command_valid"), 0);
    println!(
        "fault between request-space read and PUSH: AXI SLVERR (BRESP=2), not Error::Fault; no command executed"
    );
}
