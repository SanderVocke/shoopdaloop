//! Native simulator only: the processing and memory service remain RTL.
use marlin::verilator::{
    VerilatedModelConfig, VerilatorRuntime, VerilatorRuntimeOptions, WideIn,
    tracing::{TraceFile, Waveform},
};
use marlin::verilog::prelude::*;
use rt_fpga_adapter::image::Image;
use rt_fpga_adapter::looping::{MediaCommand, TpReply};
use rt_fpga_adapter::port::{self, Capacities, PortIo, Run, Status, StoreReply};
pub use rt_fpga_adapter::port::{instruction_words, value_words};
pub use rt_fpga_adapter::protocol::{get, put};
use std::path::{Path, PathBuf};

pub type SimResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[verilog(src = "rtl/marlin_pipeline.sv", name = "marlin_pipeline")]
struct Pipeline;

pub fn runtime(artifacts: &Path) -> SimResult<VerilatorRuntime> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut sources = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("rtl/marlin_pipeline.sv")];
    for (directory, names) in [
        (
            "state_processor",
            "sp_ram sp_job_fifo sp_engine state_processor",
        ),
        (
            "job_processor",
            "jp_sample_ram jp_divider job_processor audio_pipeline",
        ),
        (
            "transition_processor",
            "tp_ram tp_priority transition_processor",
        ),
        ("chunk_memory", "chunk_ram chunk_memory"),
        (
            "media_store",
            "media_ram metadata_cache media_metadata media_reclaimer media_prefetch_resolver media_store memory_delay media_audio_pipeline",
        ),
        ("loop_pipeline", "loop_pipeline"),
        (
            "axi_memory",
            "axi_master axi_burst_mux shared_axi chunk_dma word_dma word_axi chunk_axi loop_pipeline_axi",
        ),
    ] {
        sources.extend(
            names
                .split_whitespace()
                .map(|name| root.join(format!("fpga/{directory}/rtl/{name}.sv"))),
        );
    }
    sources.push(root.join("fpga/axi_memory/tests/axi_direct_memory.sv"));
    sources.push(root.join("fpga/host_bridge/rtl/host_core.sv"));
    sources.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("rtl/marlin_bridge.sv"));
    Ok(VerilatorRuntime::new2(
        artifacts,
        &sources,
        &[
            root.join("fpga/state_processor/rtl"),
            root.join("fpga/host_bridge/rtl"),
        ],
        [],
        VerilatorRuntimeOptions::default().verilator_executable(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("verilator-wrapper")
                .into(),
        ),
    )?)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoreUsage {
    pub issued_views: u32,
    pub high_water_slots: u32,
    pub free_slots: u32,
    pub slot_limit: u32,
}
impl StoreUsage {
    pub fn live_slots(self) -> u32 {
        self.high_water_slots - self.free_slots
    }
}
/// Reads the media store's slot and view counters through any port.
pub fn store_usage<P: PortIo + ?Sized>(port: &mut P) -> SimResult<StoreUsage> {
    let mut values = [0; 4];
    for (address, value) in values.iter_mut().enumerate() {
        *value = port.store_configure(0, address as u32, None)?[0];
    }
    Ok(StoreUsage {
        issued_views: values[0],
        high_water_slots: values[1],
        free_slots: values[2],
        slot_limit: values[3],
    })
}
pub struct Simulator<'ctx> {
    // Close the trace while its model is still alive (fields drop in order).
    trace: Option<TraceFile<'ctx>>,
    model: Pipeline<'ctx>,
    pub cycles: u64,
    media_slots: u32,
    pub stalls: bool,
    pub timeout: u64,
    pub last_error: Option<String>,
}
impl<'ctx> Simulator<'ctx> {
    pub fn new(
        runtime: &'ctx VerilatorRuntime,
        wave: Option<&Path>,
        stalls: bool,
    ) -> SimResult<Self> {
        let mut model = runtime.create_model::<Pipeline>(
            &VerilatedModelConfig::default().enable_tracing(Some(Waveform::Vcd)),
        )?;
        let trace = wave.map(|path| model.open_trace(path));
        let mut sim = Self {
            model,
            trace,
            cycles: 0,
            media_slots: 128,
            stalls,
            timeout: 100_000,
            last_error: None,
        };
        sim.reset();
        Ok(sim)
    }
    pub fn store_usage(&mut self) -> SimResult<StoreUsage> {
        store_usage(self)
    }
    pub fn arena_capacity(&mut self, slots: u32) -> SimResult<()> {
        if slots == 0 || slots > 32768 {
            return Err("invalid simulation arena capacity".into());
        }
        self.media_slots = slots;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.last_error = None;
        self.model.run_valid = 0;
        self.model.cfg_valid = 0;
        self.model.audio_cfg_valid = 0;
        self.model.command_valid = 0;
        self.model.response_ready = 0;
        self.model.store_cmd_valid = 0;
        self.model.store_cfg_valid = 0;
        self.model.store_write_valid = 0;
        self.model.store_read_ready = 0;
        self.model.sim_control = 0;
        self.model.sim_image_request = 0;
        self.model.sim_fail_token = 0;
        self.model.job_enable = 1;
        self.model.drain_ready = 0;
        self.model.arena_base = 0x01000000;
        self.model.reset = 1;
        self.cycle();
        self.cycle();
        self.model.reset = 0;
        self.cycle();
    }
    pub fn cycle(&mut self) {
        for (offset, clock) in [(0, 0), (5, 1), (9, 0)] {
            self.model.clk = clock;
            self.model.eval();
            if let Some(trace) = &mut self.trace {
                trace.dump(self.cycles * 10 + offset);
            }
        }
        self.cycles += 1;
    }
    pub fn frame(&self) -> u64 {
        self.model.debug_frame
    }
    pub fn command(&mut self, opcode: u8, index: u32, data: [u32; 20]) -> SimResult<[u32; 26]> {
        let (error, value) = self.command_raw(opcode, index, data)?;
        if error != 0 {
            return Err(format!("TP command {opcode} error {error}").into());
        }
        Ok(value)
    }
    pub fn command_raw(
        &mut self,
        opcode: u8,
        index: u32,
        data: [u32; 20],
    ) -> SimResult<(u8, [u32; 26])> {
        self.model.command_opcode = opcode;
        self.model.command_index = index;
        self.model.command_data = WideIn::new(data);
        for _ in 0..self.timeout {
            self.model.eval();
            if self.model.command_ready != 0 {
                break;
            }
            self.cycle();
        }
        if self.model.command_ready == 0 {
            return Err("TP command ready timeout".into());
        }
        self.model.command_valid = 1;
        self.cycle();
        self.model.command_valid = 0;
        for _ in 0..self.timeout {
            if self.model.response_valid != 0 {
                break;
            }
            self.cycle();
        }
        if self.model.response_valid == 0 {
            return Err("TP response timeout".into());
        }
        let value = *self.model.response_data.value();
        let error = self.model.response_error;
        for _ in 0..if self.stalls { 3 } else { 0 } {
            self.cycle();
            if self.model.response_valid == 0
                || *self.model.response_data.value() != value
                || self.model.response_error != error
            {
                return Err("TP response changed while stalled".into());
            }
        }
        self.model.response_ready = 1;
        self.cycle();
        self.model.response_ready = 0;
        Ok((error, value))
    }
    pub fn state(
        &mut self,
        address: u16,
        data: Option<[u32; 5]>,
        program: bool,
    ) -> SimResult<[u32; 5]> {
        self.model.eval();
        if self.model.cfg_ready == 0 {
            return Err("configuration port busy".into());
        }
        if self.stalls {
            self.cycle();
        }
        self.model.cfg_state = u8::from(!program);
        self.model.cfg_write = u8::from(data.is_some());
        self.model.cfg_address = address;
        self.model.cfg_write_data = WideIn::new(data.unwrap_or_default());
        self.model.cfg_valid = 1;
        self.cycle();
        self.model.cfg_valid = 0;
        if self.model.cfg_response_valid == 0 || self.model.cfg_response_error != 0 {
            return Err("configuration rejected".into());
        }
        let result = *self.model.cfg_read_data.value();
        self.cycle();
        Ok(result)
    }
    pub fn sample(&mut self, space: u8, address: u32, value: Option<i32>) -> SimResult<i32> {
        self.model.eval();
        if self.model.audio_cfg_ready == 0 {
            return Err("audio port busy".into());
        }
        if self.stalls {
            self.cycle();
        }
        self.model.audio_cfg_space = space;
        self.model.audio_cfg_address = address;
        self.model.audio_cfg_write = u8::from(value.is_some());
        self.model.audio_cfg_write_data = value.unwrap_or_default() as u32;
        self.model.audio_cfg_valid = 1;
        self.cycle();
        self.model.audio_cfg_valid = 0;
        if self.model.audio_cfg_response_valid == 0 || self.model.audio_cfg_response_error != 0 {
            return Err("audio configuration rejected".into());
        }
        let value = self.model.audio_cfg_read_data as i32;
        self.cycle();
        Ok(value)
    }
    pub fn load(&mut self, image: &Image) -> SimResult<()> {
        Ok(port::load(self, image)?)
    }
    pub fn store_config(
        &mut self,
        space: u8,
        address: u32,
        value: Option<[u32; 5]>,
    ) -> SimResult<[u32; 5]> {
        self.model.store_cfg_space = space;
        self.model.store_cfg_address = address;
        self.model.store_cfg_write = u8::from(value.is_some());
        self.model.store_cfg_write_data = WideIn::new(value.unwrap_or_default());
        self.model.store_cfg_valid = 1;
        let mut accepted = false;
        for _ in 0..self.timeout {
            self.model.eval();
            let ready = self.model.store_cfg_ready != 0;
            self.cycle();
            if ready {
                accepted = true;
                break;
            }
        }
        self.model.store_cfg_valid = 0;
        if !accepted {
            return Err("media configuration admission timeout".into());
        }
        for _ in 0..self.timeout {
            if self.model.store_cfg_response_valid != 0 {
                break;
            }
            self.cycle();
        }
        if self.model.store_cfg_response_valid == 0 || self.model.store_cfg_response_error != 0 {
            return Err("media configuration response failed".into());
        }
        let words = *self.model.store_cfg_read_data.value();
        self.cycle();
        Ok(words)
    }
    pub fn append_view(&mut self, view: u32, samples: &[i32]) -> SimResult<()> {
        let command = MediaCommand {
            opcode: 5,
            view,
            start: 0,
            length: 0,
            count: u32::try_from(samples.len())?,
        };
        let reply = self.store_run(command, samples)?;
        if reply.code != 0 {
            return Err(format!(
                "append error {} after {} samples",
                reply.code, reply.written
            )
            .into());
        }
        if reply.written != samples.len() {
            return Err("append completed before all samples".into());
        }
        Ok(())
    }
    pub fn read_view(&mut self, view: u64, frames: u32) -> SimResult<Vec<i32>> {
        self.model.eval();
        if self.model.store_cmd_ready == 0 {
            return Err("media command busy".into());
        }
        self.model.store_cmd_opcode = 6;
        self.model.store_cmd_view = view;
        self.model.store_cmd_start = 0;
        self.model.store_cmd_length = u64::from(frames);
        self.model.store_cmd_count = frames;
        self.model.store_cmd_valid = 1;
        self.cycle();
        self.model.store_cmd_valid = 0;
        let mut samples = vec![];
        for _ in 0..self.timeout {
            if self.model.store_done != 0 {
                break;
            }
            self.model.store_read_ready = u8::from(!self.stalls || self.cycles % 4 != 0);
            self.model.eval();
            if self.model.store_read_valid != 0 && self.model.store_read_ready != 0 {
                samples.push(self.model.store_read_data as i32);
            }
            self.cycle();
        }
        self.model.store_read_ready = 0;
        if self.model.store_done == 0
            || self.model.store_error_code != 0
            || samples.len() != frames as usize
        {
            return Err(format!(
                "media read view {view}: done {}, code {}, received {} of {frames}",
                self.model.store_done,
                self.model.store_error_code,
                samples.len()
            )
            .into());
        }
        self.cycle();
        Ok(samples)
    }
    fn maintenance_budget(&self, chunks: u64) -> u64 {
        if self.timeout == 0 {
            0
        } else {
            self.timeout.saturating_add(chunks.saturating_mul(1024))
        }
    }
    pub fn run(&mut self, image: &Image, inputs: &[Vec<i32>]) -> SimResult<Vec<Vec<i32>>> {
        Ok(port::process(self, image, inputs)?)
    }
    fn store_run(&mut self, command: MediaCommand, write: &[i32]) -> SimResult<StoreReply> {
        self.model.store_cmd_opcode = command.opcode;
        self.model.store_cmd_view = u64::from(command.view);
        self.model.store_cmd_start = command.start;
        self.model.store_cmd_length = command.length;
        self.model.store_cmd_count = command.count;
        self.model.store_cmd_valid = 1;
        let mut accepted = false;
        for _ in 0..self.timeout {
            self.model.eval();
            let ready = self.model.store_cmd_ready != 0;
            self.cycle();
            if ready {
                accepted = true;
                break;
            }
        }
        self.model.store_cmd_valid = 0;
        if !accepted {
            return Err("media admission timeout".into());
        }
        let append = command.opcode == 5;
        let mut reply = StoreReply::default();
        for _ in 0..self.maintenance_budget(if command.opcode == 1 {
            command.length.div_ceil(32)
        } else {
            u64::from(self.media_slots)
        }) {
            if self.model.store_done != 0 {
                break;
            }
            if append {
                self.model.store_write_valid = u8::from(reply.written < write.len());
                self.model.store_write_data = write.get(reply.written).copied().unwrap_or(0) as u32;
                self.model.eval();
                let accepted =
                    self.model.store_write_valid != 0 && self.model.store_write_ready != 0;
                self.cycle();
                if accepted {
                    reply.written += 1;
                }
            } else {
                self.model.store_read_ready = u8::from(!self.stalls || self.cycles % 4 != 0);
                self.model.eval();
                if self.model.store_read_valid != 0 && self.model.store_read_ready != 0 {
                    reply.result.samples.push(self.model.store_read_data as i32);
                }
                self.cycle();
            }
        }
        self.model.store_write_valid = 0;
        self.model.store_read_ready = 0;
        if self.model.store_done == 0 {
            return Err(format!(
                "media completion timeout: opcode {}, view {}, frames {}, written {}",
                command.opcode, command.view, command.length, reply.written
            )
            .into());
        }
        reply.code = self.model.store_error_code;
        reply.result.view = self.model.store_result_view;
        reply.result.empty = self.model.store_result_empty != 0;
        self.cycle();
        Ok(reply)
    }
}

