use rt_runtime::buffer::{AudioPorts, MidiBuffer};
use rt_runtime::planner::Planner;
use rt_runtime::processor::ProcessBlock;
use rt_runtime::resource::ResourceIo;

pub(crate) struct SignalBlock<'a, 'b> {
    pub frames: usize,
    pub callback: Option<rt_runtime::Frame>,
    pub audio: &'a mut AudioPorts<'b>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_outputs: &'a mut [MidiBuffer],
    pub resources: &'a mut ResourceIo<'b>,
}
impl<'a, 'b> SignalBlock<'a, 'b> {
    pub fn interval<L: Planner>(b: &'a mut ProcessBlock<'b, L>) -> Self {
        Self {
            frames: b.frames,
            callback: None,
            audio: &mut b.audio,
            midi_inputs: b.midi_inputs,
            midi_outputs: b.midi_outputs,
            resources: &mut b.resources,
        }
    }
    pub fn fixed(b: &'a mut rt_runtime::fixed::Block<'b>) -> Self {
        Self {
            frames: b.tick.quantum.frames(),
            callback: Some(b.callback_frame),
            audio: &mut b.audio,
            midi_inputs: b.midi_inputs,
            midi_outputs: b.midi_outputs,
            resources: &mut b.resources,
        }
    }
}
