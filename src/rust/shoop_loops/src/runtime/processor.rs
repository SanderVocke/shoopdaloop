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
    auxiliary: Option<audio::AudioRenderer>,
    midi: Option<midi::MidiRenderer>,
}
pub struct CaptureNode {
    binding: Binding,
    audio: Option<audio::AudioCapture>,
    auxiliary: Option<audio::AudioCapture>,
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
            auxiliary: if binding.auxiliary_channels > 0 {
                Some(audio::AudioRenderer::prepare(
                    binding.auxiliary_channels,
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
            auxiliary: if binding.auxiliary_channels > 0 {
                Some(audio::AudioCapture::prepare(
                    binding.auxiliary_channels,
                    frames,
                )?)
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
    fn process_references(
        &mut self,
        block: &mut rt_runtime::audio_refs::ReferenceBlock<'_, LoopPlanner>,
    ) -> Result<bool, Error> {
        match self {
            Self::Playback(node) => node.process_references(block),
            Self::Capture(_) => Ok(false),
        }
    }
    fn midi_buffer_settings(&self) -> Result<rt_runtime::processor::MidiBufferSettings, Error> {
        match self {
            Self::Playback(node) => node.midi_buffer_settings(),
            Self::Capture(node) => node.midi_buffer_settings(),
        }
    }
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
    fn process_references(
        &mut self,
        block: &mut rt_runtime::audio_refs::ReferenceBlock<'_, LoopPlanner>,
    ) -> Result<bool, Error> {
        use rt_runtime::audio_refs::{AudioSequence, GainEnvelope};
        let view = *block
            .domain
            .views
            .get(self.binding.target.slot() as usize)
            .ok_or(Error::InvalidId)?;
        if view.dry_wet || view.lead || view.raw_position != view.decision.state.position as i64 {
            return Ok(false);
        }
        let Some(renderer) = &mut self.audio else {
            return Ok(false);
        };
        if let Some(source) = view.audio {
            if !renderer.reference_ready(view.smoothing, view.decision.playback_discontinuity) {
                return Ok(false);
            }
            for channel in 0..self.binding.channels {
                let window = view.windows.audio[channel];
                if window.origin() != 0 || window.preplay != 0 {
                    return Ok(false);
                }
                let Some(sequence) =
                    block.playback(source, view.decision.state.position, channel)?
                else {
                    return Ok(false);
                };
                let balance = if view.audio_widths[0] == 2 {
                    if channel == 0 {
                        1. - view.balance.max(0.)
                    } else {
                        1. + view.balance.min(0.)
                    }
                } else {
                    1.
                };
                let gain = view.gain * balance * f32::from_bits(window.gain_bits);
                block.outputs[channel] = sequence.scaled(GainEnvelope::constant(gain));
            }
            renderer.set_smoothing(view.smoothing);
            for channel in 0..self.binding.channels {
                let sequence = block.outputs[channel];
                renderer.reference_last(channel, block.arena.last(sequence, block.resources)?);
                if block.resources.audio_render_mode() == rt_runtime::AudioRenderMode::Chunks
                    && sequence.gain != GainEnvelope::default()
                {
                    block.materialize(channel, sequence)?;
                }
            }
        } else {
            renderer.reference_silence();
            block.outputs[..self.binding.channels].fill(AudioSequence::default());
        }
        if let Some(auxiliary) = &mut self.auxiliary {
            auxiliary.reference_silence();
        }
        block.arena.work.forwarded += self.binding.channels as u64;
        Ok(true)
    }
    fn midi_buffer_settings(&self) -> Result<rt_runtime::processor::MidiBufferSettings, Error> {
        Ok(rt_runtime::processor::MidiBufferSettings {
            output: Some(crate::midi_restore::output_capacity(
                self.binding.midi_events,
                self.binding.midi_bytes,
            )?),
            ..Default::default()
        })
    }
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
            || row.binding.auxiliary_channels != self.binding.auxiliary_channels
            || row.binding.channels != self.binding.channels
            || row.binding.midi != self.binding.midi
        {
            return Err(Error::InvalidId);
        }
        Ok(())
    }

    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let slot = self.binding.target.slot() as usize;
        let view = block.domain.views.get(slot).ok_or(Error::InvalidId)?;
        if block.node != self.binding.playback
            || view.playback_node != self.binding.playback
            || view.capture_node != self.binding.capture
        {
            return Err(Error::InvalidId);
        }
        if let Some(audio) = &mut self.audio {
            audio.set_balance_channels(view.audio_widths[0])?;
            audio.set_smoothing(view.smoothing);
            let playback = audio::Playback {
                source: view.audio,
                position: view.decision.state.position,
                gain: view.gain,
                balance: view.balance,
                discontinuity: view.decision.playback_discontinuity,
            };
            if self.binding.channels > crate::windows::MAX_AUDIO_CHANNELS {
                audio.render_port_range(
                    &mut block.resources.render(),
                    playback,
                    &mut block.audio,
                    0,
                )?;
            } else {
                audio.render_windowed_port_range(
                    &mut block.resources.render(),
                    playback,
                    view.raw_position,
                    &view.windows.audio[if view.dry_wet {
                        view.audio_widths[1]
                    } else {
                        0
                    }..if view.dry_wet {
                        view.audio_widths[1]
                    } else {
                        0
                    } + self.binding.channels],
                    view.lead,
                    &mut block.audio,
                    0,
                )?;
            }
        }
        if let Some(audio) = &mut self.auxiliary {
            audio.set_balance_channels(view.audio_widths[1])?;
            audio.set_smoothing(view.smoothing);
            audio.render_windowed_port_range(
                &mut block.resources.render(),
                audio::Playback {
                    source: view.auxiliary,
                    position: view.decision.state.position,
                    gain: view.gain,
                    balance: view.balance,
                    discontinuity: view.decision.playback_discontinuity,
                },
                view.raw_position,
                &view.windows.audio[..self.binding.auxiliary_channels],
                view.lead,
                &mut block.audio,
                self.binding.channels,
            )?;
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
            audio_outputs: self.binding.channels + self.binding.auxiliary_channels,
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
            || row.binding.auxiliary_channels != self.binding.auxiliary_channels
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
            audio.stage_port_range(&mut block.resources.render(), interval, 0, &block.audio, 0)?;
        }
        if let (Some(audio), Some(interval)) =
            (&mut self.auxiliary, view.capture.filter(|_| view.dry_wet))
        {
            audio.stage_port_range(
                &mut block.resources.render(),
                interval,
                usize::from(self.binding.channels > 0),
                &block.audio,
                self.binding.channels,
            )?;
        }
        if let (Some(audio), Some((resource, revision))) = (&mut self.audio, view.replace) {
            match audio.replace_windowed_ports(
                &mut block.resources.render(),
                resource,
                revision,
                view.decision.state.position,
                view.decision.state.content_length,
                &view.windows.audio
                    [view.audio_widths[1]..view.audio_widths[1] + view.audio_widths[0]],
                &block.audio,
            ) {
                Ok(revision) => {
                    block.domain.rows[slot]
                        .as_mut()
                        .ok_or(Error::InvalidId)?
                        .active
                        .as_mut()
                        .ok_or(Error::InvalidState)?
                        .edit = Some((resource, revision))
                }
                Err(error @ (Error::Capacity | Error::Exhausted | Error::Budget)) => {
                    block.domain.rows[slot]
                        .as_mut()
                        .ok_or(Error::InvalidId)?
                        .failure = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        if let Some(midi) = &mut self.midi {
            if view.reset_input {
                midi.reset();
            }
            midi.process(
                &mut block.resources.render(),
                block.midi_inputs.first().ok_or(Error::InvalidShape)?,
                block.quantum.frames(),
                view.capture.map(|interval| {
                    (
                        interval,
                        usize::from(self.binding.channels > 0)
                            + usize::from(view.dry_wet && self.binding.auxiliary_channels > 0),
                    )
                }),
            )?;
            midi.publish_state(&mut block.domain.inputs[slot]);
        }
        block.domain.write_processed_capture(
            slot,
            block.frame,
            block.quantum.frames(),
            &block.audio,
            &mut block.resources,
        )?;
        Ok(())
    }
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: self.binding.channels + self.binding.auxiliary_channels,
            midi_inputs: usize::from(self.binding.midi),
            ..PortCounts::default()
        }
    }
}
