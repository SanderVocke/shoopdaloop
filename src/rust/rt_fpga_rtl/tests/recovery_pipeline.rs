#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
#[path = "support/host_axi.rs"]
mod host_axi;
use host_axi::{Axi, build_pipeline, pipeline_contract};
use rt_fpga_rtl::{
    host::{Client, Error, Mailbox, RecoveryState, RegisterBus, Request, registers::*},
    pipeline::Geometry,
    sim::Sim,
};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};

#[shoop_test(no_wasm = "Native AXI/FPGA recovery integration")]
fn failed_mailbox_recovers_repeatedly_without_resetting_its_management_registers() {
    let limits = rt_fpga_compiler::ProgramLimits {
        instructions: 32,
        state_values: 16,
        loop_slots: 8,
        jobs: 32,
        buffers: 16,
        history_slots: 16,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    let geometry = Geometry::new([16, 256, 32], 32);
    let compiled = build_pipeline(&geometry, limits, 4096);
    let mut model = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    model.model.set_all(&[("reset", 1), ("epoch", 1)]);
    model.idle(2);
    model.model.set("reset", 0);
    model.idle(2);
    let sim = RefCell::new(model);
    let mut mailbox = Mailbox::open(Axi(&sim), pipeline_contract()).unwrap();
    let mut recovery = mailbox.recovery();
    assert!(recovery.supported().unwrap());
    for generation in 2..6 {
        if generation == 2 {
            let mut client = Client::new(recovery.reopen().unwrap());
            let mut budget = 1000;
            let reply = client
                .exchange(Request::begin_control(0), || {
                    budget -= 1;
                    budget > 0
                })
                .unwrap();
            assert_eq!(reply.code, RESULT_OK);
            sim.borrow_mut().idle(20_000);
        } else {
            mailbox.abort().unwrap();
        }
        assert_ne!(Axi(&sim).read(REG_STATUS).unwrap() & STATUS_FAULT, 0);
        let first = mailbox.fault_record().unwrap().unwrap();
        assert_eq!(first.epoch, generation - 1);
        assert_eq!(
            first.cause,
            if generation == 2 {
                FAULT_CONTROL_TIMEOUT
            } else {
                FAULT_ABORT
            }
        );
        if generation == 2 {
            assert_eq!(first.operation, OP_BEGIN_CONTROL);
            assert_eq!(first.control_cycles, 4095);
            mailbox.abort().unwrap();
            assert_eq!(mailbox.fault_record().unwrap(), Some(first));
        }
        recovery.begin().unwrap();
        let mut done = false;
        for _ in 0..1000 {
            match recovery.poll().unwrap() {
                RecoveryState::Ready(epoch) => {
                    assert_eq!(epoch, generation);
                    done = true;
                    break;
                }
                RecoveryState::Failed => panic!("unexpected unsafe reset"),
                _ => {}
            }
        }
        assert!(done, "reset deadline");
        assert_eq!(mailbox.metrics(), Err(Error::EpochChanged));
        assert_eq!(mailbox.abort(), Err(Error::EpochChanged));
        assert_eq!(
            Axi(&sim).write(REG_RECOVERY_REQUEST, generation - 1),
            Ok(())
        );
        assert_eq!(Axi(&sim).read(REG_RECOVERY_STATUS).unwrap(), 3);
        assert_eq!(Axi(&sim).read(REG_EPOCH).unwrap(), generation);
        assert_eq!(mailbox.fault_record().unwrap(), Some(first));
        mailbox = recovery.reopen().unwrap();
        assert_eq!(Axi(&sim).read(REG_STATUS).unwrap() & STATUS_FAULT, 0);
        let mut client = Client::new(recovery.reopen().unwrap());
        let mut budget = 1000;
        let reply = client
            .exchange(Request::capture_snapshot(0), || {
                budget -= 1;
                budget > 0
            })
            .unwrap();
        assert_eq!(reply.code, RESULT_OK);
        assert_eq!(reply.data[1..5], [0, 0, 0, 0]);
    }
}

struct Ack {
    after_status: bool,
    armed: bool,
    requested: bool,
    requests: usize,
}
#[derive(Clone)]
struct LostAck<B> {
    bus: B,
    ack: Rc<RefCell<Ack>>,
}
impl<B: RegisterBus<Error = u64>> RegisterBus for LostAck<B> {
    type Error = u64;
    fn read(&mut self, offset: u32) -> Result<u32, u64> {
        let value = self.bus.read(offset)?;
        let mut ack = self.ack.borrow_mut();
        if offset == REG_RECOVERY_STATUS && ack.after_status && ack.armed && ack.requested {
            ack.armed = false;
            return Err(99);
        }
        Ok(value)
    }
    fn write(&mut self, offset: u32, value: u32) -> Result<(), u64> {
        self.bus.write(offset, value)?;
        let mut ack = self.ack.borrow_mut();
        if offset == REG_RECOVERY_REQUEST {
            ack.requests += 1;
            ack.requested = true;
            if !ack.after_status && ack.armed {
                ack.armed = false;
                return Err(99);
            }
        }
        Ok(())
    }
}

#[shoop_test(no_wasm = "Native AXI/FPGA recovery integration")]
fn uncertain_software_acknowledgements_never_issue_a_second_hardware_recovery() {
    let limits = rt_fpga_compiler::ProgramLimits {
        instructions: 32,
        state_values: 16,
        loop_slots: 8,
        jobs: 32,
        buffers: 16,
        history_slots: 16,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    let geometry = Geometry::new([16, 256, 32], 32);
    let compiled = build_pipeline(&geometry, limits, 4096);
    for after_status in [false, true] {
        let mut model = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
        model.model.set_all(&[("reset", 1), ("epoch", 1)]);
        model.idle(2);
        model.model.set("reset", 0);
        model.idle(2);
        let sim = RefCell::new(model);
        let ack = Rc::new(RefCell::new(Ack {
            after_status,
            armed: true,
            requested: false,
            requests: 0,
        }));
        let mut mailbox = Mailbox::open(
            LostAck {
                bus: Axi(&sim),
                ack: ack.clone(),
            },
            pipeline_contract(),
        )
        .unwrap();
        let mut recovery = mailbox.recovery();
        assert_eq!(recovery.begin(), Err(Error::Bus(99)));
        sim.borrow_mut().idle(512);
        assert_eq!(Axi(&sim).read(REG_RECOVERY_STATUS), Ok(3));
        assert_eq!(Axi(&sim).read(REG_EPOCH), Ok(2));
        assert!(
            matches!(recovery.reopen(), Err(Error::Full)),
            "unqueried completion has no new mailbox authority"
        );
        assert_eq!(
            recovery.begin(),
            Err(Error::InvalidRequest),
            "do not reset epoch2 after losing epoch1's ACK"
        );
        let mut stale = mailbox.recovery();
        assert!(matches!(stale.reopen(), Err(Error::EpochChanged)));
        assert_eq!(stale.begin(), Err(Error::EpochChanged));
        assert_eq!(ack.borrow().requests, 1);
        assert_eq!(recovery.poll(), Ok(RecoveryState::Ready(2)));
        assert_eq!(mailbox.metrics(), Err(Error::EpochChanged));
        let mut fresh = recovery.reopen().unwrap();
        assert_eq!(fresh.metrics().unwrap().completed_quanta, 0);
        assert_eq!(Axi(&sim).read(REG_EPOCH), Ok(2));
        assert_eq!(ack.borrow().requests, 1);
    }
}
