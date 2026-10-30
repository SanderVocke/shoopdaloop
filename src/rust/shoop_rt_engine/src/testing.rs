//! A deterministic offline host: callbacks are pulled synchronously with given
//! inputs and the control endpoint is pumped through the in-process carrier. No
//! threads and no real time are involved.
//!
//! [`OfflineRender`] is the host's render half; [`OfflineRender::new`] also returns
//! an unconnected client, for an adapter that owns the client. [`OfflineHost`] keeps
//! both together.
//!
//! [`OfflineRender::process`], [`OfflineHost::request`] and [`OfflineHost::receive`]
//! do not allocate, so callers can guard them with an allocator check.

use std::ops::{Deref, DerefMut};
use std::time::Duration;

use crate::description::MAX_HOST_PORTS;
use crate::{
    prepare, ControlEndpoint, EngineDescription, HostStatus, PrepareError, Render, Report,
};
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::Error;
use shoop_rt_protocol::carrier::InProcess;
use shoop_rt_protocol::client::{ClientError, ProtocolClient};
use shoop_rt_protocol::{
    BulkRead, DriverKind, DriverRunState, DriverStatus, EngineMessage, Request,
};

/// A client of the engine through the in-process carrier.
pub type OfflineClient = ProtocolClient<InProcess<ControlEndpoint>>;

/// The render owner, host status and host buffers of an offline host.
pub struct OfflineRender {
    pub render: Render,
    pub host: HostStatus,
    inputs: Box<[Box<[f32]>]>,
    outputs: Box<[Box<[f32]>]>,
    midi_inputs: Box<[MidiBuffer]>,
    midi_outputs: Box<[MidiBuffer]>,
    max_frames: usize,
    sample_rate: u32,
    /// Time left over by [`OfflineRender::run_for`], in nanoseconds times the sample rate.
    remainder: u128,
}

