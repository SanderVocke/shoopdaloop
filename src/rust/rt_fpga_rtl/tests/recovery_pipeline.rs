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
use std::cell::RefCell;

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
