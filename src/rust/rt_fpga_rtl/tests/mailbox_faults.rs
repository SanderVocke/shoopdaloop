#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{
    host::{Contract, Error, Mailbox, RegisterBus, registers::*},
    model::{Compiled, Design},
    sim::Sim,
};
use shoop_wasm_test_support::shoop_test;
use std::cell::RefCell;
#[allow(dead_code)]
#[path = "support/host_axi.rs"]
mod host_axi;
use host_axi::Axi;

fn design() -> Compiled {
    let root = rt_fpga_rtl::root();
    let rtl = root.join("fpga/host_bridge/rtl");
    Design::new(
        "host_mailbox",
        vec![rtl.join("host_registers.sv"), rtl.join("host_mailbox.sv")],
    )
    .build(&root.join("work/host-mailbox"))
    .unwrap()
}
fn model(compiled: &Compiled) -> Sim<'_> {
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.model.set_all(&[("reset", 1), ("epoch", 7)]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(2);
    sim
}
fn contract() -> Contract {
    Contract {
        clock_hz: 100_000_000,
        arena_base: 0x30000000,
        arena_bytes: 0x08000000,
    }
}

struct Race<'a, 'b> {
    sim: &'a RefCell<Sim<'b>>,
    offset: u32,
    epoch: bool,
}
impl RegisterBus for Race<'_, '_> {
    type Error = u64;
    fn read(&mut self, offset: u32) -> Result<u32, u64> {
        Axi(self.sim).read(offset)
    }
    fn write(&mut self, offset: u32, value: u32) -> Result<(), u64> {
        if offset == self.offset {
            let mut sim = self.sim.borrow_mut();
            sim.model.set(
                if self.epoch { "epoch" } else { "fault" },
                if self.epoch { 8 } else { 1 },
            );
            sim.idle(2);
        }
        Axi(self.sim).write(offset, value)
    }
}

struct ReplaceRecord<'a, 'b>(&'a RefCell<Sim<'b>>);
impl RegisterBus for ReplaceRecord<'_, '_> {
    type Error = u64;
    fn read(&mut self, offset: u32) -> Result<u32, u64> {
        if offset == REG_FAULT_TAG {
            let mut sim = self.0.borrow_mut();
            sim.model.set_all(&[
                ("epoch", 8),
                ("fault_epoch", 8),
                ("fault_cause", u64::from(FAULT_AUDIO)),
            ]);
            sim.idle(2);
        }
        Axi(self.0).read(offset)
    }
    fn write(&mut self, offset: u32, value: u32) -> Result<(), u64> {
        Axi(self.0).write(offset, value)
    }
}

#[shoop_test(no_wasm = "Native fault-record coherent-read regression")]
fn fault_record_reader_rejects_replacement_during_a_read() {
    let compiled = design();
    let sim = RefCell::new(model(&compiled));
    sim.borrow_mut().model.set_all(&[
        ("fault", 1),
        ("fault_epoch", 7),
        ("fault_cause", u64::from(FAULT_MEMORY)),
    ]);
    sim.borrow_mut().idle(2);
    let mut mailbox = Mailbox::open(ReplaceRecord(&sim), contract()).unwrap();
    assert_eq!(mailbox.fault_record(), Err(Error::EpochChanged));
    assert_eq!(Axi(&sim).read(REG_FAULT_EPOCH), Ok(8));
}

#[shoop_test(no_wasm = "Native retained mailbox fault record")]
fn retained_fault_is_immutable_per_epoch_and_survives_datapath_epoch_zero() {
    let compiled = design();
    let sim = RefCell::new(model(&compiled));
    let mut mailbox = Mailbox::open(Axi(&sim), contract()).unwrap();
    assert_eq!(mailbox.fault_record().unwrap(), None);
    {
        let mut sim = sim.borrow_mut();
        sim.model.set_all(&[
            ("fault", 1),
            ("fault_epoch", 7),
            ("fault_cause", u64::from(FAULT_CONTROL_TIMEOUT)),
            ("fault_tag", 19),
            ("fault_op", u64::from(OP_CORE_CONFIG)),
            ("fault_cycles", 200),
            ("fault_control_cycles", 16665),
            ("fault_detail", 12),
            ("fault_revision", 0x1234567887654321),
            ("fault_frame", 0x23456789abcdef01),
        ]);
        sim.idle(2);
    }
    let first = mailbox.fault_record().unwrap().unwrap();
    assert_eq!(
        first,
        rt_fpga_rtl::host::FaultRecord {
            epoch: 7,
            cause: FAULT_CONTROL_TIMEOUT,
            tag: 19,
            operation: OP_CORE_CONFIG,
            cycles: 200,
            control_cycles: 16665,
            detail: 12,
            revision: 0x1234567887654321,
            frame: 0x23456789abcdef01
        }
    );
    Axi(&sim).write(REG_ABORT, 7).unwrap();
    sim.borrow_mut()
        .model
        .set("fault_cause", u64::from(FAULT_ABORT));
    sim.borrow_mut().idle(2);
    assert_eq!(mailbox.fault_record().unwrap(), Some(first));
    sim.borrow_mut()
        .model
        .set_all(&[("epoch", 0), ("fault", 0)]);
    sim.borrow_mut().idle(2);
    assert_eq!(mailbox.fault_record().unwrap(), Some(first));
    sim.borrow_mut().model.set("epoch", 8);
    sim.borrow_mut().idle(2);
    assert_eq!(mailbox.fault_record().unwrap(), Some(first));
    sim.borrow_mut().model.set_all(&[
        ("fault", 1),
        ("fault_epoch", 8),
        ("fault_cause", u64::from(FAULT_MEMORY)),
    ]);
    sim.borrow_mut().idle(2);
    let next = mailbox.fault_record().unwrap().unwrap();
    assert_eq!((next.epoch, next.cause), (8, FAULT_MEMORY));
    sim.borrow_mut()
        .model
        .set_all(&[("reset", 1), ("fault", 0)]);
    sim.borrow_mut().idle(1);
    sim.borrow_mut().model.set("reset", 0);
    sim.borrow_mut().idle(2);
    assert_eq!(mailbox.fault_record().unwrap(), None);
}

