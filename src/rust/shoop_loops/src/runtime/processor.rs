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
    type IntoIter = alloc::vec::IntoIter<(NodeId, LoopNode)>;
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
impl Processor<LoopPlanner> for LoopNode {
    fn admit(&self, node: NodeId, domain: &LoopPlanner) -> Result<(), Error> {
        match self {
            Self::Playback(processor) => processor.admit(node, domain),
            Self::Capture(processor) => processor.admit(node, domain),
        }
    }

    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Playback(node) => node.process(block),
            Self::Capture(node) => node.process(block),
        }
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Playback(node) => node.ports(),
            Self::Capture(node) => node.ports(),
        }
    }
}
impl Processor<LoopPlanner> for PlaybackNode {
    fn admit(&self, node: NodeId, domain: &LoopPlanner) -> Result<(), Error> {
        let row = domain
            .rows
            .get(self.binding.target.slot() as usize)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidId)?;
        if node != self.binding.playback
            || row.binding.playback != self.binding.playback
            || row.binding.capture != self.binding.capture
            || row.binding.target.slot() != self.binding.target.slot()
            || row.binding.channels != self.binding.channels
            || row.binding.midi != self.binding.midi
        {
            return Err(Error::InvalidId);
        }
        Ok(())
    }

    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let slot = self.binding.target.slot() as usize;
        let view = *block.domain.views.get(slot).ok_or(Error::InvalidId)?;
        if block.node != self.binding.playback
            || view.playback_node != self.binding.playback
            || view.capture_node != self.binding.capture
        {
            return Err(Error::InvalidId);
        }
        if let Some(audio) = &mut self.audio {
            audio.set_smoothing(view.smoothing);
            let playback = audio::Playback {
                source: view.audio,
                position: view.decision.state.position,
                gain: view.gain,
                balance: view.balance,
                discontinuity: view.decision.playback_discontinuity,
            };
            if self.binding.channels > crate::windows::MAX_AUDIO_CHANNELS {
                audio.render_ports(&mut block.resources.render(), playback, &mut block.audio)?;
            } else {
                audio.render_windowed_ports(
                    &mut block.resources.render(),
                    playback,
                    view.raw_position,
                    &view.windows.audio[..self.binding.channels],
                    view.lead,
                    &mut block.audio,
                )?;
            }
        }
        let error = if let Some(midi) = &mut self.midi {
            midi.render_windowed(
                &mut block.resources.render(),
                midi::Playback {
                    source: view.midi,
                    position: view.decision.state.position,
                    discontinuity: view.decision.playback_discontinuity,
                },
                view.raw_position,
                view.windows.midi,
                view.lead,
                block.quantum.frames(),
                block.midi_outputs.first_mut().ok_or(Error::InvalidShape)?,
            )?
            .output_error
        } else {
            None
        };
        if error.is_some() {
            let row = block.domain.rows[slot].as_mut().ok_or(Error::InvalidId)?;
            row.overflows = row.overflows.saturating_add(1);
        }
        error.map_or(Ok(()), Err)
    }
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_outputs: self.binding.channels,
            midi_outputs: usize::from(self.binding.midi),
            ..PortCounts::default()
        }
    }
}
impl Processor<LoopPlanner> for CaptureNode {
    fn admit(&self, node: NodeId, domain: &LoopPlanner) -> Result<(), Error> {
        let row = domain
            .rows
            .get(self.binding.target.slot() as usize)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidId)?;
        if node != self.binding.capture
            || row.binding.playback != self.binding.playback
            || row.binding.capture != self.binding.capture
            || row.binding.target.slot() != self.binding.target.slot()
            || row.binding.channels != self.binding.channels
            || row.binding.midi != self.binding.midi
        {
            return Err(Error::InvalidId);
        }
        Ok(())
    }

    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let slot = self.binding.target.slot() as usize;
        let view = *block.domain.views.get(slot).ok_or(Error::InvalidId)?;
        if block.node != self.binding.capture
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
                block.quantum.frames(),
                view.capture
                    .map(|interval| (interval, usize::from(self.binding.channels > 0))),
            )?;
            block.domain.inputs[slot] = *midi.state();
        }
        Ok(())
    }
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: self.binding.channels,
            midi_inputs: usize::from(self.binding.midi),
            ..PortCounts::default()
        }
    }
}
