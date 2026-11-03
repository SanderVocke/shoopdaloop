#![no_std]

use rt_runtime::graph::PortCounts;
use rt_runtime::planner::Planner;
use rt_runtime::processor::{ProcessBlock, Processor, ProcessorOperation};
use rt_runtime::{Error, PreparedConfig};
use shoop_dsp_link::{
    returns::Returns, Block, Buffer, Descriptor, Endpoint, Header, MidiEvent, Transport,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Processed,
    Concealed,
    Silence,
}
#[derive(Clone, Copy, Debug)]
pub struct Delivery {
    pub frame: u64,
    pub quality: Quality,
}
#[derive(Clone, Copy, Debug)]
pub enum Observation {
    Delivery(Delivery),
    InputDropped(u64),
    Stats(Stats),
}
pub struct Observer {
    records: rtrb::Consumer<Observation>,
}
impl Observer {
    pub fn receive(&mut self) -> Option<Observation> {
        self.records.pop().ok()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub submitted: u64,
    pub input_dropped: u64,
    pub received: u64,
    pub rejected: u64,
    pub processed: u64,
    pub concealed: u64,
    pub fallback: u64,
    pub midi_dropped: u64,
    pub midi_resets: u64,
    pub retired: bool,
    pub observation_dropped: u64,
}

pub struct RemoteNode<E: Endpoint> {
    endpoint: E,
    descriptor: Descriptor,
    input: Block,
    output: Block,
    returns: Returns,
    input_fill: usize,
    output_fill: usize,
    expected_frame: u64,
    input_frame: u64,
    transport: Transport,
    stats: Stats,
    observations: Option<rtrb::Producer<Observation>>,
}
impl<E: Endpoint> RemoteNode<E> {
    pub fn prepare(endpoint: E, config: PreparedConfig) -> Result<Self, Error> {
        let d = endpoint
            .descriptor()
            .validate()
            .map_err(|_| Error::InvalidConfig)?;
        if config.limits().sample_rate != d.sample_rate
            || config.limits().max_frames > d.max_callback as usize
        {
            return Err(Error::InvalidConfig);
        }
        Ok(Self {
            endpoint,
            descriptor: d,
            input: Block::prepare(d).map_err(|_| Error::Capacity)?,
            output: Block::prepare(d).map_err(|_| Error::Capacity)?,
            returns: Returns::prepare(d).map_err(|_| Error::Capacity)?,
            input_fill: 0,
            output_fill: 0,
            expected_frame: d.start_frame,
            input_frame: d.start_frame,
            transport: Transport {
                playing: true,
                tempo: None,
            },
            stats: Stats::default(),
            observations: None,
        })
    }
    pub fn prepare_observed(
        endpoint: E,
        config: PreparedConfig,
    ) -> Result<(Self, Observer), Error> {
        let mut node = Self::prepare(endpoint, config)?;
        let capacity = 2
            * (node.descriptor.max_callback as usize).div_ceil(node.descriptor.quantum as usize)
            + 8;
        let (producer, consumer) = rtrb::RingBuffer::new(capacity);
        node.observations = Some(producer);
        Ok((node, Observer { records: consumer }))
    }
    fn observe(&mut self, record: Observation) {
        if self
            .observations
            .as_mut()
            .is_some_and(|p| p.push(record).is_err())
        {
            self.stats.observation_dropped += 1;
        }
    }
    pub fn descriptor(&self) -> Descriptor {
        self.descriptor
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    pub fn set_transport(&mut self, transport: Transport) -> Result<(), Error> {
        if transport.tempo.is_some_and(|t| !t.is_finite() || t <= 0.)
            || self.input_fill != 0
            || (self.input_frame - self.descriptor.start_frame)
                % u64::from(self.descriptor.dsp_frames)
                != 0
        {
            return Err(Error::InvalidParameter);
        }
        self.transport = transport;
        Ok(())
    }
    pub fn into_endpoint(self) -> E {
        self.endpoint
    }
    fn submit(&mut self) {
        let mut header = self.input.header();
        header.generation = self.descriptor.generation;
        header.frame = self.input_frame;
        header.transport = self.transport;
        self.input.set_header(header);
        let mut sent = false;
        if self.endpoint.available() {
            if let Some(mut lease) = self.endpoint.acquire_input() {
                lease.set_header(header);
                lease.audio_mut().copy_from_slice(self.input.audio());
                lease.clear_midi();
                let mut valid = true;
                for event in self.input.midi() {
                    valid &= lease.push_midi(*event).is_ok();
                }
                if valid {
                    match self.endpoint.submit(lease) {
                        Ok(()) => sent = true,
                        Err(lease) => self.endpoint.release_input(lease),
                    }
                } else {
                    self.endpoint.release_input(lease);
                }
            }
        }
        self.input.clear_midi();
        self.input.set_header(Header {
            midi_reset: !sent,
            ..Header::default()
        });
        if sent {
            self.stats.submitted += 1;
        } else {
            self.stats.input_dropped += 1;
            self.observe(Observation::InputDropped(self.input_frame));
        }
        self.input_fill = 0;
        match self
            .input_frame
            .checked_add(u64::from(self.descriptor.quantum))
        {
            Some(frame) => self.input_frame = frame,
            None => self.stats.retired = true,
        }
    }
}
impl<E: Endpoint, L: Planner, Q: ProcessorOperation> Processor<L, Q> for RemoteNode<E> {
    fn ports(&self) -> PortCounts {
        let midi = usize::from(self.descriptor.midi_capacity != 0);
        PortCounts {
            audio_inputs: self.descriptor.channels as usize,
            audio_outputs: self.descriptor.channels as usize,
            midi_inputs: midi,
            midi_outputs: midi,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, L>) -> Result<(), Error> {
        let d = self.descriptor;
        let q = d.quantum as usize;
        if block.frames > d.max_callback as usize
            || block.audio.input_count() != d.channels as usize
            || block.audio.output_count() != d.channels as usize
        {
            return Err(Error::InvalidShape);
        }
        if block.frame.0 != self.expected_frame {
            self.stats.retired = true;
        }
        match block.frame.0.checked_add(block.frames as u64) {
            Some(next) => self.expected_frame = next,
            None => self.stats.retired = true,
        }
        if self.stats.retired {
            self.output.audio_mut().fill(0.);
            self.output.clear_midi();
            self.output.set_header(Header {
                midi_reset: true,
                ..Header::default()
            });
        }
        for _ in 0..d.result_budget {
            let Some(lease) = self.endpoint.receive() else {
                break;
            };
            if !self.stats.retired && self.returns.accept(&lease).is_ok() {
                self.stats.received += 1;
            } else {
                self.stats.rejected += 1;
            }
            self.endpoint.release_output(lease);
        }
        let mut event = 0;
        for i in 0..block.frames {
            let frame = block
                .frame
                .0
                .checked_add(i as u64)
                .ok_or(Error::InvalidFrame)?;
            if !self.stats.retired {
                for c in 0..d.channels as usize {
                    self.input.audio_mut()[c * q + self.input_fill] =
                        block.audio.input(c).ok_or(Error::InvalidShape)?[i];
                }
                if let Some(midi) = block.midi_inputs.first() {
                    while let Some((offset, bytes)) = midi.get(event) {
                        if offset as usize != i {
                            break;
                        }
                        if self.input.header().midi_reset {
                            self.stats.midi_dropped += 1;
                        } else {
                            let e = MidiEvent::new(self.input_fill as u32, 0, bytes);
                            if e.ok().is_none_or(|e| self.input.push_midi(e).is_err()) {
                                let mut h = self.input.header();
                                h.midi_reset = true;
                                self.input.set_header(h);
                                self.input.clear_midi();
                                self.stats.midi_dropped += 1;
                            }
                        }
                        event += 1;
                    }
                }
                self.input_fill += 1;
                if self.input_fill == q {
                    self.submit();
                }
            }
            let active = frame >= d.start_frame.saturating_add(u64::from(d.latency));
            if active && self.output_fill == 0 {
                let ready = !self.stats.retired
                    && self
                        .returns
                        .consume_at(frame, self.endpoint.available(), &mut self.output)
                        .unwrap_or(false);
                if !ready {
                    self.output.audio_mut().fill(0.);
                    self.output.clear_midi();
                    self.output.set_header(Header {
                        midi_reset: true,
                        ..Header::default()
                    });
                    self.stats.fallback += 1;
                } else {
                    self.stats.processed += 1;
                    if self.output.header().concealed {
                        self.stats.concealed += 1;
                    }
                }
                self.observe(Observation::Delivery(Delivery {
                    frame: frame - u64::from(d.latency),
                    quality: if !ready {
                        Quality::Silence
                    } else if self.output.header().concealed {
                        Quality::Concealed
                    } else {
                        Quality::Processed
                    },
                }));
            }
            for c in 0..d.channels as usize {
                block.audio.output(c).ok_or(Error::InvalidShape)?[i] = if active {
                    self.output.audio()[c * q + self.output_fill]
                } else {
                    0.
                };
            }
            if active {
                if let Some(midi) = block.midi_outputs.first_mut() {
                    if self.output.header().midi_reset {
                        if !midi.is_recovery() {
                            midi.request_reset(i as u32, block.frames)?;
                            self.stats.midi_resets += 1;
                        }
                    } else if !midi.is_recovery() {
                        for event in self.output.midi() {
                            if event.frame as usize == self.output_fill
                                && event.bytes().ok().is_none_or(|bytes| {
                                    midi.push(i as u32, block.frames, bytes).is_err()
                                })
                            {
                                midi.request_reset(i as u32, block.frames)?;
                                self.stats.midi_dropped += 1;
                                break;
                            }
                        }
                    }
                }
                self.output_fill = (self.output_fill + 1) % q;
            }
        }
        self.observe(Observation::Stats(self.stats));
        Ok(())
    }
}
