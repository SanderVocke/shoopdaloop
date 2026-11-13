//! The CPU host bridge top driven over AXI-lite pins, as a `Bus` for the adapter's `Bridge`.
use crate::{SimResult, Simulator};
use marlin::verilator::{VerilatedModelConfig, VerilatorRuntime};
use marlin::verilog::prelude::*;
use rt_fpga_adapter::bridge::{Bridge, Bus};
use rt_fpga_adapter::port::PortIo;

#[verilog(src = "rtl/marlin_bridge.sv", name = "marlin_bridge")]
struct DirectTop;
#[verilog(src = "rtl/marlin_bridge.sv", name = "marlin_bridge_external")]
struct ExternalTop;

enum Top<'ctx> {
    Direct(DirectTop<'ctx>),
    External(ExternalTop<'ctx>),
}

macro_rules! pins {
    ($top:expr, |$model:ident| $body:expr) => {
        match $top {
            Top::Direct($model) => $body,
            Top::External($model) => $body,
        }
    };
}

/// Register accesses take a bounded number of clock cycles; exceeding it is a model bug.
const ACCESS_CYCLES: u32 = 64;

pub struct MarlinBus<'ctx> {
    top: Top<'ctx>,
    pub cycles: u64,
    pub reads: u64,
    pub writes: u64,
}

impl<'ctx> MarlinBus<'ctx> {
    /// Builds the bridge top; `external_audio` selects the core that sources its own samples.
    pub fn new(runtime: &'ctx VerilatorRuntime, external_audio: bool) -> SimResult<Self> {
        let config = VerilatedModelConfig::default();
        let top = if external_audio {
            Top::External(runtime.create_model::<ExternalTop>(&config)?)
        } else {
            Top::Direct(runtime.create_model::<DirectTop>(&config)?)
        };
        let mut bus = Self {
            top,
            cycles: 0,
            reads: 0,
            writes: 0,
        };
        pins!(&mut bus.top, |m| {
            m.s_axi_awvalid = 0;
            m.s_axi_wvalid = 0;
            m.s_axi_bready = 0;
            m.s_axi_arvalid = 0;
            m.s_axi_rready = 0;
            m.reset = 1;
        });
        bus.cycle();
        bus.cycle();
        pins!(&mut bus.top, |m| m.reset = 0);
        bus.cycle();
        Ok(bus)
    }
    fn cycle(&mut self) {
        pins!(&mut self.top, |m| {
            for clock in [0, 1, 0] {
                m.clk = clock;
                m.eval();
            }
        });
        self.cycles += 1;
    }
    fn until(&mut self, what: &str, mut ready: impl FnMut(&mut Top<'ctx>) -> bool) {
        for _ in 0..ACCESS_CYCLES {
            if ready(&mut self.top) {
                return;
            }
            self.cycle();
        }
        panic!("bridge AXI-lite {what} did not complete");
    }
}

impl Bus for MarlinBus<'_> {
    fn write(&mut self, offset: u32, value: u32) {
        pins!(&mut self.top, |m| {
            m.s_axi_awaddr = offset as u16;
            m.s_axi_wdata = value;
            m.s_axi_awvalid = 1;
            m.s_axi_wvalid = 1;
            m.s_axi_bready = 1;
        });
        self.until("write address", |top| {
            pins!(top, |m| {
                m.eval();
                m.s_axi_awready != 0 && m.s_axi_wready != 0
            })
        });
        self.cycle();
        pins!(&mut self.top, |m| {
            m.s_axi_awvalid = 0;
            m.s_axi_wvalid = 0;
        });
        self.until("write response", |top| pins!(top, |m| m.s_axi_bvalid != 0));
        self.cycle();
        pins!(&mut self.top, |m| m.s_axi_bready = 0);
        self.writes += 1;
    }
    fn read(&mut self, offset: u32) -> u32 {
        pins!(&mut self.top, |m| {
            m.s_axi_araddr = offset as u16;
            m.s_axi_arvalid = 1;
            m.s_axi_rready = 1;
        });
        self.until("read address", |top| {
            pins!(top, |m| {
                m.eval();
                m.s_axi_arready != 0
            })
        });
        self.cycle();
        pins!(&mut self.top, |m| m.s_axi_arvalid = 0);
        self.until("read data", |top| pins!(top, |m| m.s_axi_rvalid != 0));
        let data = pins!(&mut self.top, |m| m.s_axi_rdata);
        self.cycle();
        pins!(&mut self.top, |m| m.s_axi_rready = 0);
        self.reads += 1;
        data
    }
}

/// Device diagnostics shared by the pin-level simulator and the bridge.
pub trait Probe {
    fn cycles(&self) -> u64;
    fn last_error(&self) -> Option<&str>;
    /// The core's current audio frame.
    fn frame(&mut self) -> u64;
    /// Host register accesses so far; zero for transports without registers.
    fn bus_accesses(&self) -> u64 {
        0
    }
}

impl Probe for Simulator<'_> {
    fn cycles(&self) -> u64 {
        self.cycles
    }
    fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
    fn frame(&mut self) -> u64 {
        Simulator::frame(self)
    }
}

impl Probe for Bridge<MarlinBus<'_>> {
    fn cycles(&self) -> u64 {
        self.bus().cycles
    }
    fn bus_accesses(&self) -> u64 {
        self.bus().reads + self.bus().writes
    }
    fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
    fn frame(&mut self) -> u64 {
        let status = self.transition(0, 0, [0; 20]).expect("TP status row");
        rt_fpga_adapter::protocol::get(&status.words, 0, 64)
    }
}
