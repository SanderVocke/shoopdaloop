use super::*;
use rt_runtime::{graph::PortCounts, processor::ProcessBlock};

pub enum Node<P> {
    Playback(PlaybackNode),
    Capture(CaptureNode),
    Effect(P),
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
impl<P> Node<P> {
    pub(super) fn playback(binding: Binding, frames: usize) -> Result<Self, Error> {
        Ok(Self::Playback(PlaybackNode {
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
        }))
    }
    pub(super) fn capture(binding: Binding, frames: usize) -> Result<Self, Error> {
        Ok(Self::Capture(CaptureNode {
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
        }))
    }
}
impl<P: Processor<LoopPlanner>> Processor<LoopPlanner> for Node<P> {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Playback(node) => PortCounts {
                audio_outputs: node.binding.channels,
                midi_outputs: usize::from(node.binding.midi),
                ..PortCounts::default()
            },
            Self::Capture(node) => PortCounts {
                audio_inputs: node.binding.channels,
                midi_inputs: usize::from(node.binding.midi),
                ..PortCounts::default()
            },
            Self::Effect(node) => node.ports(),
        }
    }
    fn planning(&self) -> ConsumerAccess {
        match self {
            Self::Effect(node) => node.planning(),
            _ => ConsumerAccess {
                decision_required: true,
                observations: 1,
                operation_bytes: 0,
            },
        }
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        match self {
            Self::Effect(node) => node.set_parameter(parameter, value),
            _ => Err(Error::InvalidParameter),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Effect(node) => node.process(block),
            Self::Playback(node) => {
                let view = block.decision.ok_or(Error::InvalidState)?.value;
                if view.decision.state.id != node.binding.target {
                    return Err(Error::InvalidId);
                }
                if let Some(audio) = &mut node.audio {
                    audio.render_ports(
                        &mut block.resources.render(),
                        audio::Playback {
                            source: view.audio,
                            position: view.decision.state.position,
                            gain: view.gain,
                            discontinuity: view.decision.playback_discontinuity,
                        },
                        &mut block.audio,
                    )?;
                }
                let error = if let Some(midi) = &mut node.midi {
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
                    target: node.binding.target,
                    overflow: error.is_some(),
                })?;
                error.map_or(Ok(()), Err)
            }
            Self::Capture(node) => {
                let view = block.decision.ok_or(Error::InvalidState)?.value;
                if view.decision.state.id != node.binding.target {
                    return Err(Error::InvalidId);
                }
                if let (Some(audio), Some(interval)) = (&mut node.audio, view.capture) {
                    audio.stage_ports(&mut block.resources.render(), interval, 0, &block.audio)?;
                }
                if let Some(midi) = &mut node.midi {
                    midi.process(
                        &mut block.resources.render(),
                        block.midi_inputs.first().ok_or(Error::InvalidShape)?,
                        block.frames,
                        view.capture
                            .map(|interval| (interval, usize::from(node.binding.channels > 0))),
                    )?;
                    block.observations.push(Observed::Input {
                        target: node.binding.target,
                        state: *midi.state(),
                    })?;
                } else {
                    block.observations.push(Observed::NoMidiInput {
                        target: node.binding.target,
                    })?;
                }
                Ok(())
            }
        }
    }
}
