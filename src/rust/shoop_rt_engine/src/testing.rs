//! A deterministic offline host: callbacks are pulled synchronously with given
//! inputs and the control endpoint is pumped through the in-process carrier. No
//! threads and no real time are involved.
//!
//! [`OfflineHost::process`], [`OfflineHost::request`] and [`OfflineHost::receive`]
//! do not allocate, so callers can guard them with an allocator check.

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

pub struct OfflineHost {
    pub client: ProtocolClient<InProcess<ControlEndpoint>>,
    pub render: Render,
    pub host: HostStatus,
    inputs: Box<[Box<[f32]>]>,
    outputs: Box<[Box<[f32]>]>,
    midi_inputs: Box<[MidiBuffer]>,
    midi_outputs: Box<[MidiBuffer]>,
    max_frames: usize,
}

impl OfflineHost {
    pub fn new(description: &EngineDescription) -> Result<Self, PrepareError> {
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
        Ok(Self {
            client: ProtocolClient::new(InProcess::new(engine.endpoint, outbox)),
            render: engine.render,
            host: engine.host,
            inputs: buffers(host.audio_inputs.len()),
            outputs: buffers(host.audio_outputs.len()),
            midi_inputs: midi(host.midi_inputs.len())?,
            midi_outputs: midi(host.midi_outputs.len())?,
            max_frames: frames,
        })
    }

    /// Drop the client and its control endpoint, keeping the render owner.
    pub fn into_render(self) -> Render {
        self.render
    }
    pub fn endpoint(&mut self) -> &mut ControlEndpoint {
        self.client.carrier_mut().endpoint_mut()
    }
    pub fn next_frame(&self) -> u64 {
        self.render.next_frame().0
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
