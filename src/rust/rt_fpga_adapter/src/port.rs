//! Device driver over the core's valid/ready ports, independent of how they are reached.
use crate::{
    adapter::Device,
    engine::EngineDevice,
    image::{Image, Instruction, Ramp, Value},
    looping::{LoopDevice, MediaCommand, MediaResult, TpReply},
    memory::{self, MemoryReport},
    protocol::{get, put},
};
use alloc::{format, string::String, vec, vec::Vec};
use rt_runtime::Error;

/// Compiled capacities of one core configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacities {
    pub instructions: usize,
    pub states: usize,
    pub buffers: u32,
    pub quantum: u32,
    pub input_ports: u16,
    pub output_ports: u16,
    pub loop_slots: u32,
    pub pending_requests: u32,
    pub events: u32,
    pub views: usize,
    pub chunk_limit: u32,
    /// The core generates its own input and consumes its own output.
    pub external_audio: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub faulted: bool,
    pub error_stage: u8,
    pub error_code: u8,
}

/// The run port's quantum descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub instructions: u32,
    pub states: u32,
    pub quantum: u32,
    pub chunk_frames: u32,
    pub job_budget: u32,
    pub buffers: u32,
    pub inputs: u32,
    pub outputs: u32,
}

#[derive(Debug, Default)]
pub struct StoreReply {
    pub code: u8,
    pub result: MediaResult,
    pub written: usize,
}

/// One transaction per core port. Errors describe transport failures such as timeouts.
pub trait PortIo {
    fn capacities(&self) -> Capacities;
    /// Media store slot limit applied when an image is loaded.
    fn media_slots(&self) -> u32;
    fn reset_core(&mut self) -> Result<(), String>;
    fn core_status(&mut self) -> Result<Status, String>;
    fn transition(&mut self, opcode: u8, index: u32, data: [u32; 20]) -> Result<TpReply, String>;
    fn configure(
        &mut self,
        program: bool,
        address: u16,
        write: Option<[u32; 5]>,
    ) -> Result<[u32; 5], String>;
    fn audio_word(&mut self, space: u8, address: u32, write: Option<i32>) -> Result<i32, String>;
    fn store_configure(
        &mut self,
        space: u8,
        address: u32,
        write: Option<[u32; 5]>,
    ) -> Result<[u32; 5], String>;
    /// Runs a media command, streaming `write` in and collecting any read samples.
    fn store_command(&mut self, command: MediaCommand, write: &[i32])
    -> Result<StoreReply, String>;
    /// Admits one quantum and waits for its completion.
    fn run_quantum(&mut self, run: Run) -> Result<Status, String>;
    fn record_error(&mut self, message: String);
}

fn command<P: PortIo + ?Sized>(
    port: &mut P,
    opcode: u8,
    index: u32,
    data: [u32; 20],
) -> Result<[u32; 26], String> {
    let reply = port.transition(opcode, index, data)?;
    if reply.error != 0 {
        return Err(format!("TP command {opcode} error {}", reply.error));
    }
    Ok(reply.words)
}

/// Resets the core and loads a complete image, verifying program and state by readback.
pub fn load<P: PortIo + ?Sized>(port: &mut P, image: &Image) -> Result<(), String> {
    let capacities = port.capacities();
    let limits = &image.transition.limits;
    if image.program.instructions.len() > capacities.instructions
        || image.initial_state.memory.len() > capacities.states
        || image.buffers > capacities.buffers
        || image.program.quantum > capacities.quantum
        || image.input_ports > capacities.input_ports
        || image.output_ports > capacities.output_ports
        || limits.loop_slots > capacities.loop_slots
        || limits.pending_requests > capacities.pending_requests
        || limits.events > capacities.events
    {
        return Err("image exceeds device capacities".into());
    }
    if !image.media.samples.is_empty()
        || !image.media.free_chunks.is_empty()
        || image.media.views.len() > capacities.views
        || image
            .media
            .views
            .iter()
            .any(|v| !v.chunks.is_empty() || v.frames != 0)
    {
        return Err("direct media loader supports initially empty recording views only".into());
    }
    port.reset_core()?;
    for i in 0..limits.loop_slots {
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
        command(port, 1, i, words)?;
    }
    for (i, id) in image.transition.dependency_order.iter().enumerate() {
        let mut words = [0; 20];
        words[0] = id.slot;
        command(port, 2, i as u32, words)?;
    }
    let mut prepare = [0; 20];
    prepare[0] = limits.loop_slots;
    prepare[1] = limits.pending_requests;
    prepare[2] = limits.events;
    prepare[3] = image.transition.dependency_order.len() as u32;
    prepare[4] = image.program.quantum;
    command(port, 3, 0, prepare)?;
    let slots = port.media_slots();
    port.store_configure(0, 3, Some([slots, 0, 0, 0, 0]))?;
    for i in 0..image.media.views.len() {
        port.store_configure(1, i as u32, Some([1, 0, 0, 0, 0]))?;
    }
    port.store_configure(0, 0, Some([image.media.views.len() as u32, 0, 0, 0, 0]))?;
    port.store_configure(0, 1, Some([0; 5]))?;
    port.store_configure(0, 2, Some([0; 5]))?;
    for (i, instruction) in image.program.instructions.iter().enumerate() {
        let value = instruction_words(*instruction)?;
        port.configure(true, i as u16, Some(value))?;
        if port.configure(true, i as u16, None)? != value {
            return Err("instruction readback mismatch".into());
        }
    }
    for (i, value) in image.initial_state.memory.iter().enumerate() {
        let words = value_words(*value);
        port.configure(false, i as u16, Some(words))?;
        if port.configure(false, i as u16, None)? != words {
            return Err("129-bit state readback mismatch".into());
        }
    }
    Ok(())
}