impl PortIo for Simulator<'_> {
    fn capacities(&self) -> Capacities {
        Capacities {
            instructions: 256,
            states: 256,
            buffers: 128,
            quantum: 32,
            input_ports: 2,
            output_ports: 2,
            loop_slots: 4,
            pending_requests: 32,
            events: 64,
            views: 32,
            chunk_limit: 128,
            external_audio: false,
        }
    }
    fn media_slots(&self) -> u32 {
        self.media_slots
    }
    fn reset_core(&mut self) -> Result<(), String> {
        self.reset();
        Ok(())
    }
    fn core_status(&mut self) -> Result<Status, String> {
        self.model.eval();
        Ok(Status {
            faulted: self.model.faulted != 0,
            error_stage: self.model.error_stage,
            error_code: self.model.error_code,
        })
    }
    fn transition(&mut self, opcode: u8, index: u32, data: [u32; 20]) -> Result<TpReply, String> {
        self.command_raw(opcode, index, data)
            .map(|(error, words)| TpReply { error, words })
            .map_err(|e| e.to_string())
    }
    fn configure(
        &mut self,
        program: bool,
        address: u16,
        write: Option<[u32; 5]>,
    ) -> Result<[u32; 5], String> {
        self.state(address, write, program)
            .map_err(|e| e.to_string())
    }
    fn audio_word(&mut self, space: u8, address: u32, write: Option<i32>) -> Result<i32, String> {
        self.sample(space, address, write)
            .map_err(|e| e.to_string())
    }
    fn store_configure(
        &mut self,
        space: u8,
        address: u32,
        write: Option<[u32; 5]>,
    ) -> Result<[u32; 5], String> {
        self.store_config(space, address, write)
            .map_err(|e| e.to_string())
    }
    fn store_command(
        &mut self,
        command: MediaCommand,
        write: &[i32],
    ) -> Result<StoreReply, String> {
        self.store_run(command, write).map_err(|e| e.to_string())
    }
    fn run_quantum(&mut self, run: Run) -> Result<Status, String> {
        self.model.run_instruction_count = run.instructions;
        self.model.run_state_count = run.states;
        self.model.run_quantum = run.quantum;
        self.model.run_chunk_frames = run.chunk_frames;
        self.model.run_job_budget = run.job_budget;
        self.model.run_buffers = run.buffers;
        self.model.run_inputs = run.inputs;
        self.model.run_outputs = run.outputs;
        self.model.run_valid = 1;
        let mut accepted = false;
        for _ in 0..self.timeout {
            self.model.eval();
            let ready = self.model.run_ready != 0;
            self.cycle();
            if ready {
                accepted = true;
                break;
            }
        }
        self.model.run_valid = 0;
        if !accepted {
            return Err("pipeline admission timeout".into());
        }
        for _ in 0..self.maintenance_budget(u64::from(self.media_slots)) {
            if self.model.done != 0 {
                break;
            }
            self.model.job_enable = u8::from(!self.stalls || self.cycles % 5 != 0);
            self.cycle();
        }
        self.model.job_enable = 1;
        if self.model.done == 0 {
            return Err("pipeline completion timeout".into());
        }
        self.core_status()
    }
    fn record_error(&mut self, message: String) {
        self.last_error = Some(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn failed_host_settlement_retains_fault_diagnostics_and_reset_recovers() {
        let cache =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
        let rt = runtime(&cache).unwrap();
        let mut sim = Simulator::new(&rt, None, true).unwrap();
        let (error, _) = sim.command_raw(16, 0, [0; 20]).unwrap();
        assert_ne!(error, 0);
        assert_eq!(sim.model.faulted, 1);
        assert_eq!(sim.model.error_stage, 1);
        assert_eq!(sim.model.error_code, error);
        assert_eq!(sim.model.run_ready, 0);
        sim.reset();
        assert_eq!(sim.model.faulted, 0);
        assert_eq!(sim.model.error_stage, 0);
        assert_eq!(sim.model.error_code, 0);
    }
}
