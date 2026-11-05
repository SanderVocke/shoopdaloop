//! Editor controls and confirmed host state, independent of concrete DSP packages.
use crate::{mirror::Track, requests::Purpose, EngineClock, RtBackend};
use alloc::{format, sync::Arc, vec::Vec};
use anyhow::{anyhow, Result};
use shoop_app_api::*;
use shoop_backend::{BackendTrackFxControl, BackendTrackId, BackendTrackTopology};
use shoop_processor_protocol::{
    state::State, Control, Kind, Phase, CC_BASE, CLEAR_CC, DRIVE_TYPE, MODULATION_TYPE,
    REVERB_TYPE, STAGE_BASE, SYNTH_PANIC, SYNTH_PRESET,
};
use shoop_rt_protocol::Request;
use shoop_rt_transport::carrier::Carrier;

pub(crate) fn track_kind(track: &Track) -> Option<Kind> {
    topology_kind(&track.topology)
}
pub(crate) fn topology_kind(topology: &BackendTrackTopology) -> Option<Kind> {
    let BackendTrackTopology::DryWetProcessor { processor_type, .. } = topology else {
        return None;
    };
    match processor_type.as_str() {
        TrackProcessorTypeId::BUILTIN_FX => Some(Kind::BuiltInFx),
        TrackProcessorTypeId::OXISYNTH => Some(Kind::OxiSynth),
        _ => None,
    }
}

pub(crate) fn recorded_state(
    topology: &BackendTrackTopology,
    encoded: Option<&str>,
) -> Result<Option<shoop_rt_protocol::RecordedProcessorState>> {
    let Some(encoded) = encoded else {
        return Ok(None);
    };
    let kind = topology_kind(topology)
        .ok_or_else(|| anyhow!("recorded state requires a hosted processor"))?;
    let mut state = State::decode(kind, encoded)?;
    if kind == Kind::BuiltInFx {
        state.values[55] = topology.dry_audio_channels() as f32;
    }
    Ok(Some(shoop_rt_protocol::RecordedProcessorState {
        source_frame: 0,
        kind,
        values: state.values,
    }))
}

