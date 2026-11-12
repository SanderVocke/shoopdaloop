//! Native simulator only: the processing and memory service remain RTL.
use marlin::verilator::{
    VerilatedModelConfig, VerilatorRuntime, VerilatorRuntimeOptions, WideIn,
    tracing::{TraceFile, Waveform},
};
use marlin::verilog::prelude::*;
use rt_backend_testbench::rtl::{Image, Instruction, Value};
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
            "media_ram metadata_cache media_metadata media_prefetch_resolver media_store memory_delay media_audio_pipeline",
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
    Ok(VerilatorRuntime::new2(
        artifacts,
        &sources,
        &[root.join("fpga/state_processor/rtl")],
        [],
        VerilatorRuntimeOptions::default().verilator_executable(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("verilator-wrapper")
                .into(),
        ),
    )?)
}

pub struct Simulator<'ctx> {
    model: Pipeline<'ctx>,
    trace: Option<TraceFile<'ctx>>,
    pub cycles: u64,
    pub stalls: bool,
    pub timeout: u64,
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
            stalls,
            timeout: 100_000,
        };
        sim.reset();
        Ok(sim)
    }
    pub fn reset(&mut self) {
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
        self.model.command_opcode = opcode;
        self.model.command_index = index;
        self.model.command_data = WideIn::new(data);
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
        if error != 0 {
            return Err(format!("TP command {opcode} error {error}").into());
        }
        Ok(value)
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
        if image.program.instructions.len() > 64
            || image.initial_state.memory.len() > 32
            || image.buffers > 16
            || image.program.quantum > 32
            || image.input_ports > 2
            || image.output_ports > 2
            || image.transition.limits.loop_slots > 4
            || image.transition.limits.pending_requests > 8
            || image.transition.limits.events > 16
        {
            return Err("image exceeds simulation capacities".into());
        }
        self.reset();
        for i in 0..image.transition.limits.loop_slots {
            self.command(1, i, [0; 20])?;
        }
        if !image.transition.description.loops.is_empty() {
            return Err("loop configuration not yet supported".into());
        }
        let mut prepare = [0; 20];
        prepare[0] = image.transition.limits.loop_slots;
        prepare[1] = image.transition.limits.pending_requests;
        prepare[2] = image.transition.limits.events;
        prepare[4] = image.program.quantum;
        self.command(3, 0, prepare)?;
        for (i, instruction) in image.program.instructions.iter().enumerate() {
            let value = instruction_words(*instruction)?;
            self.state(i as u16, Some(value), true)?;
            if self.state(i as u16, None, true)? != value {
                return Err("instruction readback mismatch".into());
            }
        }
        for (i, value) in image.initial_state.memory.iter().enumerate() {
            let words = value_words(*value);
            self.state(i as u16, Some(words), false)?;
            if self.state(i as u16, None, false)? != words {
                return Err("129-bit state readback mismatch".into());
            }
        }
        Ok(())
    }
    pub fn run(&mut self, image: &Image, inputs: &[Vec<i32>]) -> SimResult<Vec<Vec<i32>>> {
        let q = image.program.quantum;
        if inputs.len() != usize::from(image.input_ports)
            || inputs.iter().any(|v| v.len() != q as usize)
        {
            return Err("input shape".into());
        }
        for (p, samples) in inputs.iter().enumerate() {
            for (f, &value) in samples.iter().enumerate() {
                self.sample(0, (p * 32 + f) as u32, Some(value))?;
            }
        }
        self.model.eval();
        if self.model.run_ready == 0 {
            return Err("pipeline not ready".into());
        }
        self.model.run_instruction_count = image.program.instructions.len() as u32;
        self.model.run_state_count = image.initial_state.memory.len() as u32;
        self.model.run_quantum = q;
        self.model.run_chunk_frames = image.program.chunk_frames;
        self.model.run_job_budget = image.program.jobs_per_quantum;
        self.model.run_buffers = image.buffers;
        self.model.run_inputs = u32::from(image.input_ports);
        self.model.run_outputs = u32::from(image.output_ports);
        self.model.run_valid = 1;
        self.cycle();
        self.model.run_valid = 0;
        for _ in 0..self.timeout {
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
        if self.model.error_stage != 0 || self.model.faulted != 0 {
            return Err(format!(
                "pipeline fault stage {} code {}",
                self.model.error_stage, self.model.error_code
            )
            .into());
        }
        let mut outputs = vec![];
        for port in 0..image.output_ports {
            let mut samples = vec![];
            for f in 0..q {
                samples.push(self.sample(1, u32::from(port) * 32 + f, None)?);
            }
            outputs.push(samples);
        }
        Ok(outputs)
    }
}
pub fn value_words(value: Value) -> [u32; 5] {
    match value {
        Value::Word(v) => [v as u32, (v >> 32) as u32, 0, 0, 0],
        Value::Ramp(r) => [r.start as u32, r.target as u32, r.duration, r.elapsed, 1],
    }
}
pub fn instruction_words(value: Instruction) -> SimResult<[u32; 5]> {
    let (opcode, args) = match value {
        Instruction::ProcessInput { port, destination } => (0, vec![port, destination]),
        Instruction::ProcessGain {
            ramp,
            source,
            destination,
        } => (2, vec![ramp, source, destination]),
        Instruction::ProcessMix {
            left,
            right,
            destination,
        } => (3, vec![left, right, destination]),
        Instruction::ProcessOutput { source, port } => (5, vec![source, port]),
        Instruction::ProcessSilence { destination } => (8, vec![destination]),
        Instruction::ProcessLogicalPlayback {
            loop_id,
            view,
            destination,
        } => (
            6,
            vec![
                u16::try_from(loop_id.slot)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                view,
                destination,
            ],
        ),
        Instruction::ProcessLogicalCapture {
            loop_id,
            view,
            source,
        } => (
            7,
            vec![
                u16::try_from(loop_id.slot)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                view,
                source,
            ],
        ),
    };
    let packed = args
        .iter()
        .enumerate()
        .fold(opcode as u128, |word, (i, &arg)| {
            word | (u128::from(arg) << (8 + 16 * i))
        });
    Ok([
        packed as u32,
        (packed >> 32) as u32,
        (packed >> 64) as u32,
        0,
        0,
    ])
}