/// Writes the program and initial state without resetting the core.
pub fn program<P: PortIo + ?Sized>(port: &mut P, image: &Image) -> Result<(), String> {
    let capacities = port.capacities();
    if image.program.instructions.len() > capacities.instructions
        || image.initial_state.memory.len() > capacities.states
        || image.buffers > capacities.buffers
    {
        return Err("program exceeds model capacity".into());
    }
    for (address, &instruction) in image.program.instructions.iter().enumerate() {
        port.configure(true, address as u16, Some(instruction_words(instruction)?))?;
    }
    for (address, &value) in image.initial_state.memory.iter().enumerate() {
        port.configure(false, address as u16, Some(value_words(value)))?;
    }
    Ok(())
}

/// Runs one quantum, exchanging its audio through the sample ports. With external
/// audio the core sources and sinks its own samples, so the inputs are unused and
/// the returned outputs are silent.
pub fn process<P: PortIo + ?Sized>(
    port: &mut P,
    image: &Image,
    inputs: &[Vec<i32>],
) -> Result<Vec<Vec<i32>>, String> {
    let q = image.program.quantum;
    if inputs.len() != usize::from(image.input_ports)
        || inputs.iter().any(|v| v.len() != q as usize)
    {
        return Err("input shape".into());
    }
    let external = port.capacities().external_audio;
    if !external {
        for (p, samples) in inputs.iter().enumerate() {
            for (f, &value) in samples.iter().enumerate() {
                port.audio_word(0, (p * 32 + f) as u32, Some(value))?;
            }
        }
    }
    if port.core_status()?.faulted {
        return Err("pipeline not ready".into());
    }
    let status = port.run_quantum(Run {
        instructions: image.program.instructions.len() as u32,
        states: image.initial_state.memory.len() as u32,
        quantum: q,
        chunk_frames: image.program.chunk_frames,
        job_budget: image.program.jobs_per_quantum,
        buffers: image.buffers,
        inputs: u32::from(image.input_ports),
        outputs: u32::from(image.output_ports),
    })?;
    if status.error_stage != 0 || status.faulted {
        return Err(format!(
            "pipeline fault stage {} code {}",
            status.error_stage, status.error_code
        ));
    }
    if external {
        return Ok(vec![vec![0; q as usize]; usize::from(image.output_ports)]);
    }
    let mut outputs = Vec::new();
    for output in 0..image.output_ports {
        let mut samples = Vec::new();
        for f in 0..q {
            samples.push(port.audio_word(1, u32::from(output) * 32 + f, None)?);
        }
        outputs.push(samples);
    }
    Ok(outputs)
}

pub fn value_words(value: Value) -> [u32; 5] {
    match value {
        Value::Word(v) => [v as u32, (v >> 32) as u32, 0, 0, 0],
        Value::Ramp(r) => [r.start as u32, r.target as u32, r.duration, r.elapsed, 1],
    }
}

pub fn decode_value(words: [u32; 5]) -> Value {
    if words[4] & 1 != 0 {
        Value::Ramp(Ramp {
            start: words[0] as i32,
            target: words[1] as i32,
            duration: words[2],
            elapsed: words[3],
        })
    } else {
        Value::Word(u64::from(words[0]) | (u64::from(words[1]) << 32))
    }
}

