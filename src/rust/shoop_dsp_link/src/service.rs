use crate::{validate_block, Block, Buffer, Descriptor, Error, Header, MidiEvent, Transport};
use alloc::{vec, vec::Vec};
use shoop_dsp::{MidiBuffer, Processor};

/// Prepared storage and exclusive source-order execution shared by link adapters.
pub struct ProcessingBlock {
    pub input: Vec<f32>,
    pub output: Vec<f32>,
    pub midi_input: MidiBuffer,
    pub midi_output: MidiBuffer,
    config: shoop_dsp::Config,
    next: Option<u64>,
}
impl ProcessingBlock {
    pub fn prepare(config: shoop_dsp::Config, midi_capacity: usize) -> Result<Self, Error> {
        config.validate().map_err(|_| Error::Shape)?;
        if midi_capacity > 65536 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            input: vec![0.; config.frames * config.channels],
            output: vec![0.; config.frames * config.channels],
            midi_input: MidiBuffer::new(midi_capacity),
            midi_output: MidiBuffer::new(midi_capacity),
            config,
            next: None,
        })
    }
    pub fn process(
        &mut self,
        dsp: &mut dyn Processor,
        frame: u64,
        transport: Transport,
    ) -> Result<(), Error> {
        if dsp.info().config != self.config {
            return Err(Error::Shape);
        }
        if self.next.is_some_and(|next| next != frame) {
            return Err(Error::Discontinuity);
        }
        let end = frame
            .checked_add(self.config.frames as u64)
            .ok_or(Error::Capacity)?;
        self.output.fill(0.);
        self.midi_output.clear();
        dsp.process(shoop_dsp::Block {
            frame,
            frames: self.config.frames,
            input: &self.input,
            output: &mut self.output,
            midi_input: self.midi_input.as_slice(),
            midi_output: &mut self.midi_output,
            transport,
        })
        .map_err(|_| Error::Dsp)?;
        self.next = Some(end);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub inputs: u64,
    pub dsp_blocks: u64,
    pub outputs: u64,
    pub concealed_frames: u64,
}

