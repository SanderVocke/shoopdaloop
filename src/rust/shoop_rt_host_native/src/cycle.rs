//! The render owner with the host's buffers and MIDI bridges: one engine callback
//! at a time, on whichever thread the driver renders.

use std::sync::Arc;

use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use shoop_rt_engine::{EngineDescription, Phase, Render, MAX_HOST_PORTS};

use crate::midi::{MidiInputReceiver, MidiOutputSender};
use crate::status::Shared;

pub(crate) struct Cycle {
    render: Render,
    shared: Arc<Shared>,
    inputs: Box<[Box<[f32]>]>,
    outputs: Box<[Box<[f32]>]>,
    midi_inputs: Box<[MidiBuffer]>,
    midi_outputs: Box<[MidiBuffer]>,
    receivers: Box<[Option<MidiInputReceiver>]>,
    senders: Box<[Option<MidiOutputSender>]>,
    max_frames: usize,
    phase: Phase,
}

impl Cycle {
    /// `receivers` and `senders` follow the description's MIDI ports; ports without
    /// one are silent or discarded.
    pub fn new(
        render: Render,
        shared: Arc<Shared>,
        description: &EngineDescription,
        mut receivers: Vec<Option<MidiInputReceiver>>,
        mut senders: Vec<Option<MidiOutputSender>>,
    ) -> Result<Self, rt_runtime::Error> {
        let host = &description.host;
        let frames = description.max_frames;
        let audio = |count: usize| {
            (0..count)
                .map(|_| vec![0.; frames].into_boxed_slice())
                .collect::<Box<[_]>>()
        };
        let midi = |count: usize| {
            (0..count)
                .map(|_| MidiBuffer::new(description.midi_events, description.midi_bytes))
                .collect::<Result<Box<[_]>, _>>()
        };
        receivers.resize_with(host.midi_inputs.len(), || None);
        senders.resize_with(host.midi_outputs.len(), || None);
        Ok(Self {
            render,
            shared,
            inputs: audio(host.audio_inputs.len()),
            outputs: audio(host.audio_outputs.len()),
            midi_inputs: midi(host.midi_inputs.len())?,
            midi_outputs: midi(host.midi_outputs.len())?,
            receivers: receivers.into_boxed_slice(),
            senders: senders.into_boxed_slice(),
            max_frames: frames,
            phase: Phase::Running,
        })
    }

    pub fn max_frames(&self) -> usize {
        self.max_frames
    }
    pub fn next_frame(&self) -> u64 {
        self.render.next_frame().0
    }
    /// Host audio inputs for the next callback, `max_frames` each.
    pub fn inputs_mut(&mut self) -> &mut [Box<[f32]>] {
        &mut self.inputs
    }
    /// Host audio outputs of the last callback.
    pub fn outputs(&self) -> &[Box<[f32]>] {
        &self.outputs
    }

    /// Render one engine callback of `frames`, at most `max_frames`, from the current
    /// inputs. With `take_midi`, queued MIDI input is placed in this block as if it
    /// starts at host time `now`. Output MIDI and the engine's MIDI recovery go to the
    /// output bridges. After a render fault the outputs are silent.
    pub fn render(&mut self, frames: usize, now: u64, take_midi: bool) {
        let frames = frames.min(self.max_frames);
        if frames == 0 {
            return;
        }
        if take_midi {
            for (receiver, buffer) in self.receivers.iter_mut().zip(self.midi_inputs.iter_mut()) {
                if let Some(receiver) = receiver {
                    receiver.drain(buffer, frames, now, &self.shared);
                }
            }
        }
        for output in self.midi_outputs.iter_mut() {
            output.clear();
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
        let result = self.render.process(
            AudioBlock {
                frame: self.render.next_frame(),
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
        self.phase = match result {
            Ok(report) => report.phase,
            Err(error) => {
                for output in self.outputs.iter_mut() {
                    output[..frames].fill(0.);
                }
                Phase::Faulted(error)
            }
        };
        for (port, sender) in self.senders.iter_mut().enumerate() {
            if let Some(sender) = sender {
                sender.send(&self.midi_outputs[port], &self.shared);
                if let Some(recovery) = self.render.midi_recovery(port) {
                    sender.send(recovery, &self.shared);
                }
            }
        }
        self.shared
            .set_render(self.phase, self.render.next_frame().0);
    }

    pub fn into_parts(self) -> (Render, Phase) {
        (self.render, self.phase)
    }
}