pub fn instruction_words(value: Instruction) -> Result<[u32; 5], String> {
    let slot =
        |slot: u32| u16::try_from(slot).map_err(|_| String::from("loop slot exceeds 16 bits"));
    let (opcode, args): (u8, Vec<u16>) = match value {
        Instruction::ProcessInput { port, destination } => (0, [port, destination].into()),
        Instruction::ProcessGain {
            ramp,
            source,
            destination,
        } => (2, [ramp, source, destination].into()),
        Instruction::ProcessSmoothing {
            loop_id,
            control,
            source,
        } => (
            11,
            [
                slot(loop_id.slot)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                control,
                source,
            ]
            .into(),
        ),
        Instruction::ProcessConstant { value, destination } => (
            9,
            [value as u16, (value as u32 >> 16) as u16, destination].into(),
        ),
        Instruction::ProcessArithmetic {
            left,
            right,
            destination,
            mode,
        } => (10, [left, right, destination, mode as u16].into()),
        Instruction::ProcessMix {
            left,
            right,
            destination,
        } => (3, [left, right, destination].into()),
        Instruction::ProcessOutput { source, port } => (5, [source, port].into()),
        Instruction::ProcessSilence { destination } => (8, [destination].into()),
        Instruction::ProcessLogicalPlayback {
            loop_id,
            view,
            destination,
        } => (
            6,
            [
                slot(loop_id.slot)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                view,
                destination,
            ]
            .into(),
        ),
        Instruction::ProcessLogicalCapture {
            loop_id,
            view,
            source,
        } => (
            7,
            [
                slot(loop_id.slot)?,
                loop_id.generation as u16,
                (loop_id.generation >> 16) as u16,
                view,
                source,
            ]
            .into(),
        ),
    };
    let packed = args
        .iter()
        .enumerate()
        .fold(u128::from(opcode), |word, (i, &arg)| {
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

fn failed<P: PortIo + ?Sized>(port: &mut P, error: Error) -> impl FnOnce(String) -> Error + '_ {
    move |message| {
        port.record_error(message);
        error
    }
}

impl<P: PortIo> Device for P {
    fn load(&mut self, image: &Image) -> Result<(), Error> {
        let result = load(self, image);
        result.map_err(failed(self, Error::InvalidState))
    }
    fn write_state(&mut self, address: u16, value: Value) -> Result<(), Error> {
        let result = self.configure(false, address, Some(value_words(value)));
        result
            .map(|_| ())
            .map_err(failed(self, Error::InvalidState))
    }
    fn process(&mut self, image: &Image, inputs: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
        process(self, image, inputs).map_err(|message| {
            self.record_error(message);
            match self.core_status() {
                Ok(status) if status.error_stage == 3 && status.error_code == 6 => Error::Capacity,
                _ => Error::InvalidState,
            }
        })
    }
}

impl<P: PortIo> LoopDevice for P {
    fn memory_report(&mut self) -> Result<MemoryReport, Error> {
        let faulted = |port: &mut Self| {
            port.core_status()
                .map(|status| status.faulted)
                .unwrap_or(true)
        };
        if faulted(self) {
            return Err(Error::Disconnected);
        }
        let result = (|| {
            let usage = self.store_configure(6, 16, None)?;
            let geometry = self.store_configure(6, 17, None)?;
            Ok::<_, String>((usage, geometry))
        })();
        let (usage, geometry) = result.map_err(failed(self, Error::Disconnected))?;
        if faulted(self) {
            return Err(Error::Disconnected);
        }
        let status = self.transition(0, 0, [0; 20]);
        let status = status.map_err(failed(self, Error::Disconnected))?;
        if status.error != 0 {
            return Err(Error::Disconnected);
        }
        memory::decode(get(&status.words, 0, 64), usage, geometry)
    }
    fn tp(&mut self, opcode: u8, index: u32, words: [u32; 20]) -> Result<TpReply, Error> {
        let result = self.transition(opcode, index, words);
        result.map_err(failed(self, Error::Disconnected))
    }
    fn chunk_limit(&mut self, limit: u32) -> Result<(), Error> {
        if limit == 0 || limit > self.capacities().chunk_limit {
            return Err(Error::Capacity);
        }
        let result = self.store_configure(0, 3, Some([limit, 0, 0, 0, 0]));
        result
            .map(|_| ())
            .map_err(failed(self, Error::Disconnected))
    }
    fn media(&mut self, command: MediaCommand) -> Result<MediaResult, Error> {
        let reply = self.store_command(command, &[]);
        let reply = reply.map_err(failed(self, Error::Disconnected))?;
        match reply.code {
            0 => Ok(reply.result),
            1 => Err(Error::InvalidId),
            2 => Err(Error::InvalidState),
            3 => Err(Error::Capacity),
            4 => Err(Error::InvalidShape),
            code => {
                self.record_error(format!("media fault {code}"));
                Err(Error::Disconnected)
            }
        }
    }
}

impl<P: PortIo> EngineDevice for P {
    fn program(&mut self, image: &Image) -> Result<(), Error> {
        let result = program(self, image);
        result.map_err(failed(self, Error::Disconnected))
    }
    fn read_state(&mut self, address: u16) -> Result<Value, Error> {
        let result = self.configure(false, address, None);
        result
            .map(decode_value)
            .map_err(failed(self, Error::Disconnected))
    }
    fn scratch(&mut self, buffer: u16) -> Result<Vec<i32>, Error> {
        let mut samples = Vec::with_capacity(32);
        for frame in 0..32 {
            let sample = self.audio_word(2, u32::from(buffer) * 32 + frame, None);
            samples.push(sample.map_err(failed(self, Error::Disconnected))?);
        }
        Ok(samples)
    }
}
