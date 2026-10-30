//! The engine with its record server, audio buffers and MIDI lanes, driven one
//! render quantum at a time.

use std::cell::RefCell;
use std::rc::Rc;

use crate::config::{ConfigError, WorkletConfig};
use crate::framing;
use crate::lane::{self, MidiEvent};
use crate::stream::{Batch, Stream};
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use shoop_rt_engine::{ControlEndpoint, HostStatus, PrepareError, Render, MAX_HOST_PORTS};
use shoop_rt_protocol::carrier::{CarrierError, RecordPort, RecordServer};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};

/// Lanes the main thread posts into.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Inbound {
    Records,
    Midi,
}

/// Lanes the module fills during a quantum for the main thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outbound {
    Records,
    Midi,
}

/// One render quantum as the AudioWorklet runs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantum {
    pub frames: usize,
    /// Host audio inputs the script copied in; the others are silent.
    pub input_channels: usize,
    /// `currentFrame` at the quantum's start.
    pub frame: u64,
    /// The previous quantum's processing time as a share of its period, where the
    /// script can measure it.
    pub load: Option<f32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuantumError {
    /// No frames, more than prepared, or more input channels than prepared.
    /// Nothing ran.
    Invalid,
    /// Rendering faulted. Output is silent; keep calling so the connection ends
    /// with `Disconnected(Fault)` and MIDI resets reach the outputs.
    Faulted(rt_runtime::Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrepareFailure {
    Config(ConfigError),
    Engine(PrepareError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counters {
    /// Quanta whose `currentFrame` did not continue the previous one.
    pub discontinuities: u32,
    /// Quanta whose reported load exceeded their period.
    pub overruns: u32,
    /// MIDI-in events for unknown ports, invalid or beyond the port's capacity.
    pub midi_in_dropped: u32,
    /// MIDI-in events delivered at a quantum's start because their frame passed.
    pub midi_in_late: u32,
    /// MIDI-out events that did not fit the lane.
    pub midi_out_dropped: u32,
    /// Record or MIDI-in streams discarded for an invalid length.
    pub framing_errors: u32,
}

fn bump(counter: &mut u32) {
    *counter = counter.saturating_add(1);
}

struct Records {
    inbox: Stream,
    outbox: Batch,
    framing_errors: u32,
}

/// The record lanes, shared by the record server, which owns a handle as its
/// port, and the host, which exposes their buffers to the script.
#[derive(Clone)]
struct RecordLanes(Rc<RefCell<Records>>);

impl RecordPort for RecordLanes {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        if self.0.borrow_mut().outbox.push(record) {
            Ok(())
        } else {
            Err(CarrierError::Full)
        }
    }

    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let mut records = self.0.borrow_mut();
        let pending = records.inbox.pending();
        match framing::record_len(pending) {
            Ok(None) => None,
            Ok(Some(len)) if len <= buf.len() => {
                buf[..len].copy_from_slice(&pending[..len]);
                records.inbox.consume(len);
                Some(len)
            }
            _ => {
                // The stream cannot be split any further. The invalid header makes
                // the server end the connection with a protocol error.
                buf[..2].copy_from_slice(&pending[..2]);
                records.inbox.clear();
                bump(&mut records.framing_errors);
                Some(2)
            }
        }
    }
}

/// The browser host: the engine, its record server and the buffers the
/// processor script copies through. Preparation allocates; nothing after it
/// does.
pub struct WorkletHost {
    server: RecordServer<ControlEndpoint, RecordLanes>,
    records: RecordLanes,
    render: Render,
    status: HostStatus,
    driver: DriverStatus,
    max_frames: usize,
    audio_inputs: usize,
    audio_outputs: usize,
    input: Box<[f32]>,
    output: Box<[f32]>,
    midi_inputs: Box<[MidiBuffer]>,
    midi_outputs: Box<[MidiBuffer]>,
    midi_in: Stream,
    midi_out: Batch,
    records_per_quantum: usize,
    next_frame: Option<u64>,
    counters: Counters,
    fault: Option<rt_runtime::Error>,
}

impl WorkletHost {
    pub fn prepare(config: &WorkletConfig) -> Result<Self, PrepareFailure> {
        config.validate().map_err(PrepareFailure::Config)?;
        let description = &config.engine;
        let driver = DriverStatus {
            kind: DriverKind::WebAudio,
            state: DriverRunState::Starting,
            sample_rate: description.sample_rate,
            buffer_frames: description.max_frames as u32,
        };
        let engine =
            shoop_rt_engine::prepare(description, driver).map_err(PrepareFailure::Engine)?;
        let outbox = engine.outbox();
        let midi = |count: usize| {
            (0..count)
                .map(|_| MidiBuffer::new(description.midi_events, description.midi_bytes))
                .collect::<Result<Box<[_]>, _>>()
                .map_err(|error| PrepareFailure::Engine(PrepareError::Runtime(error)))
        };
        let host = &description.host;
        let (audio_inputs, audio_outputs) = (host.audio_inputs.len(), host.audio_outputs.len());
        let frames = description.max_frames;
        let records = RecordLanes(Rc::new(RefCell::new(Records {
            inbox: Stream::new(config.records_in_bytes),
            outbox: Batch::new(config.records_out_bytes),
            framing_errors: 0,
        })));
        Ok(Self {
            server: RecordServer::new(engine.endpoint, records.clone(), outbox),
            records,
            render: engine.render,
            status: engine.host,
            driver,
            max_frames: frames,
            audio_inputs,
            audio_outputs,
            input: vec![0.; audio_inputs * frames].into_boxed_slice(),
            output: vec![0.; audio_outputs * frames].into_boxed_slice(),
            midi_inputs: midi(host.midi_inputs.len())?,
            midi_outputs: midi(host.midi_outputs.len())?,
            midi_in: Stream::new(config.midi_in_bytes),
            midi_out: Batch::new(config.midi_out_bytes),
            records_per_quantum: config.records_per_quantum,
            next_frame: None,
            counters: Counters::default(),
            fault: None,
        })
    }

    pub fn max_frames(&self) -> usize {
        self.max_frames
    }
    pub fn audio_inputs(&self) -> usize {
        self.audio_inputs
    }
    pub fn audio_outputs(&self) -> usize {
        self.audio_outputs
    }
    /// Host audio inputs for the next quantum, planar with a stride of
    /// [`WorkletHost::max_frames`].
    pub fn audio_input_mut(&mut self) -> &mut [f32] {
        &mut self.input
    }
    /// Host audio outputs of the last quantum, laid out like the inputs.
    pub fn audio_output(&self) -> &[f32] {
        &self.output
    }
    pub fn counters(&self) -> Counters {
        Counters {
            framing_errors: self
                .counters
                .framing_errors
                .saturating_add(self.records.0.borrow().framing_errors),
            ..self.counters
        }
    }
    pub fn fault(&self) -> Option<rt_runtime::Error> {
        self.fault
    }

    /// Append posted bytes to an inbound lane and return how many fit.
    pub fn write(&mut self, lane: Inbound, bytes: &[u8]) -> usize {
        self.with_inbound(lane, |stream| {
            let free = stream.free_mut();
            let count = free.len().min(bytes.len());
            free[..count].copy_from_slice(&bytes[..count]);
            stream.commit(count);
            count
        })
    }
    /// Read what an outbound lane collected during the last quantum.
    pub fn read<R>(&self, lane: Outbound, read: impl FnOnce(&[u8]) -> R) -> R {
        match lane {
            Outbound::Records => read(self.records.0.borrow().outbox.as_slice()),
            Outbound::Midi => read(self.midi_out.as_slice()),
        }
    }
    pub(crate) fn with_inbound<R>(&mut self, lane: Inbound, f: impl FnOnce(&mut Stream) -> R) -> R {
        match lane {
            Inbound::Records => f(&mut self.records.0.borrow_mut().inbox),
            Inbound::Midi => f(&mut self.midi_in),
        }
    }
    #[cfg(feature = "exports")]
    pub(crate) fn outbound_ptr(&self, lane: Outbound) -> (*const u8, usize) {
        match lane {
            Outbound::Records => {
                let records = self.records.0.borrow();
                (records.outbox.as_ptr(), records.outbox.capacity())
            }
            Outbound::Midi => (self.midi_out.as_ptr(), self.midi_out.capacity()),
        }
    }
    #[cfg(feature = "exports")]
    pub(crate) fn input_ptr(&mut self) -> *mut f32 {
        self.input.as_mut_ptr()
    }
    #[cfg(feature = "exports")]
    pub(crate) fn output_ptr(&self) -> *const f32 {
        self.output.as_ptr()
    }

    /// Deliver up to the configured number of client records, render one quantum
    /// and collect engine records and MIDI output for the script to post.
    pub fn process(&mut self, quantum: Quantum) -> Result<(), QuantumError> {
        let Quantum {
            frames,
            input_channels,
            frame,
            ..
        } = quantum;
        if frames == 0 || frames > self.max_frames || input_channels > self.audio_inputs {
            return Err(QuantumError::Invalid);
        }
        self.records.0.borrow_mut().outbox.clear();
        self.midi_out.clear();
        self.observe(quantum);
        for channel in self
            .input
            .chunks_exact_mut(self.max_frames)
            .skip(input_channels)
        {
            channel[..frames].fill(0.);
        }
        self.server.pump(self.records_per_quantum);
        self.deliver_midi(frame, frames);
        let result = self.render(frames);
        for input in self.midi_inputs.iter_mut() {
            input.clear();
        }
        self.collect_midi(frame);
        self.server.pump(0);
        self.records.0.borrow_mut().inbox.compact();
        result
    }

    fn observe(&mut self, quantum: Quantum) {
        if self.driver.state == DriverRunState::Starting {
            self.driver.state = DriverRunState::Running;
            self.status.set_driver(self.driver);
        }
        if self.next_frame.is_some_and(|next| next != quantum.frame) {
            bump(&mut self.counters.discontinuities);
            self.status.add_xruns(1);
        }
        self.next_frame = Some(quantum.frame.saturating_add(quantum.frames as u64));
        if let Some(load) = quantum.load.filter(|load| load.is_finite() && *load >= 0.) {
            self.status.set_load(load);
            if load > 1. {
                bump(&mut self.counters.overruns);
                self.status.add_xruns(1);
            }
        }
    }

    fn deliver_midi(&mut self, frame: u64, frames: usize) {
        let end = frame.saturating_add(frames as u64);
        let inputs = &mut self.midi_inputs;
        let counters = &mut self.counters;
        let delivered = self.midi_in.retain(|bytes| {
            let Some((event, len)) = lane::decode(bytes)? else {
                return Ok(None);
            };
            if event.frame >= end {
                return Ok(Some((len, true)));
            }
            if event.frame < frame {
                bump(&mut counters.midi_in_late);
            }
            let offset = event.frame.saturating_sub(frame) as u32;
            let accepted = inputs
                .get_mut(usize::from(event.port))
                .is_some_and(|input| input.insert(offset, frames, event.bytes).is_ok());
            if !accepted {
                bump(&mut counters.midi_in_dropped);
            }
            Ok(Some((len, false)))
        });
        if delivered.is_err() {
            bump(&mut self.counters.framing_errors);
        }
    }

    fn render(&mut self, frames: usize) -> Result<(), QuantumError> {
        let (input_count, output_count) = (self.audio_inputs, self.audio_outputs);
        let mut inputs: [&[f32]; MAX_HOST_PORTS] = [&[]; MAX_HOST_PORTS];
        for (slot, channel) in inputs
            .iter_mut()
            .zip(self.input.chunks_exact(self.max_frames))
        {
            *slot = &channel[..frames];
        }
        let mut outputs: [&mut [f32]; MAX_HOST_PORTS] = std::array::from_fn(|_| Default::default());
        for (slot, channel) in outputs
            .iter_mut()
            .zip(self.output.chunks_exact_mut(self.max_frames))
        {
            *slot = &mut channel[..frames];
        }
        let frame = self.render.next_frame();
        let result = self.render.process(
            AudioBlock {
                frame,
                frames,
                inputs: &inputs[..input_count],
                outputs: &mut outputs[..output_count],
            },
            MidiBlock {
                inputs: &self.midi_inputs,
                outputs: &mut self.midi_outputs,
            },
        );
        result.map(|_| ()).map_err(|error| {
            self.fault = Some(error);
            QuantumError::Faulted(error)
        })
    }

    fn collect_midi(&mut self, frame: u64) {
        for (port, output) in self.midi_outputs.iter().enumerate() {
            let recovery = self.render.midi_recovery(port);
            for buffer in std::iter::once(output).chain(recovery) {
                for index in 0..buffer.len() {
                    let Some((offset, bytes)) = buffer.get(index) else {
                        continue;
                    };
                    let event = MidiEvent {
                        frame: frame.saturating_add(u64::from(offset)),
                        port: port as u8,
                        bytes,
                    };
                    if !self.midi_out.write(|free| event.encode(free)) {
                        bump(&mut self.counters.midi_out_dropped);
                    }
                }
            }
        }
    }
}