pub(crate) fn session_state(track: &shoop_backend::BackendSessionTrack) -> Result<Option<Control>> {
    let Some(kind) = topology_kind(&track.topology) else {
        if track.processor_state.is_some()
            || !track.builtin_fx_midi_cc_assignments.is_empty()
            || !track.oxisynth_midi_cc_assignments.is_empty()
            || track.state.fx.is_some()
        {
            return Err(anyhow!("this track has no hosted processor state"));
        }
        return Ok(None);
    };
    let mut state = match &track.processor_state {
        Some(encoded) => State::decode(kind, encoded)?,
        None => State::new(kind)?,
    };
    if kind == Kind::BuiltInFx {
        if !track.oxisynth_midi_cc_assignments.is_empty() {
            return Err(anyhow!("FX track has synth mappings"));
        }
        state.values[55] = track.topology.dry_audio_channels() as f32;
        for assignment in &track.builtin_fx_midi_cc_assignments {
            let index = BuiltInFxParameter::ALL
                .iter()
                .position(|p| *p == assignment.parameter)
                .unwrap();
            if state.values[32 + index] >= 0. {
                return Err(anyhow!("duplicate processor mapping"));
            }
            state.values[32 + index] = cc_value(assignment.channel, assignment.controller)?;
        }
    } else {
        if !track.builtin_fx_midi_cc_assignments.is_empty() {
            return Err(anyhow!("synth track has FX mappings"));
        }
        for assignment in &track.oxisynth_midi_cc_assignments {
            let index = OxiSynthParameter::ALL
                .iter()
                .position(|p| *p == assignment.parameter)
                .unwrap();
            if state.values[3 + index] >= 0. {
                return Err(anyhow!("duplicate processor mapping"));
            }
            state.values[3 + index] = cc_value(assignment.channel, assignment.controller)?;
        }
    }
    let mappings = if kind == Kind::BuiltInFx {
        &state.values[32..55]
    } else {
        &state.values[3..5]
    };
    for (i, value) in mappings.iter().enumerate() {
        if *value >= 0. && mappings[..i].contains(value) {
            return Err(anyhow!("duplicate processor MIDI source"));
        }
    }
    state.validate()?;
    Ok(Some(Control::Restore {
        values: state.values,
        replace_mappings: true,
    }))
}
pub(crate) fn track_fx_state(track: &Track) -> Option<TrackFxState> {
    let kind = track_kind(track)?;
    let snapshot = track.processor_snapshot?;
    let values = snapshot.values;
    let editor = match kind {
        Kind::BuiltInFx => TrackProcessorEditorState::BuiltInFx(BuiltInFxState {
            compressor_enabled: values[23] != 0.,
            compressor_threshold_db: values[0],
            compressor_ratio: values[1],
            compressor_attack_ms: values[2],
            compressor_release_ms: values[3],
            compressor_makeup_db: values[4],
            drive_enabled: values[24] != 0.,
            drive_type: *[
                BuiltInFxDriveType::Saturation,
                BuiltInFxDriveType::Overdrive,
                BuiltInFxDriveType::Distortion,
                BuiltInFxDriveType::Fuzz,
            ]
            .get(values[29] as usize)?,
            drive_db: values[5],
            drive_tone: values[6],
            drive_mix: values[7],
            drive_output_db: values[8],
            eq_enabled: values[25] != 0.,
            eq_low_db: values[9],
            eq_mid_db: values[10],
            eq_high_db: values[11],
            chorus_enabled: values[26] != 0.,
            chorus_rate_hz: values[12],
            chorus_depth: values[13],
            chorus_mix: values[14],
            chorus_width: values[15],
            modulation_enabled: values[27] != 0.,
            modulation_type: *[
                BuiltInFxModulationType::Tremolo,
                BuiltInFxModulationType::Flanger,
                BuiltInFxModulationType::Phaser,
            ]
            .get(values[30] as usize)?,
            modulation_rate_hz: values[16],
            modulation_depth: values[17],
            modulation_mix: values[18],
            modulation_feedback: values[19],
            modulation_spread: values[20],
            reverb_enabled: values[28] != 0.,
            reverb_type: *[
                BuiltInFxReverbType::Room,
                BuiltInFxReverbType::Hall,
                BuiltInFxReverbType::Plate,
            ]
            .get(values[31] as usize)?,
            reverb_amount: values[21],
            reverb_tone: values[22],
            midi_cc_assignments: BuiltInFxParameter::ALL
                .into_iter()
                .enumerate()
                .filter_map(|(i, parameter)| {
                    cc_source(values[32 + i]).map(|(channel, controller)| {
                        BuiltInFxMidiCcAssignment {
                            parameter,
                            channel,
                            controller,
                        }
                    })
                })
                .collect::<Vec<_>>()
                .into(),
        }),
        Kind::OxiSynth => TrackProcessorEditorState::OxiSynth(OxiSynthState {
            selected_preset_id: format!("{}:{}", values[2] as u32 / 128, values[2] as u32 % 128),
            reverb_send: values[0],
            chorus_send: values[1],
            midi_cc_assignments: OxiSynthParameter::ALL
                .into_iter()
                .enumerate()
                .filter_map(|(i, parameter)| {
                    cc_source(values[3 + i]).map(|(channel, controller)| OxiSynthMidiCcAssignment {
                        parameter,
                        channel,
                        controller,
                    })
                })
                .collect::<Vec<_>>()
                .into(),
        }),
        Kind::External => return None,
    };
    Some(TrackFxState {
        processor_type: TrackProcessorTypeId::new(match kind {
            Kind::BuiltInFx => TrackProcessorTypeId::BUILTIN_FX,
            _ => TrackProcessorTypeId::OXISYNTH,
        }),
        active: snapshot.phase == Phase::Running,
        visible: track.processor_visible,
        lifecycle: match snapshot.phase {
            Phase::Running => FxLifecycle::Running,
            Phase::Faulted => FxLifecycle::Crashed,
            Phase::Free | Phase::Retiring => FxLifecycle::Unavailable,
            Phase::Owned => FxLifecycle::Stopped,
        },
        generation: track
            .processor_slot
            .map_or(0, |slot| u64::from(slot.generation)),
        deadline_misses: 0,
        stale_completions: 0,
        status_summary: if snapshot.phase == Phase::Faulted {
            Some("processor faulted".into())
        } else if snapshot.global_midi.pending > 0 {
            Some(format!(
                "{} queued MIDI controls",
                snapshot.global_midi.pending
            ))
        } else {
            None
        },
        crash_summary: None,
        logs: Arc::from([]),
        editor: Some(editor),
    })
}
fn cc_source(value: f32) -> Option<(u8, u8)> {
    (value >= 0.).then(|| ((value as u16 / 128) as u8, (value as u16 % 128) as u8))
}
fn cc_value(channel: u8, controller: u8) -> Result<f32> {
    if channel > 15 || controller > 127 {
        return Err(anyhow!("invalid MIDI CC source"));
    }
    Ok((u16::from(channel) * 128 + u16::from(controller)) as f32)
}
impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn processor_control(
        &mut self,
        id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        self.ready()?;
        let track = self
            .mirror
            .live_track(id)
            .ok_or_else(|| anyhow!("unknown track {id:?}"))?;
        let kind = track_kind(track).ok_or_else(|| anyhow!("track has no built-in processor"))?;
        let strip = track.strip;
        let parameter = |parameter, value| Control::Parameter { parameter, value };
        let update = match &control {
            BackendTrackFxControl::SetActive(on) => Control::Active(*on),
            BackendTrackFxControl::SetVisible(visible) => {
                self.mirror
                    .tracks
                    .get_mut(&id)
                    .expect("live track")
                    .processor_visible = *visible;
                return Ok(());
            }
            BackendTrackFxControl::ToggleOrRecover => {
                let track = self.mirror.tracks.get_mut(&id).expect("live track");
                track.processor_visible = !track.processor_visible;
                return Ok(());
            }
            BackendTrackFxControl::RestoreState(encoded) => {
                let mut state = State::decode(kind, encoded)?;
                state.values[55] = track.topology.dry_audio_channels() as f32;
                Control::Restore {
                    values: state.values,
                    replace_mappings: false,
                }
            }
            BackendTrackFxControl::BuiltInFx(control) if kind == Kind::BuiltInFx => {
                match *control {
                    BuiltInFxControl::SetStageEnabled(stage, on) => parameter(
                        STAGE_BASE
                            + match stage {
                                BuiltInFxStage::Compressor => 0,
                                BuiltInFxStage::Drive => 1,
                                BuiltInFxStage::Eq => 2,
                                BuiltInFxStage::Chorus => 3,
                                BuiltInFxStage::Modulation => 4,
                                BuiltInFxStage::Reverb => 5,
                            },
                        u8::from(on) as f32,
                    ),
                    BuiltInFxControl::SetReverbEnabled(on) => {
                        parameter(STAGE_BASE + 5, u8::from(on) as f32)
                    }
                    BuiltInFxControl::SetDriveType(value) => {
                        parameter(DRIVE_TYPE, value as u8 as f32)
                    }
                    BuiltInFxControl::SetModulationType(value) => {
                        parameter(MODULATION_TYPE, value as u8 as f32)
                    }
                    BuiltInFxControl::SetReverbType(value) => {
                        parameter(REVERB_TYPE, value as u8 as f32)
                    }
                    BuiltInFxControl::SetParameter(id, value) => {
                        if !id.is_valid(value) {
                            return Err(anyhow!("invalid Built-in FX parameter"));
                        }
                        parameter(id as u32, value)
                    }
                    BuiltInFxControl::AssignMidiCc(a) => parameter(
                        CC_BASE + a.parameter as u32,
                        cc_value(a.channel, a.controller)?,
                    ),
                    BuiltInFxControl::RemoveMidiCc(id) => parameter(CC_BASE + id as u32, -1.),
                    BuiltInFxControl::ClearMidiCcAssignments => parameter(CLEAR_CC, 0.),
                }
            }
            BackendTrackFxControl::OxiSynth(control) if kind == Kind::OxiSynth => match control {
                OxiSynthControl::SelectPreset(id) => {
                    let preset = self
                        .processor_presets
                        .iter()
                        .find(|p| format!("{}:{}", p.bank, p.program) == *id)
                        .ok_or_else(|| anyhow!("unknown host synth preset"))?;
                    parameter(
                        SYNTH_PRESET,
                        (u32::from(preset.bank) * 128 + u32::from(preset.program)) as f32,
                    )
                }
                OxiSynthControl::SetReverbSend(value) | OxiSynthControl::SetChorusSend(value) => {
                    if !value.is_finite() || !(0. ..=1.).contains(value) {
                        return Err(anyhow!("invalid synth send"));
                    }
                    parameter(
                        u32::from(matches!(control, OxiSynthControl::SetChorusSend(_))),
                        *value,
                    )
                }
                OxiSynthControl::AssignMidiCc(a) => parameter(
                    CC_BASE + a.parameter as u32,
                    cc_value(a.channel, a.controller)?,
                ),
                OxiSynthControl::RemoveMidiCc(id) => parameter(CC_BASE + *id as u32, -1.),
                OxiSynthControl::ClearMidiCcAssignments => parameter(CLEAR_CC, 0.),
                OxiSynthControl::Panic => parameter(SYNTH_PANIC, 0.),
            },
            _ => return Err(anyhow!("unsupported control for this processor")),
        };
        self.requests.push(
            Request::ProcessorControl {
                strip,
                control: update,
            },
            Purpose::ProcessorControl(id, control),
        );
        Ok(())
    }
}