impl OfflineRender {
    /// Prepares an engine and returns its render half and an unconnected client.
    pub fn new(description: &EngineDescription) -> Result<(Self, OfflineClient), PrepareError> {
        let driver = DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: description.sample_rate,
            buffer_frames: description.max_frames as u32,
        };
        let engine = prepare(description, driver)?;
        let outbox = engine.outbox();
        let frames = description.max_frames;
        let buffers = |count: usize| {
            (0..count)
                .map(|_| vec![0.; frames].into_boxed_slice())
                .collect::<Box<[_]>>()
        };
        let midi = |count: usize| {
            (0..count)
                .map(|_| MidiBuffer::new(description.midi_events, description.midi_bytes))
                .collect::<Result<Box<[_]>, _>>()
        };
        let host = &description.host;
        let render = Self {
            render: engine.render,
            host: engine.host,
            inputs: buffers(host.audio_inputs.len()),
            outputs: buffers(host.audio_outputs.len()),
            midi_inputs: midi(host.midi_inputs.len())?,
            midi_outputs: midi(host.midi_outputs.len())?,
            max_frames: frames,
            sample_rate: description.sample_rate,
            remainder: 0,
        };
        let client = ProtocolClient::new(InProcess::new(engine.endpoint, outbox));
        Ok((render, client))
    }

    pub fn into_render(self) -> Render {
        self.render
    }
    pub fn next_frame(&self) -> u64 {
        self.render.next_frame().0
    }
    pub fn max_frames(&self) -> usize {
        self.max_frames
    }
    /// Samples of a host audio input for the next callback.
    pub fn input_mut(&mut self, port: usize) -> &mut [f32] {
        &mut self.inputs[port]
    }
    /// Samples of a host audio output from the last callback.
    pub fn output(&self, port: usize) -> &[f32] {
        &self.outputs[port]
    }
    /// Events of a host MIDI input for the next callback; cleared after it.
    pub fn midi_input_mut(&mut self, port: usize) -> &mut MidiBuffer {
        &mut self.midi_inputs[port]
    }
    pub fn midi_output(&self, port: usize) -> &MidiBuffer {
        &self.midi_outputs[port]
    }

    /// Render one callback of `frames` frames from the current inputs.
    pub fn process(&mut self, frames: usize) -> Result<Report, Error> {
        if frames > self.max_frames {
            return Err(Error::InvalidShape);
        }
        let mut inputs: [&[f32]; MAX_HOST_PORTS] = [&[]; MAX_HOST_PORTS];
        for (slot, input) in inputs.iter_mut().zip(self.inputs.iter()) {
            *slot = &input[..frames];
        }
        let output_count = self.outputs.len();
        let mut outputs: [&mut [f32]; MAX_HOST_PORTS] = std::array::from_fn(|_| Default::default());
        for (slot, output) in outputs.iter_mut().zip(self.outputs.iter_mut()) {
            *slot = &mut output[..frames];
        }
        let frame = self.render.next_frame();
        let report = self.render.process(
            AudioBlock {
                frame,
                frames,
                inputs: &inputs[..self.inputs.len()],
                outputs: &mut outputs[..output_count],
            },
            MidiBlock {
                inputs: &self.midi_inputs,
                outputs: &mut self.midi_outputs,
            },
        );
        for input in self.midi_inputs.iter_mut() {
            input.clear();
        }
        report
    }

    /// Render `frames` frames in callbacks of at most the prepared size, with
    /// `input(port, frame)` on the host audio inputs.
    pub fn run(&mut self, frames: usize, input: impl Fn(usize, u64) -> f32) -> Result<(), Error> {
        let mut remaining = frames;
        while remaining > 0 {
            let count = remaining.min(self.max_frames);
            let start = self.next_frame();
            for (port, samples) in self.inputs.iter_mut().enumerate() {
                for (offset, sample) in samples[..count].iter_mut().enumerate() {
                    *sample = input(port, start + offset as u64);
                }
            }
            self.process(count)?;
            remaining -= count;
        }
        Ok(())
    }

    /// Render the whole frames that `elapsed` covers at the sample rate; the rest
    /// carries over to the next call.
    pub fn run_for(
        &mut self,
        elapsed: Duration,
        input: impl Fn(usize, u64) -> f32,
    ) -> Result<(), Error> {
        const NANOS: u128 = 1_000_000_000;
        let total = self.remainder + elapsed.as_nanos() * u128::from(self.sample_rate);
        self.remainder = total % NANOS;
        self.run((total / NANOS) as usize, input)
    }
}

/// An offline host with its client.
pub struct OfflineHost {
    pub client: OfflineClient,
    offline: OfflineRender,
}

impl Deref for OfflineHost {
    type Target = OfflineRender;
    fn deref(&self) -> &OfflineRender {
        &self.offline
    }
}

impl DerefMut for OfflineHost {
    fn deref_mut(&mut self) -> &mut OfflineRender {
        &mut self.offline
    }
}

impl OfflineHost {
    pub fn new(description: &EngineDescription) -> Result<Self, PrepareError> {
        let (offline, client) = OfflineRender::new(description)?;
        Ok(Self { client, offline })
    }

    /// Drop the client and its control endpoint, keeping the render owner.
    pub fn into_render(self) -> Render {
        self.offline.render
    }
    pub fn into_parts(self) -> (OfflineRender, OfflineClient) {
        (self.offline, self.client)
    }
    pub fn endpoint(&mut self) -> &mut ControlEndpoint {
        self.client.carrier_mut().endpoint_mut()
    }

    pub fn connect(&mut self) -> Result<(), ClientError> {
        self.client.connect()
    }
    pub fn request(&mut self, request: Request) -> Result<u64, ClientError> {
        self.client.request(request)
    }
    pub fn bulk(&mut self, read: BulkRead) -> Result<(), ClientError> {
        self.client.bulk(read)
    }
    /// The next engine message, polling the endpoint when its outbox is empty.
    pub fn receive(&mut self) -> Option<EngineMessage> {
        self.client.poll()
    }
}