/// Logical Q/B adaptation. The carrier decides when a missing input is declared;
/// no timer, packet format or OS execution model is imposed here.
pub struct Service {
    descriptor: Descriptor,
    block: ProcessingBlock,
    output: Block,
    input_fill: usize,
    output_fill: usize,
    next_input: u64,
    block_frame: u64,
    output_frame: u64,
    transport: Transport,
    concealed: bool,
    midi_reset: bool,
    stats: Stats,
}
impl Service {
    pub fn prepare(descriptor: Descriptor) -> Result<Self, Error> {
        let d = descriptor.validate()?;
        let midi = d.midi_capacity as usize
            * ((d.dsp_frames as usize).div_ceil(d.quantum as usize) + 1)
            + 32;
        Ok(Self {
            descriptor: d,
            block: ProcessingBlock::prepare(d.dsp_config(), midi)?,
            output: Block::prepare(d)?,
            input_fill: 0,
            output_fill: 0,
            next_input: d.start_frame,
            block_frame: d.start_frame,
            output_frame: d.start_frame,
            transport: Transport::default(),
            concealed: false,
            midi_reset: false,
            stats: Stats::default(),
        })
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    pub fn push(
        &mut self,
        input: &impl Buffer,
        dsp: &mut dyn Processor,
        mut emit: impl FnMut(&Block),
    ) -> Result<(), Error> {
        let d = self.descriptor;
        validate_block(d, input)?;
        let h = input.header();
        if h.frame < self.next_input {
            return Err(Error::Stale);
        }
        let gap = h.frame - self.next_input;
        if gap > u64::from(d.slots) * u64::from(d.quantum) {
            return Err(Error::Future);
        }
        if self.input_fill != 0 && h.transport != self.transport {
            return Err(Error::Discontinuity);
        }
        if self.input_fill == 0 {
            self.transport = h.transport;
        }
        if gap != 0 || h.midi_reset {
            self.recover_input()?;
        }
        for _ in 0..gap {
            self.sample(None, 0, true, dsp, &mut emit)?;
            self.stats.concealed_frames += 1;
        }
        let mut event = 0;
        for i in 0..d.quantum as usize {
            if self.input_fill == 0 {
                self.transport = h.transport;
            }
            while event < input.midi().len() && input.midi()[event].frame as usize == i {
                let mut e = input.midi()[event];
                e.frame = self.input_fill as u32;
                self.block.midi_input.push(e).map_err(|_| Error::Capacity)?;
                event += 1;
            }
            self.sample(Some(input.audio()), i, h.concealed, dsp, &mut emit)?;
        }
        self.stats.inputs += 1;
        Ok(())
    }
    fn recover_input(&mut self) -> Result<(), Error> {
        self.midi_reset = true;
        for channel in 0..16 {
            for control in [120, 123] {
                self.block
                    .midi_input
                    .push(
                        MidiEvent::new(self.input_fill as u32, 0, &[0xb0 | channel, control, 0])
                            .map_err(|_| Error::Shape)?,
                    )
                    .map_err(|_| Error::Capacity)?;
            }
        }
        Ok(())
    }
    fn sample(
        &mut self,
        input: Option<&[f32]>,
        i: usize,
        concealed: bool,
        dsp: &mut dyn Processor,
        emit: &mut impl FnMut(&Block),
    ) -> Result<(), Error> {
        let d = self.descriptor;
        let b = d.dsp_frames as usize;
        for c in 0..d.channels as usize {
            self.block.input[c * b + self.input_fill] =
                input.map_or(0., |a| a[c * d.quantum as usize + i]);
        }
        self.concealed |= concealed;
        self.input_fill += 1;
        self.next_input = self.next_input.checked_add(1).ok_or(Error::Capacity)?;
        if self.input_fill != b {
            return Ok(());
        }
        self.block.process(dsp, self.block_frame, self.transport)?;
        self.stats.dsp_blocks += 1;
        let midi = self.block.midi_output.as_slice();
        let valid = midi
            .iter()
            .all(|e| e.frame < b as u32 && e.port == 0 && e.bytes().is_ok())
            && midi.windows(2).all(|w| w[0].frame <= w[1].frame);
        let mut event = 0;
        for offset in 0..b {
            let mut header = self.output.header();
            header.concealed |= self.concealed;
            header.midi_reset |= self.concealed || self.midi_reset || !valid;
            self.output.set_header(header);
            for c in 0..d.channels as usize {
                self.output.audio_mut()[c * d.quantum as usize + self.output_fill] =
                    self.block.output[c * b + offset];
            }
            if valid {
                while event < midi.len() && midi[event].frame as usize == offset {
                    let mut e = midi[event];
                    e.frame = self.output_fill as u32;
                    if self.output.push_midi(e).is_err() {
                        let mut h = self.output.header();
                        h.midi_reset = true;
                        self.output.set_header(h);
                    }
                    event += 1;
                }
            }
            self.output_fill += 1;
            if self.output_fill == d.quantum as usize {
                let mut h = self.output.header();
                h.frame = self.output_frame;
                h.generation = d.generation;
                h.transport = self.transport;
                self.output.set_header(h);
                if h.midi_reset {
                    self.output.clear_midi();
                }
                emit(&self.output);
                self.stats.outputs += 1;
                self.output_frame = self
                    .output_frame
                    .checked_add(u64::from(d.quantum))
                    .ok_or(Error::Capacity)?;
                self.output_fill = 0;
                self.output.clear_midi();
                self.output.set_header(Header {
                    generation: d.generation,
                    ..Header::default()
                });
            }
        }
        self.block_frame = self.next_input;
        self.input_fill = 0;
        self.block.midi_input.clear();
        self.concealed = false;
        self.midi_reset = false;
        Ok(())
    }
}

pub struct ReferenceService {
    pub endpoint: crate::memory::ServiceEndpoint,
    service: Service,
    pub dropped_results: u64,
}
impl ReferenceService {
    pub fn prepare(endpoint: crate::memory::ServiceEndpoint) -> Result<Self, Error> {
        let service = Service::prepare(endpoint.descriptor())?;
        Ok(Self {
            endpoint,
            service,
            dropped_results: 0,
        })
    }
    pub fn stats(&self) -> Stats {
        self.service.stats()
    }
    pub fn poll(&mut self, dsp: &mut dyn Processor) -> Result<usize, Error> {
        let mut processed = 0;
        for _ in 0..self.endpoint.descriptor().slots {
            let Some(input) = self.endpoint.receive() else {
                break;
            };
            let endpoint = &mut self.endpoint;
            let drops = &mut self.dropped_results;
            let result = self.service.push(&input, dsp, |block| {
                if let Some(mut output) = endpoint.acquire_output() {
                    if output.copy_from(block).is_ok() {
                        if let Err(output) = endpoint.publish(output) {
                            endpoint.release_output(output);
                            *drops += 1;
                        }
                    } else {
                        endpoint.release_output(output);
                        *drops += 1;
                    }
                } else {
                    *drops += 1;
                }
            });
            self.endpoint.release_input(input);
            if let Err(error) = result {
                self.endpoint.set_available(false);
                return Err(error);
            }
            processed += 1;
        }
        Ok(processed)
    }
}
