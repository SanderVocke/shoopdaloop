use super::{contract, packet};
use rt_fpga_rtl::{
    host::{
        Error, RecoveryState, RegisterBus,
        semantic::{Device, MAX_PUBLICATION_BUS_OPS, PublicationStep},
        semantic_registers::*,
    },
    model::{Design, Probe},
    pipeline::{self, Geometry},
    sim::Sim,
};
use shoop_wasm_test_support::shoop_test;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[allow(dead_code)]
#[path = "host_axi.rs"]
mod host_axi;
use host_axi::Axi;

#[derive(Clone)]
struct Count<B> {
    bus: B,
    ops: Rc<Cell<usize>>,
    lose_recovery: Rc<Cell<bool>>,
    resets: Rc<Cell<usize>>,
}
impl<B: RegisterBus<Error = u64>> RegisterBus for Count<B> {
    type Error = u64;
    fn read(&mut self, address: u32) -> Result<u32, u64> {
        self.ops.set(self.ops.get() + 1);
        self.bus.read(address)
    }
    fn write(&mut self, address: u32, value: u32) -> Result<(), u64> {
        self.ops.set(self.ops.get() + 1);
        self.bus.write(address, value)?;
        if address == REG_RECOVERY_REQUEST {
            self.resets.set(self.resets.get() + 1);
            if self.lose_recovery.replace(false) {
                return Err(99);
            }
        }
        Ok(())
    }
}
fn design() -> Design {
    let root = rt_fpga_rtl::root();
    let geometry = Geometry::new([512, 31 * 1024 * 1024 / 32, 32], 32);
    let limits = rt_fpga_compiler::ProgramLimits {
        instructions: 1024,
        state_values: 512,
        loop_slots: 128,
        jobs: 1024,
        buffers: 512,
        history_slots: 512,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    let mut design = pipeline::design(&geometry, limits);
    design.top = "fifo_mmio_pipeline_axi".into();
    design.parameters.remove("LIVE_LOADING");
    design.parameters.remove("PROGRAM_REGIONS");
    design.probes = ["last_admitted", "head_sequence", "release_sequence"]
        .map(|name| {
            Probe::inject(&format!(
                "pipeline.execution.core.pipeline.consumer.banks.{name}"
            ))
        })
        .into();
    design
        .sources
        .insert(0, root.join("fpga/host_bridge/rtl/semantic_registers.sv"));
    for module in [
        "fifo_group_banks",
        "fifo_edit_kernel",
        "fifo_group_consumer",
        "fifo_observations",
        "fifo_snapshots",
        "fifo_audio_pipeline",
        "fifo_media_loader",
        "fifo_media_arbiter",
        "fifo_media_audio_pipeline",
        "host_recovery",
        "host_fault_record",
        "fifo_config_service",
        "fifo_mmio",
        "fifo_mmio_pipeline_axi",
    ] {
        design
            .sources
            .push(root.join(format!("fpga/host_bridge/rtl/{module}.sv")));
    }
    for module in [
        "fifo_media_audio_pipeline_axi",
        "fifo_recovery_pipeline_axi",
    ] {
        design
            .sources
            .push(root.join(format!("fpga/axi_memory/rtl/{module}.sv")));
    }
    design.memory.as_mut().unwrap().base = 0x3000_0000;
    design.parameters(&[
        ("LOOP_DEPTH", 128),
        ("EXTERNAL_IO", 1),
        ("PHYSICAL_BASE", 0x3000_0000),
    ])
}

#[shoop_test(no_wasm = "Actual full-profile semantic AXI/RTL transport integration")]
fn exact_profile_publication_uses_real_mmio_banks_and_typed_recovery_without_rendering() {
    let root = rt_fpga_rtl::root();
    let compiled = design()
        .build(&root.join("work/cmd-fifo-publication-axi"))
        .unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.max_ps = 10_000_000_000;
    sim.model.set_all(&[
        ("reset", 1),
        ("epoch", 1),
        ("run_quantum", 32),
        ("run_chunk_frames", 32),
        ("run_inputs", 1),
        ("run_outputs", 2),
        ("job_enable", 1),
        ("run_valid", 0),
    ]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(6000);
    let sim = RefCell::new(sim);
    let ops = Rc::new(Cell::new(0));
    let resets = Rc::new(Cell::new(0));
    let lose_recovery = Rc::new(Cell::new(false));
    let bus = Count {
        bus: Axi(&sim),
        ops: ops.clone(),
        lose_recovery: lose_recovery.clone(),
        resets: resets.clone(),
    };
    assert_eq!(Axi(&sim).read(REG_SNAPSHOT_READY), Ok(1));
    let mut device = Device::open(bus, contract()).unwrap();
    for (index, count) in [1, 16, 32, 2].into_iter().enumerate() {
        let sequence = index as u64 + 1;
        device.queue(packet(sequence, count)).unwrap();
        let mut complete = false;
        for step in 0..1000 {
            let before = ops.get();
            let result = device.publication_step().unwrap();
            assert!(ops.get() - before <= MAX_PUBLICATION_BUS_OPS);
            assert_eq!(sim.borrow().model.get("sync_frame"), 0);
            if step == 10 {
                sim.borrow_mut().idle(10_000);
                assert_eq!(sim.borrow().model.get("last_admitted"), sequence - 1);
                assert_eq!(sim.borrow().model.get("sync_frame"), 0);
            }
            if let PublicationStep::Published { bank, identity } = result {
                assert_eq!(bank, index);
                assert_eq!(identity.sequence(), sequence);
                assert_eq!(sim.borrow().model.get("last_admitted"), sequence);
                write_state_check(&sim, bank as u32);
                complete = true;
                break;
            }
        }
        assert!(complete);
    }
    assert_eq!(device.queue(packet(5, 1)), Err(Error::Full));
    assert_eq!(Axi(&sim).read(REG_COMPLETED_QUANTA), Ok(0));
    assert_eq!(Axi(&sim).read(REG_GROUPS_ATTEMPTED_LOW), Ok(0));
    let mut recovery = device.recovery();
    lose_recovery.set(true);
    assert_eq!(recovery.begin(), Err(Error::Bus(99)));
    assert_eq!(recovery.begin(), Err(Error::InvalidRequest));
    let mut ready = false;
    for _ in 0..1000 {
        match recovery.poll().unwrap() {
            RecoveryState::Ready(epoch) => {
                assert_eq!(epoch, 2);
                ready = true;
                break;
            }
            RecoveryState::Failed => panic!("unexpected retirement failure"),
            _ => {}
        }
    }
    assert!(ready);
    let fresh = recovery.reopen().unwrap();
    assert_eq!(fresh.epoch(), 2);
    assert_eq!(fresh.next_sequence(), 1);
    assert_eq!(resets.get(), 1);
    assert_eq!(Axi(&sim).read(REG_FREE_BANKS), Ok(15));
    assert_eq!(Axi(&sim).read(REG_COMPLETED_QUANTA), Ok(0));
    assert_eq!(sim.borrow().model.get("sync_frame"), 0);
}
fn write_state_check(sim: &RefCell<Sim<'_>>, bank: u32) {
    Axi(sim).write(REG_COMMAND_BANK, bank).unwrap();
    assert_eq!(Axi(sim).read(REG_COMMAND_BANK_STATE), Ok(2));
}
