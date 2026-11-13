//! Native simulator only: the processing and memory service remain RTL.
use marlin::verilator::{
    VerilatedModelConfig, VerilatorRuntime, VerilatorRuntimeOptions, WideIn,
    tracing::{TraceFile, Waveform},
};
use marlin::verilog::prelude::*;
use rt_fpga_adapter::image::{Image, Instruction, Value};
use std::path::{Path, PathBuf};

pub type SimResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[verilog(src = "rtl/marlin_pipeline.sv", name = "marlin_pipeline")]
struct Pipeline;

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub artifacts: PathBuf,
    pub rtl_directory: PathBuf,
    pub verilator: std::ffi::OsString,
    pub jobs: usize,
}
impl RuntimeConfig {
    pub fn development(artifacts: &Path) -> Self {
        Self {
            artifacts: artifacts.into(),
            rtl_directory: std::env::var_os("SHOOP_FPGA_RTL")
                .map(PathBuf::from)
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fpga")),
            verilator: std::env::var_os("SHOOP_VERILATOR").unwrap_or_else(|| "verilator".into()),
            jobs: std::env::var("SHOOP_VERILATOR_JOBS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
        }
    }
}
pub fn runtime(artifacts: &Path) -> SimResult<VerilatorRuntime> {
    runtime_with_config(&RuntimeConfig::development(artifacts))
}
pub fn runtime_with_config(config: &RuntimeConfig) -> SimResult<VerilatorRuntime> {
    crate::check_verilator(&config.verilator)?;
    if !config.rtl_directory.is_dir() {
        return Err(format!(
            "FPGA RTL source directory is missing: {}",
            config.rtl_directory.display()
        )
        .into());
    }
    if config.jobs == 0 {
        return Err("Verilator build jobs must be positive".into());
    }
    std::fs::create_dir_all(&config.artifacts)?;
    let artifacts = config.artifacts.canonicalize()?;
    let root = config.rtl_directory.canonicalize()?;
    let top = artifacts.join("marlin_pipeline.sv");
    let source = include_str!("../rtl/marlin_pipeline.sv");
    if std::fs::read_to_string(&top).ok().as_deref() != Some(source) {
        std::fs::write(&top, source)?;
    }
    let mut sources = vec![top.clone()];
    sources.extend(pipeline_sources_at(&root));
    sources.push(root.join("axi_memory/tests/axi_direct_memory.sv"));
    let options = VerilatorRuntimeOptions {
        verilator_executable: config.verilator.clone(),
        build_jobs: config.jobs,
        source_path_overrides: vec![(
            Pipeline::source_path().into(),
            top.to_str().ok_or("RTL path is not UTF-8")?.into(),
        )],
        ..Default::default()
    };
    Ok(VerilatorRuntime::new2(
        artifacts,
        &sources,
        &[root.join("state_processor/rtl")],
        [],
        options,
    )?)
}

/// Processing and AXI modules shared by native simulation tops.
pub fn pipeline_sources() -> Vec<PathBuf> {
    pipeline_sources_at(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fpga"))
}
fn pipeline_sources_at(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
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
                .map(|name| root.join(format!("{directory}/rtl/{name}.sv"))),
        );
    }
    sources
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
pub struct Simulator<'ctx> {
    // Close the trace while its model is still alive (fields drop in order).
    trace: Option<TraceFile<'ctx>>,
    model: Pipeline<'ctx>,
    pub cycles: u64,
    media_slots: u32,
    pub stalls: bool,
    pub timeout: u64,
    pub last_error: Option<String>,
    held_reply: Option<(u8, [u32; 26])>,
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
            held_reply: None,
        };
        sim.reset();
        Ok(sim)
    }
    pub fn store_usage(&mut self) -> SimResult<StoreUsage> {
        let mut values = [0; 4];
        for (address, value) in values.iter_mut().enumerate() {
            *value = self.store_config(0, address as u32, None)?[0];
        }
        Ok(StoreUsage {
            issued_views: values[0],
            high_water_slots: values[1],
            free_slots: values[2],
            slot_limit: values[3],
        })
    }
    pub fn arena_capacity(&mut self, slots: u32) -> SimResult<()> {
        if slots == 0 || slots > 32768 {
            return Err("invalid simulation arena capacity".into());
        }
        self.media_slots = slots;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.held_reply = None;
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
            if let Some((error, words)) = self.held_reply {
                assert_ne!(self.model.response_valid, 0, "held TP valid disappeared");
                assert_eq!(self.model.response_error, error);
                assert_eq!(
                    *self.model.response_data.value(),
                    words,
                    "held TP payload changed"
                );
            }
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
    pub fn hold_command(
        &mut self,
        opcode: u8,
        index: u32,
        data: [u32; 20],
    ) -> SimResult<(u8, [u32; 26])> {
        if self.held_reply.is_some() {
            return Err("a TP reply is already held".into());
        }
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
        self.held_reply = Some((error, value));
        Ok((error, value))
    }
    pub fn held_response(&mut self) -> Option<(u8, [u32; 26])> {
        self.model.eval();
        (self.model.response_valid != 0)
            .then(|| (self.model.response_error, *self.model.response_data.value()))
    }
    pub fn consume_response(&mut self) -> SimResult<(u8, [u32; 26])> {
        let response = self.held_response().ok_or("no held TP response")?;
        self.held_reply = None;
        self.model.response_ready = 1;
        self.cycle();
        self.model.response_ready = 0;
        Ok(response)
    }
    pub fn command_raw(
        &mut self,
        opcode: u8,
        index: u32,
        data: [u32; 20],
    ) -> SimResult<(u8, [u32; 26])> {
        let response = self.hold_command(opcode, index, data)?;
        for _ in 0..if self.stalls { 3 } else { 0 } {
            self.cycle();
            if self.held_response() != Some(response) {
                return Err("TP response changed while stalled".into());
            }
        }
        self.consume_response()
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
        if image.program.instructions.len() > 256
            || image.initial_state.memory.len() > 256
            || image.buffers > 128
            || image.program.quantum > 32
            || image.input_ports > 2
            || image.output_ports > 2
            || image.transition.limits.loop_slots > 4
            || image.transition.limits.pending_requests > 32
            || image.transition.limits.events > 64
        {
            return Err("image exceeds simulation capacities".into());
        }
        if !image.media.samples.is_empty()
            || !image.media.free_chunks.is_empty()
            || image.media.views.len() > 32
            || image
                .media
                .views
                .iter()
                .any(|v| !v.chunks.is_empty() || v.frames != 0)
        {
            return Err("direct media loader supports initially empty recording views only".into());
        }
        self.reset();
        for i in 0..image.transition.limits.loop_slots {
            let mut words = [0; 20];
            if let Some(spec) = image
                .transition
                .description
                .loops
                .iter()
                .find(|s| s.id.slot == i)
            {
                if spec.kind != "primitive" || spec.timeline.is_some() {
                    return Err("unsupported direct TP spec".into());
                }
                put(&mut words, 0, 1, 1);
                put(&mut words, 1, 32, u64::from(spec.id.generation));
                if let Some(source) = spec.sync_source {
                    put(&mut words, 33, 1, 1);
                    put(
                        &mut words,
                        34,
                        64,
                        u64::from(source.slot) | (u64::from(source.generation) << 32),
                    );
                }
                put(&mut words, 98, 1, u64::from(spec.repeat_sync));
                put(&mut words, 128, 64, spec.length);
            }
            self.command(1, i, words)?;
        }
        for (i, id) in image.transition.dependency_order.iter().enumerate() {
            let mut words = [0; 20];
            words[0] = id.slot;
            self.command(2, i as u32, words)?;
        }
        let mut prepare = [0; 20];
        prepare[0] = image.transition.limits.loop_slots;
        prepare[1] = image.transition.limits.pending_requests;
        prepare[2] = image.transition.limits.events;
        prepare[3] = image.transition.dependency_order.len() as u32;
        prepare[4] = image.program.quantum;
        prepare[5] = rt_fpga_adapter::transition_service::wire::ABI;
        self.command(3, 0, prepare)?;
        self.store_config(0, 3, Some([self.media_slots, 0, 0, 0, 0]))?;
        for i in 0..image.media.views.len() {
            self.store_config(1, i as u32, Some([1, 0, 0, 0, 0]))?;
        }
        self.store_config(0, 0, Some([image.media.views.len() as u32, 0, 0, 0, 0]))?;
        self.store_config(0, 1, Some([0; 5]))?;
        self.store_config(0, 2, Some([0; 5]))?;
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
        self.model.store_cmd_opcode = 5;
        self.model.store_cmd_view = u64::from(view);
        self.model.store_cmd_start = 0;
        self.model.store_cmd_length = 0;
        self.model.store_cmd_count = u32::try_from(samples.len())?;
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
            return Err("append admission timeout".into());
        }
        let mut offset = 0;
        for _ in 0..self.maintenance_budget(u64::from(self.media_slots)) {
            if self.model.store_done != 0 {
                break;
            }
            self.model.store_write_valid = u8::from(offset < samples.len());
            self.model.store_write_data = samples.get(offset).copied().unwrap_or(0) as u32;
            self.model.eval();
            let write = self.model.store_write_valid != 0 && self.model.store_write_ready != 0;
            self.cycle();
            if write {
                offset += 1;
            }
        }
        self.model.store_write_valid = 0;
        if self.model.store_done == 0 {
            return Err(format!("append completion timeout after {offset} samples").into());
        }
        let code = self.model.store_error_code;
        self.cycle();
        if code != 0 {
            return Err(format!("append error {code} after {offset} samples").into());
        }
        if offset != samples.len() {
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
        if self.model.faulted != 0 {
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
        Instruction::ProcessSmoothing {
            loop_id,
            control,
            source,
        } => (
            11,
            vec![
                u16::try_from(loop_id.slot)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                control,
                source,
            ],
        ),
        Instruction::ProcessConstant { value, destination } => (
            9,
            vec![value as u16, (value as u32 >> 16) as u16, destination],
        ),
        Instruction::ProcessArithmetic {
            left,
            right,
            destination,
            mode,
        } => (10, vec![left, right, destination, mode as u16]),
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

impl rt_fpga_adapter::adapter::Device for Simulator<'_> {
    fn load(&mut self, image: &Image) -> std::result::Result<(), rt_runtime::Error> {
        let result = Simulator::load(self, image);
        result.map_err(|e| {
            self.last_error = Some(e.to_string());
            rt_runtime::Error::InvalidState
        })
    }
    fn write_state(
        &mut self,
        address: u16,
        value: Value,
    ) -> std::result::Result<(), rt_runtime::Error> {
        let result = self.state(address, Some(value_words(value)), false);
        result.map(|_| ()).map_err(|e| {
            self.last_error = Some(e.to_string());
            rt_runtime::Error::InvalidState
        })
    }
    fn process(
        &mut self,
        image: &Image,
        inputs: &[Vec<i32>],
    ) -> std::result::Result<Vec<Vec<i32>>, rt_runtime::Error> {
        let result = self.run(image, inputs);
        result.map_err(|e| {
            self.last_error = Some(e.to_string());
            if self.model.error_stage == 3 && self.model.error_code == 6 {
                rt_runtime::Error::Capacity
            } else {
                rt_runtime::Error::InvalidState
            }
        })
    }
}

pub fn put<const N: usize>(words: &mut [u32; N], offset: usize, width: usize, value: u64) {
    assert!(width <= 64 && offset + width <= N * 32);
    for i in 0..width {
        let bit = offset + i;
        words[bit / 32] =
            (words[bit / 32] & !(1 << (bit % 32))) | ((((value >> i) & 1) as u32) << (bit % 32));
    }
}
pub fn get<const N: usize>(words: &[u32; N], offset: usize, width: usize) -> u64 {
    assert!(width <= 64 && offset + width <= N * 32);
    (0..width).fold(0, |v, i| {
        v | u64::from((words[(offset + i) / 32] >> ((offset + i) % 32)) & 1) << i
    })
}

impl rt_fpga_adapter::looping::LoopDevice for Simulator<'_> {
    fn memory_report(
        &mut self,
    ) -> Result<rt_fpga_adapter::memory::MemoryReport, rt_runtime::Error> {
        if self.model.faulted != 0 {
            return Err(rt_runtime::Error::Disconnected);
        }
        let result = (|| {
            let usage = self.store_config(6, 16, None)?;
            let geometry = self.store_config(6, 17, None)?;
            Ok::<_, Box<dyn std::error::Error>>((usage, geometry))
        })();
        let (usage, geometry) = result.map_err(|error| {
            self.last_error = Some(error.to_string());
            rt_runtime::Error::Disconnected
        })?;
        if self.model.faulted != 0 {
            return Err(rt_runtime::Error::Disconnected);
        }
        rt_fpga_adapter::memory::decode(self.frame(), usage, geometry)
    }
    fn tp(
        &mut self,
        opcode: u8,
        index: u32,
        words: [u32; 20],
    ) -> std::result::Result<rt_fpga_adapter::looping::TpReply, rt_runtime::Error> {
        self.command_raw(opcode, index, words)
            .map(|(error, words)| rt_fpga_adapter::looping::TpReply { error, words })
            .map_err(|e| {
                self.last_error = Some(e.to_string());
                rt_runtime::Error::Disconnected
            })
    }
    fn chunk_limit(&mut self, limit: u32) -> std::result::Result<(), rt_runtime::Error> {
        if limit == 0 || limit > 128 {
            return Err(rt_runtime::Error::Capacity);
        }
        self.store_config(0, 3, Some([limit, 0, 0, 0, 0]))
            .map(|_| ())
            .map_err(|e| {
                self.last_error = Some(e.to_string());
                rt_runtime::Error::Disconnected
            })
    }
    fn media(
        &mut self,
        command: rt_fpga_adapter::looping::MediaCommand,
    ) -> std::result::Result<rt_fpga_adapter::looping::MediaResult, rt_runtime::Error> {
        use rt_runtime::Error;
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
            self.last_error = Some("media admission timeout".into());
            return Err(Error::Disconnected);
        }
        let mut result = rt_fpga_adapter::looping::MediaResult::default();
        for _ in 0..self.maintenance_budget(if command.opcode == 1 {
            command.length.div_ceil(32)
        } else {
            u64::from(self.media_slots)
        }) {
            if self.model.store_done != 0 {
                break;
            }
            self.model.store_read_ready = u8::from(!self.stalls || self.cycles % 4 != 0);
            self.model.eval();
            if self.model.store_read_valid != 0 && self.model.store_read_ready != 0 {
                result.samples.push(self.model.store_read_data as i32);
            }
            self.cycle();
        }
        self.model.store_read_ready = 0;
        if self.model.store_done == 0 {
            self.last_error = Some(format!(
                "media completion timeout: opcode {}, view {}, frames {}",
                command.opcode, command.view, command.length
            ));
            return Err(Error::Disconnected);
        }
        let code = self.model.store_error_code;
        result.view = self.model.store_result_view;
        result.empty = self.model.store_result_empty != 0;
        self.cycle();
        match code {
            0 => Ok(result),
            1 => Err(Error::InvalidId),
            2 => Err(Error::InvalidState),
            3 => Err(Error::Capacity),
            4 => Err(Error::InvalidShape),
            _ => {
                self.last_error = Some(format!("media fault {code}"));
                Err(Error::Disconnected)
            }
        }
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

impl rt_fpga_adapter::engine::EngineDevice for Simulator<'_> {
    fn program(&mut self, image: &Image) -> Result<(), rt_runtime::Error> {
        let result = (|| -> SimResult<()> {
            if image.program.instructions.len() > 256
                || image.initial_state.memory.len() > 256
                || image.buffers > 128
            {
                return Err("program exceeds model capacity".into());
            }
            for (address, &instruction) in image.program.instructions.iter().enumerate() {
                self.state(address as u16, Some(instruction_words(instruction)?), true)?;
            }
            for (address, &value) in image.initial_state.memory.iter().enumerate() {
                self.state(address as u16, Some(value_words(value)), false)?;
            }
            Ok(())
        })();
        result.map_err(|error| {
            self.last_error = Some(error.to_string());
            rt_runtime::Error::Disconnected
        })
    }
    fn read_state(&mut self, address: u16) -> Result<Value, rt_runtime::Error> {
        self.state(address, None, false)
            .map(|words| {
                if words[4] & 1 != 0 {
                    Value::Ramp(rt_fpga_adapter::image::Ramp {
                        start: words[0] as i32,
                        target: words[1] as i32,
                        duration: words[2],
                        elapsed: words[3],
                    })
                } else {
                    Value::Word(u64::from(words[0]) | (u64::from(words[1]) << 32))
                }
            })
            .map_err(|error| {
                self.last_error = Some(error.to_string());
                rt_runtime::Error::Disconnected
            })
    }
    fn scratch(&mut self, buffer: u16) -> Result<Vec<i32>, rt_runtime::Error> {
        (0..32)
            .map(|frame| {
                self.sample(2, u32::from(buffer) * 32 + frame, None)
                    .map_err(|error| {
                        self.last_error = Some(error.to_string());
                        rt_runtime::Error::Disconnected
                    })
            })
            .collect()
    }
}
