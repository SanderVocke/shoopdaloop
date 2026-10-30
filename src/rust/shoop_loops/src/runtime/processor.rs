use super::*;
use rt_runtime::{graph::PortCounts, processor::ProcessBlock};

/// A loop's playback or capture node, for wrapping in an application's node type.
pub enum LoopNode {
    Playback(PlaybackNode),
    Capture(CaptureNode),
}
/// Every prepared loop node with its runtime identity, including unassigned
/// node-pair templates that stay outside the initial graph.
pub struct LoopNodes(pub(super) Vec<(NodeId, LoopNode)>);
impl LoopNodes {
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &LoopNode)> {
        self.0.iter().map(|(id, node)| (*id, node))
    }
}
impl IntoIterator for LoopNodes {
    type Item = (NodeId, LoopNode);
    type IntoIter = std::vec::IntoIter<(NodeId, LoopNode)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
pub struct PlaybackNode {
    binding: Binding,
    audio: Option<audio::AudioRenderer>,
    midi: Option<midi::MidiRenderer>,
}
pub struct CaptureNode {
    binding: Binding,
    audio: Option<audio::AudioCapture>,
    midi: Option<midi::MidiCapture>,
}
impl PlaybackNode {
    pub(super) fn prepare(binding: Binding, frames: usize) -> Result<Self, Error> {
        Ok(Self {
            binding,
            audio: if binding.channels > 0 {
                Some(audio::AudioRenderer::prepare(
                    binding.channels,
                    frames,
                    binding.smoothing,
                )?)
            } else {
                None
            },
            midi: if binding.midi {
                Some(midi::MidiRenderer::prepare(frames, binding.packet_bytes)?)
            } else {
                None
            },
        })
    }
}
impl CaptureNode {
    pub(super) fn prepare(binding: Binding, frames: usize) -> Result<Self, Error> {
        Ok(Self {
            binding,
            audio: if binding.channels > 0 {
                Some(audio::AudioCapture::prepare(binding.channels, frames)?)
            } else {
                None
            },
            midi: if binding.midi {
                Some(midi::MidiCapture::prepare(
                    frames,
                    binding.midi_events,
                    binding.midi_bytes,
                )?)
            } else {
                None
            },
        })
    }
}
const CONSUMER: ConsumerAccess = ConsumerAccess {
    decision_required: true,
    observations: 1,
    operation_bytes: 0,
};
impl Processor<LoopPlanner> for LoopNode {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Playback(node) => node.ports(),
            Self::Capture(node) => node.ports(),
        }
    }
    fn planning(&self) -> ConsumerAccess {
        CONSUMER
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Playback(node) => node.process(block),
            Self::Capture(node) => node.process(block),
        }
    }
}
impl Processor<LoopPlanner> for PlaybackNode {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_outputs: self.binding.channels,
            midi_outputs: usize::from(self.binding.midi),
            ..PortCounts::default()
        }
    }
    fn planning(&self) -> ConsumerAccess {
        CONSUMER
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let view = block.decision.ok_or(Error::InvalidState)?.value;
        if view.decision.state.id.slot() != self.binding.target.slot()
            || view.playback_node != self.binding.playback
            || view.capture_node != self.binding.capture
        {
            return Err(Error::InvalidId);
        }
        if let Some(audio) = &mut self.audio {
            audio.set_smoothing(view.smoothing);
            audio.render_ports(
                &mut block.resources.render(),
                audio::Playback {
                    source: view.audio,
                    position: view.decision.state.position,
                    gain: view.gain,
                    balance: view.balance,
                    discontinuity: view.decision.playback_discontinuity,
                },
                &mut block.audio,
            )?;
        }
        let error = if let Some(midi) = &mut self.midi {
            midi.render(
                &mut block.resources.render(),
                midi::Playback {
                    source: view.midi,
                    position: view.decision.state.position,
                    discontinuity: view.decision.playback_discontinuity,
                },
                block.frames,
                block.midi_outputs.get_mut(0).ok_or(Error::InvalidShape)?,
            )?
            .output_error
        } else {
            None
        };
        block.observations.push(Observed::Playback {
            target: view.decision.state.id,
            overflow: error.is_some(),
        })?;
        error.map_or(Ok(()), Err)
    }
}
impl Processor<LoopPlanner> for CaptureNode {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: self.binding.channels,
            midi_inputs: usize::from(self.binding.midi),
            ..PortCounts::default()
        }
    }
    fn planning(&self) -> ConsumerAccess {
        CONSUMER
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let view = block.decision.ok_or(Error::InvalidState)?.value;
        if view.decision.state.id.slot() != self.binding.target.slot()
            || view.playback_node != self.binding.playback
            || view.capture_node != self.binding.capture
        {
            return Err(Error::InvalidId);
        }
        if let (Some(audio), Some(interval)) = (&mut self.audio, view.capture) {
            audio.stage_ports(&mut block.resources.render(), interval, 0, &block.audio)?;
        }
        if let Some(midi) = &mut self.midi {
            if view.reset_input {
                midi.reset();
            }
            midi.process(
                &mut block.resources.render(),
                block.midi_inputs.first().ok_or(Error::InvalidShape)?,
                block.frames,
                view.capture
                    .map(|interval| (interval, usize::from(self.binding.channels > 0))),
            )?;
            block.observations.push(Observed::Input {
                target: view.decision.state.id,
                state: *midi.state(),
            })?;
        } else {
            block.observations.push(Observed::NoMidiInput {
                target: view.decision.state.id,
            })?;
        }
        Ok(())
    }
}