#[shoop_test(no_wasm = "Native AXI acknowledgement retirement races")]
fn epoch_change_during_acknowledgements_is_a_driver_error_not_a_bus_abort() {
    let compiled = design();
    for offset in [REG_RESPONSE_POP, REG_EVENT_POP, REG_ABORT] {
        let sim = RefCell::new(model(&compiled));
        let mut mailbox = Mailbox::open(
            Race {
                sim: &sim,
                offset,
                epoch: true,
            },
            contract(),
        )
        .unwrap();
        if offset == REG_RESPONSE_POP {
            sim.borrow_mut().model.set_all(&[
                ("response_valid", 1),
                ("response_epoch", 7),
                ("response_tag", 33),
            ]);
            sim.borrow_mut().idle(1);
            sim.borrow_mut().model.set("response_valid", 0);
            assert_eq!(mailbox.response(), Err(Error::EpochChanged));
        } else if offset == REG_EVENT_POP {
            sim.borrow_mut().model.set("sync_event_valid", 1);
            sim.borrow_mut().idle(1);
            sim.borrow_mut().model.set("sync_event_valid", 0);
            assert_eq!(mailbox.sync_event(), Err(Error::EpochChanged));
        } else {
            assert_eq!(mailbox.abort(), Err(Error::EpochChanged));
            assert_eq!(sim.borrow().model.get("host_abort"), 0);
        }
    }
}

#[shoop_test(no_wasm = "Native AXI fault-race simulation")]
fn asynchronous_faults_and_epoch_changes_reject_submission_without_bus_errors() {
    let compiled = design();
    for epoch in [false, true] {
        for offset in std::iter::once(REG_REQUEST_EPOCH)
            .chain(std::iter::once(REG_REQUEST_OP))
            .chain((0..HOST_PAYLOAD_WORDS).map(|i| REG_REQUEST_DATA + 4 * i as u32))
            .chain(std::iter::once(REG_REQUEST_PUSH))
        {
            let sim = RefCell::new(model(&compiled));
            let mut mailbox = Mailbox::open(
                Race {
                    sim: &sim,
                    offset,
                    epoch,
                },
                contract(),
            )
            .unwrap();
            assert_eq!(
                mailbox.submit(12, OP_BEGIN_CONTROL, &[]),
                Err(if epoch {
                    Error::EpochChanged
                } else {
                    Error::Fault
                }),
                "offset={offset:x}, epoch={epoch}"
            );
            assert_eq!(sim.borrow().model.get("command_valid"), 0);
            assert_eq!(
                Axi(&sim).read(REG_SUBMISSION_RESULT),
                Ok(if epoch { SUBMIT_EPOCH } else { SUBMIT_FAULT })
            );
        }
    }
}

#[shoop_test(no_wasm = "Native AXI write-channel and backpressure simulation")]
fn fault_at_each_push_handshake_never_produces_slverr_or_executes_a_rejected_command() {
    let compiled = design();
    for first_address in [true, false] {
        for fault_at in 0..7 {
            let sim = RefCell::new(model(&compiled));
            let mut bus = Axi(&sim);
            sim.borrow_mut().model.set("command_ready", 1);
            bus.write(REG_REQUEST_EPOCH, 7).unwrap();
            bus.write(REG_REQUEST_OP, OP_BEGIN_CONTROL).unwrap();
            {
                let mut sim = sim.borrow_mut();
                sim.model.set_all(&[
                    ("s_axi_awaddr", u64::from(REG_REQUEST_PUSH)),
                    ("s_axi_wdata", 9),
                    ("s_axi_wstrb", 15),
                ]);
                for cycle in 0..12 {
                    if cycle == fault_at {
                        sim.model.set("fault", 1);
                    }
                    let aw = if first_address { 0 } else { 2 };
                    let w = if first_address { 2 } else { 0 };
                    sim.model.set("s_axi_awvalid", u64::from(cycle == aw));
                    sim.model.set("s_axi_wvalid", u64::from(cycle == w));
                    sim.idle(1);
                }
                assert_eq!(sim.model.get("s_axi_bvalid"), 1);
                assert_eq!(sim.model.get("s_axi_bresp"), 0);
                assert_eq!(sim.model.get("command_valid"), 0);
                sim.model.set("s_axi_bready", 1);
                sim.idle(1);
                sim.model.set("s_axi_bready", 0);
            }
            let result = bus.read(REG_SUBMISSION_RESULT).unwrap();
            assert!(matches!(result, SUBMIT_ACCEPTED | SUBMIT_FAULT));
            assert_eq!(
                sim.borrow().model.get("command_tag"),
                if result == SUBMIT_FAULT { 0 } else { 9 }
            );
            assert_ne!(bus.read(REG_STATUS).unwrap() & STATUS_FAULT, 0);
        }
    }
}
