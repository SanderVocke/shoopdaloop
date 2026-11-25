use rt_fpga_rtl::{
    host::{Contract, RegisterBus},
    model::Compiled,
    pipeline::Geometry,
    sim::Sim,
};
use std::cell::RefCell;

#[derive(Clone, Copy)]
pub struct Axi<'a, 'model>(pub &'a RefCell<Sim<'model>>);

impl Axi<'_, '_> {
    pub fn write_word(&self, offset: u32, value: u32, strobes: u64) -> Result<(), u64> {
        let mut sim = self.0.borrow_mut();
        sim.model.set_all(&[
            ("s_axi_awaddr", u64::from(offset)),
            ("s_axi_wdata", u64::from(value)),
            ("s_axi_wstrb", strobes),
        ]);
        let mut channels = [
            ("s_axi_awvalid", "s_axi_awready"),
            ("s_axi_wvalid", "s_axi_wready"),
        ];
        if offset & 4 != 0 {
            channels.reverse();
        }
        for (valid, ready) in channels {
            sim.model.set(valid, 1);
            assert_eq!(sim.tick(&[ready]).0.get(ready), 1);
            sim.model.set(valid, 0);
        }
        sim.idle(1);
        assert_eq!(sim.model.get("s_axi_bvalid"), 1);
        assert_eq!(sim.model.get("s_axi_awready"), 0);
        assert_eq!(sim.model.get("s_axi_wready"), 0);
        let code = sim.model.get("s_axi_bresp");
        sim.idle(2);
        assert_eq!(sim.model.get("s_axi_bresp"), code);
        assert_eq!(sim.model.get("s_axi_bvalid"), 1);
        sim.model.set("s_axi_bready", 1);
        sim.idle(1);
        sim.model.set("s_axi_bready", 0);
        if code != 0 {
            return Err(code);
        }
        Ok(())
    }
}

impl RegisterBus for Axi<'_, '_> {
    type Error = u64;

    fn read(&mut self, offset: u32) -> Result<u32, u64> {
        let mut sim = self.0.borrow_mut();
        sim.model
            .set_all(&[("s_axi_arvalid", 1), ("s_axi_araddr", u64::from(offset))]);
        assert_eq!(sim.tick(&["s_axi_arready"]).0.get("s_axi_arready"), 1);
        sim.model.set("s_axi_arvalid", 0);
        for _ in 0..8 {
            if sim.model.get("s_axi_rvalid") != 0 {
                break;
            }
            sim.idle(1);
        }
        assert_eq!(sim.model.get("s_axi_rvalid"), 1);
        let data = sim.model.get("s_axi_rdata") as u32;
        let code = sim.model.get("s_axi_rresp");
        sim.idle(2);
        assert_eq!(sim.model.get("s_axi_rdata"), u64::from(data));
        assert_eq!(sim.model.get("s_axi_rvalid"), 1);
        sim.model.set("s_axi_rready", 1);
        sim.idle(1);
        sim.model.set("s_axi_rready", 0);
        if code != 0 {
            return Err(code);
        }
        Ok(data)
    }

    fn write(&mut self, offset: u32, value: u32) -> Result<(), u64> {
        self.write_word(offset, value, 0xf)
    }
}

pub fn build_pipeline(
    geometry: &Geometry,
    limits: rt_fpga_compiler::ProgramLimits,
    control_cycles: u64,
) -> Compiled {
    let mut design = rt_fpga_rtl::pipeline::design(geometry, limits);
    design.top = "host_pipeline_axi".into();
    design.probes.clear();
    design.parameters.remove("PHYSICAL_BASE");
    design = design.parameters(&[
        ("ARENA_BASE", 0x01000000),
        ("QUANTUM", u64::from(limits.quantum)),
        ("CONTROL_CYCLES", control_cycles),
    ]);
    let root = rt_fpga_rtl::root();
    for source in [
        "host_registers.sv",
        "host_mailbox.sv",
        "host_dispatch.sv",
        "host_recovery.sv",
        "host_pipeline_axi.sv",
    ] {
        design
            .sources
            .push(root.join("fpga/host_bridge/rtl").join(source));
    }
    design.build(&root.join("work/host-mailbox")).unwrap()
}

pub fn pipeline_contract() -> Contract {
    Contract {
        clock_hz: 100_000_000,
        arena_base: 0x01000000,
        arena_bytes: 0x08000000,
    }
}
