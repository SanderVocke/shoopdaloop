//! Portable processor and role conversion for supported session tracks.
use super::*;
use shoop_processor_protocol::{state::State, Kind};

pub(super) fn mode_to_document(mode: image::ChannelMode) -> ChannelModeDocument {
    match mode {
        image::ChannelMode::Direct => ChannelModeDocument::Direct,
        image::ChannelMode::Dry => ChannelModeDocument::Dry,
        image::ChannelMode::Wet => ChannelModeDocument::Wet,
    }
}
pub(super) fn mode_from_document(mode: ChannelModeDocument) -> Result<image::ChannelMode> {
    Ok(match mode {
        ChannelModeDocument::Direct => image::ChannelMode::Direct,
        ChannelModeDocument::Dry => image::ChannelMode::Dry,
        ChannelModeDocument::Wet => image::ChannelMode::Wet,
        _ => bail!("disabled channels are unsupported"),
    })
}
pub(super) fn chain_kind(chain: FxChainTypeDocument) -> Result<Kind> {
    match chain {
        FxChainTypeDocument::BuiltInFx => Ok(Kind::BuiltInFx),
        FxChainTypeDocument::OxiSynth => Ok(Kind::OxiSynth),
        _ => bail!("unsupported processor type (P07)"),
    }
}
pub(super) fn canonical_state(chain: FxChainTypeDocument, encoded: &str) -> Result<String> {
    Ok(State::decode(chain_kind(chain)?, encoded)?.encode())
}
pub(super) fn image_topology(
    topology: &TrackTopologyDocument,
) -> Result<(TrackTopology, u32, bool)> {
    let (dry, wet, midi, processor_type) = match *topology {
        TrackTopologyDocument::Direct {
            audio_channels,
            midi,
        } => {
            ensure!(audio_channels <= 8, "track exceeds channel capacity");
            return Ok((TrackTopology::Direct, audio_channels, midi));
        }
        TrackTopologyDocument::Trigger => return Ok((TrackTopology::Direct, 0, false)),
        TrackTopologyDocument::DryWetExternal {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } => (
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
            TrackProcessorTypeId::EXTERNAL,
        ),
        TrackTopologyDocument::BuiltInFx { audio_channels } => {
            ensure!(audio_channels > 0, "FX needs audio channels");
            (
                audio_channels,
                audio_channels,
                true,
                TrackProcessorTypeId::BUILTIN_FX,
            )
        }
        TrackTopologyDocument::OxiSynth => (2, 2, true, TrackProcessorTypeId::OXISYNTH),
        _ => bail!("unsupported track processor (P07)"),
    };
    ensure!(
        dry <= 8 && wet <= 8 && (dry != 0 || wet != 0 || midi),
        "invalid dry/wet shape"
    );
    Ok((
        TrackTopology::DryWet {
            dry_audio_channels: dry,
            wet_audio_channels: wet,
            dry_midi: midi,
            processor_type: TrackProcessorTypeId::new(processor_type),
        },
        wet,
        midi,
    ))
}
pub(super) fn document_topology(track: &image::Track) -> Result<TrackTopologyDocument> {
    Ok(match &track.topology {
        TrackTopology::Direct if track.audio_channels == 0 && !track.midi => {
            TrackTopologyDocument::Trigger
        }
        TrackTopology::Direct => TrackTopologyDocument::Direct {
            audio_channels: track.audio_channels,
            midi: track.midi,
        },
        TrackTopology::DryWet {
            dry_audio_channels: dry,
            wet_audio_channels: wet,
            dry_midi: midi,
            processor_type,
        } => {
            ensure!(
                track.audio_channels == *wet && track.midi == *midi,
                "inconsistent track topology"
            );
            match processor_type.as_str() {
                TrackProcessorTypeId::EXTERNAL => TrackTopologyDocument::DryWetExternal {
                    dry_audio_channels: *dry,
                    wet_audio_channels: *wet,
                    dry_midi: *midi,
                },
                TrackProcessorTypeId::BUILTIN_FX => {
                    ensure!(*dry > 0 && dry == wet && *midi, "invalid FX shape");
                    TrackTopologyDocument::BuiltInFx {
                        audio_channels: *dry,
                    }
                }
                TrackProcessorTypeId::OXISYNTH => {
                    ensure!(*dry == 2 && *wet == 2 && *midi, "invalid synth shape");
                    TrackTopologyDocument::OxiSynth
                }
                _ => bail!("unsupported track processor (P07)"),
            }
        }
    })
}
pub(super) fn fx_to_document(parameter: BuiltInFxParameter) -> BuiltInFxParameterDocument {
    match parameter {
        BuiltInFxParameter::CompressorThreshold => BuiltInFxParameterDocument::CompressorThreshold,
        BuiltInFxParameter::CompressorRatio => BuiltInFxParameterDocument::CompressorRatio,
        BuiltInFxParameter::CompressorAttack => BuiltInFxParameterDocument::CompressorAttack,
        BuiltInFxParameter::CompressorRelease => BuiltInFxParameterDocument::CompressorRelease,
        BuiltInFxParameter::CompressorMakeup => BuiltInFxParameterDocument::CompressorMakeup,
        BuiltInFxParameter::Drive => BuiltInFxParameterDocument::Drive,
        BuiltInFxParameter::DriveTone => BuiltInFxParameterDocument::DriveTone,
        BuiltInFxParameter::DriveMix => BuiltInFxParameterDocument::DriveMix,
        BuiltInFxParameter::DriveOutput => BuiltInFxParameterDocument::DriveOutput,
        BuiltInFxParameter::EqLow => BuiltInFxParameterDocument::EqLow,
        BuiltInFxParameter::EqMid => BuiltInFxParameterDocument::EqMid,
        BuiltInFxParameter::EqHigh => BuiltInFxParameterDocument::EqHigh,
        BuiltInFxParameter::ChorusRate => BuiltInFxParameterDocument::ChorusRate,
        BuiltInFxParameter::ChorusDepth => BuiltInFxParameterDocument::ChorusDepth,
        BuiltInFxParameter::ChorusMix => BuiltInFxParameterDocument::ChorusMix,
        BuiltInFxParameter::ChorusWidth => BuiltInFxParameterDocument::ChorusWidth,
        BuiltInFxParameter::ModulationRate => BuiltInFxParameterDocument::ModulationRate,
        BuiltInFxParameter::ModulationDepth => BuiltInFxParameterDocument::ModulationDepth,
        BuiltInFxParameter::ModulationMix => BuiltInFxParameterDocument::ModulationMix,
        BuiltInFxParameter::ModulationFeedback => BuiltInFxParameterDocument::ModulationFeedback,
        BuiltInFxParameter::ModulationSpread => BuiltInFxParameterDocument::ModulationSpread,
        BuiltInFxParameter::ReverbAmount => BuiltInFxParameterDocument::ReverbAmount,
        BuiltInFxParameter::ReverbTone => BuiltInFxParameterDocument::ReverbTone,
    }
}
pub(super) fn fx_from_document(parameter: BuiltInFxParameterDocument) -> BuiltInFxParameter {
    match parameter {
        BuiltInFxParameterDocument::CompressorThreshold => BuiltInFxParameter::CompressorThreshold,
        BuiltInFxParameterDocument::CompressorRatio => BuiltInFxParameter::CompressorRatio,
        BuiltInFxParameterDocument::CompressorAttack => BuiltInFxParameter::CompressorAttack,
        BuiltInFxParameterDocument::CompressorRelease => BuiltInFxParameter::CompressorRelease,
        BuiltInFxParameterDocument::CompressorMakeup => BuiltInFxParameter::CompressorMakeup,
        BuiltInFxParameterDocument::Drive => BuiltInFxParameter::Drive,
        BuiltInFxParameterDocument::DriveTone => BuiltInFxParameter::DriveTone,
        BuiltInFxParameterDocument::DriveMix => BuiltInFxParameter::DriveMix,
        BuiltInFxParameterDocument::DriveOutput => BuiltInFxParameter::DriveOutput,
        BuiltInFxParameterDocument::EqLow => BuiltInFxParameter::EqLow,
        BuiltInFxParameterDocument::EqMid => BuiltInFxParameter::EqMid,
        BuiltInFxParameterDocument::EqHigh => BuiltInFxParameter::EqHigh,
        BuiltInFxParameterDocument::ChorusRate => BuiltInFxParameter::ChorusRate,
        BuiltInFxParameterDocument::ChorusDepth => BuiltInFxParameter::ChorusDepth,
        BuiltInFxParameterDocument::ChorusMix => BuiltInFxParameter::ChorusMix,
        BuiltInFxParameterDocument::ChorusWidth => BuiltInFxParameter::ChorusWidth,
        BuiltInFxParameterDocument::ModulationRate => BuiltInFxParameter::ModulationRate,
        BuiltInFxParameterDocument::ModulationDepth => BuiltInFxParameter::ModulationDepth,
        BuiltInFxParameterDocument::ModulationMix => BuiltInFxParameter::ModulationMix,
        BuiltInFxParameterDocument::ModulationFeedback => BuiltInFxParameter::ModulationFeedback,
        BuiltInFxParameterDocument::ModulationSpread => BuiltInFxParameter::ModulationSpread,
        BuiltInFxParameterDocument::ReverbAmount => BuiltInFxParameter::ReverbAmount,
        BuiltInFxParameterDocument::ReverbTone => BuiltInFxParameter::ReverbTone,
    }
}
pub(super) fn synth_to_document(parameter: OxiSynthParameter) -> OxiSynthParameterDocument {
    match parameter {
        OxiSynthParameter::ReverbSend => OxiSynthParameterDocument::ReverbSend,
        OxiSynthParameter::ChorusSend => OxiSynthParameterDocument::ChorusSend,
    }
}
pub(super) fn synth_from_document(parameter: OxiSynthParameterDocument) -> OxiSynthParameter {
    match parameter {
        OxiSynthParameterDocument::ReverbSend => OxiSynthParameter::ReverbSend,
        OxiSynthParameterDocument::ChorusSend => OxiSynthParameter::ChorusSend,
    }
}
pub(super) fn bus_processor_chain(bus: &image::Bus) -> Result<Option<FxChainDocument>> {
    bus.fx
        .as_ref()
        .map(|fx| {
            ensure!(
                fx.processor_type.as_str() == TrackProcessorTypeId::BUILTIN_FX,
                "unsupported bus processor"
            );
            Ok(FxChainDocument {
                id: bus.id.raw(),
                title: bus.name.clone(),
                chain_type: FxChainTypeDocument::BuiltInFx,
                ports: Vec::new(),
                internal_state: canonical_state(FxChainTypeDocument::BuiltInFx, &fx.state)?,
                builtin_fx_midi_cc_assignments: fx
                    .midi_cc_assignments
                    .iter()
                    .map(|a| BuiltInFxMidiCcAssignmentDocument {
                        parameter: fx_to_document(a.parameter),
                        channel: a.channel,
                        controller: a.controller,
                    })
                    .collect(),
                midi_cc_assignments: Vec::new(),
            })
        })
        .transpose()
}
pub(super) fn processor_chain(track: &image::Track) -> Result<Option<FxChainDocument>> {
    let chain_type = match &track.topology {
        TrackTopology::DryWet { processor_type, .. } => match processor_type.as_str() {
            TrackProcessorTypeId::BUILTIN_FX => Some(FxChainTypeDocument::BuiltInFx),
            TrackProcessorTypeId::OXISYNTH => Some(FxChainTypeDocument::OxiSynth),
            TrackProcessorTypeId::EXTERNAL => None,
            _ => bail!("unsupported processor type (P07)"),
        },
        TrackTopology::Direct => None,
    };
    let Some(chain_type) = chain_type else {
        ensure!(
            track.processor_state.is_none()
                && track.builtin_fx_midi_cc_assignments.is_empty()
                && track.oxisynth_midi_cc_assignments.is_empty(),
            "track has unexpected processor state"
        );
        return Ok(None);
    };
    Ok(Some(FxChainDocument {
        id: track.id.raw(),
        title: track.name.clone(),
        chain_type,
        ports: Vec::new(),
        internal_state: canonical_state(
            chain_type,
            track
                .processor_state
                .as_deref()
                .ok_or_else(|| anyhow!("processor state missing"))?,
        )?,
        builtin_fx_midi_cc_assignments: track
            .builtin_fx_midi_cc_assignments
            .iter()
            .map(|a| BuiltInFxMidiCcAssignmentDocument {
                parameter: fx_to_document(a.parameter),
                channel: a.channel,
                controller: a.controller,
            })
            .collect(),
        midi_cc_assignments: track
            .oxisynth_midi_cc_assignments
            .iter()
            .map(|a| OxiSynthMidiCcAssignmentDocument {
                parameter: synth_to_document(a.parameter),
                channel: a.channel,
                controller: a.controller,
            })
            .collect(),
    }))
}
