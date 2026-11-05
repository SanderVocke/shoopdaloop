//! Capture settled core state while its media cannot be changed by admitted work.
use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    mirror::Mirror,
    prepared::Validation,
    reads::{empty_audio, empty_midi, Outcome},
    EngineClock, RtBackend,
};
use alloc::sync::Arc;
use anyhow::{anyhow, bail, Result};
use shoop_backend::*;
use shoop_rt_protocol::LoopMode;
use shoop_rt_transport::carrier::Carrier;

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    fn capture_records(
        &self,
        id: BackendLoopId,
        only: Option<BackendLoopId>,
        visited: &mut BTreeSet<BackendLoopId>,
    ) -> bool {
        if !visited.insert(id) {
            return false;
        }
        let records = |target, mode, visited: &mut BTreeSet<BackendLoopId>| {
            mode == BackendLoopMode::Recording && only.is_none_or(|id| id == target)
                || mode == BackendLoopMode::Playing && self.capture_records(target, only, visited)
        };
        let Some(entry) = self.mirror.loops.get(&id) else {
            return false;
        };
        entry.program_spans.iter().any(|span| {
            records(
                self.mirror.slot_loops[&span.target],
                crate::mirror::app_mode(span.mode),
                visited,
            )
        }) || self
            .composites
            .jobs
            .iter()
            .filter(|job| job.id == id)
            .any(|job| {
                job.plan.spans.iter().any(|span| {
                    let target = match span.target {
                        BackendCompositeTarget::Loop(id) => id,
                        BackendCompositeTarget::Composite(id) => BackendLoopId::from_raw(id.raw()),
                    };
                    records(target, span.mode, visited)
                })
            })
    }
    fn capture_pending(
        &self,
        completed: usize,
        total: usize,
    ) -> BackendAsyncResult<BackendSessionData> {
        BackendAsyncResult::Pending(BackendOperationProgress {
            key: 1,
            kind: BackendOperationKind::SessionCapture,
            completed,
            total: Some(total),
        })
    }
    pub(crate) fn capture_core(
        &mut self,
        only: Option<BackendLoopId>,
    ) -> Result<BackendAsyncResult<BackendSessionData>> {
        self.pump();
        self.ready()?;
        if only.is_some_and(|id| self.mirror.live_loop(id).is_none()) {
            bail!("unknown capture loop");
        }
        if self.input.is_some() || !self.composites.controls.is_empty()
            || self
                .requests
                .outgoing
                .iter()
                .chain(self.requests.in_flight.values())
                .any(|pending| match pending.request {
                    shoop_rt_protocol::Request::CreateBus { .. }
                    | shoop_rt_protocol::Request::RemoveBus { .. }
                    | shoop_rt_protocol::Request::CreateStrip { .. }
                    | shoop_rt_protocol::Request::CreateProcessedStrip { .. }
                    | shoop_rt_protocol::Request::ProcessorControl { .. }
                    | shoop_rt_protocol::Request::StripDefaultPlayback { .. }
                    | shoop_rt_protocol::Request::StripControl { .. }
                    | shoop_rt_protocol::Request::RemoveStrip { .. }
                    | shoop_rt_protocol::Request::CreateLoop { .. }
                    | shoop_rt_protocol::Request::CreateComposite { .. }
                    | shoop_rt_protocol::Request::ResetComposite { .. }
                    | shoop_rt_protocol::Request::RemoveLoop { .. }
                    | shoop_rt_protocol::Request::LoopTiming { .. }
                    | shoop_rt_protocol::Request::LoopClear { .. }
                    | shoop_rt_protocol::Request::CancelBulk => true,
                    shoop_rt_protocol::Request::LoopTransition {
                        loop_id,
                        mode: LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet,
                        ..
                    } => only.is_none_or(|id| self.mirror.loops[&id].slot == loop_id),
                    _ => false,
                })
        {
            return Ok(self.capture_pending(0, self.mirror.loops.len()));
        }
        if self
            .composites
            .jobs
            .iter()
            .any(|job| self.mirror.loops[&job.id].program.is_none())
        {
            return Ok(self.capture_pending(0, self.mirror.loops.len()));
        }
        if self.mirror.loops.iter().any(|(id, entry)| {
            only.is_none_or(|target| target == *id)
                && (entry.view.recording_armed
                    || entry.view.mode.records()
                    || entry.view.next_mode.is_some_and(LoopMode::records))
                || (entry.view.mode == LoopMode::Playing
                    || entry.view.next_mode == Some(LoopMode::Playing))
                    && self.capture_records(
                        *id,
                        if only == Some(*id) { None } else { only },
                        &mut BTreeSet::new(),
                    )
        }) || self
            .requests
            .outgoing
            .iter()
            .chain(self.requests.in_flight.values())
            .any(|pending| match pending.request {
                shoop_rt_protocol::Request::LoopTransition {
                    loop_id,
                    mode: LoopMode::Playing,
                    ..
                } => self
                    .mirror
                    .slot_loops
                    .get(&loop_id)
                    .is_some_and(|id| self.capture_records(*id, only, &mut BTreeSet::new())),
                _ => false,
            })
        {
            bail!("stop recording and settle pending recording transitions before saving or exporting");
        }
        self.canonical_content
            .retain(|id, _| self.mirror.loops.contains_key(id));
        if let Some(target) = only {
            self.canonical_content
                .retain(|id, _| self.canonical_imports.contains(id) || *id == target);
        }
        self.prepared_content.retain(|id, proof| {
            self.canonical_content
                .get(id)
                .is_some_and(|image| proof.matches(image))
        });
        let ids = self
            .mirror
            .loops
            .keys()
            .copied()
            .filter(|id| only.is_none_or(|target| *id == target))
            .collect::<Vec<_>>();
        let mut bytes = self
            .canonical_imports
            .iter()
            .filter(|id| only.is_some_and(|target| target != **id))
            .try_fold(0u64, |total, id| {
                total
                    .checked_add(
                        self.prepared_content
                            .get(id)
                            .ok_or_else(|| anyhow!("retained content lacks a validation proof"))?
                            .bytes(),
                    )
                    .ok_or_else(|| anyhow!("retained capture size overflow"))
            })?;
        let mut event_count = self
            .canonical_content
            .iter()
            .filter(|(id, _)| only.is_some_and(|target| target != **id))
            .try_fold(0usize, |total, (_, content)| {
                content.midi.iter().try_fold(total, |total, channel| {
                    total
                        .checked_add(channel.events.len())
                        .ok_or_else(|| anyhow!("retained MIDI event count overflow"))
                })
            })?;
        for (index, id) in ids.iter().copied().enumerate() {
            if self.mirror.loops[&id].program.is_some() {
                continue;
            }
            if !self.canonical_content.contains_key(&id) {
                let entry = &self.mirror.loops[&id];
                if entry.timing.content_revision != entry.rt_content_revision {
                    return Ok(self.capture_pending(index, ids.len()));
                }
                let (slot, version, length) = (entry.slot, entry.content, entry.view.length as u32);
                let track = &self.mirror.tracks[&entry
                    .track
                    .ok_or_else(|| anyhow!("composites have no media channels"))?];
                let wet = track.audio_channels as usize;
                let dry = (!matches!(track.topology, BackendTrackTopology::Direct { .. }))
                    .then_some(track.topology.dry_audio_channels() as usize);
                let (channels, midi) = (wet + dry.unwrap_or(0), track.midi);
                let audio_bytes = entry
                    .timing
                    .raw_audio_frames
                    .checked_mul(channels as u64 * 4)
                    .ok_or_else(|| anyhow!("audio size overflow"))?;
                if audio_bytes > self.resource_limits.media_bytes.saturating_sub(bytes) {
                    bail!("session capture exceeds the remaining host media budget");
                }
                let audio = if entry.timing.raw_audio_frames == 0 || channels == 0 {
                    empty_audio(channels)
                } else {
                    let read = match dry {
                        Some(dry) => self.reads.audio_roles(id, version, slot, dry, wet),
                        None => self.reads.audio(id, version, slot, channels),
                    };
                    match read {
                        Outcome::Done(audio) => self.audio_timing(id, audio),
                        Outcome::Reading => return Ok(self.capture_pending(index, ids.len())),
                        Outcome::Failed(message) => bail!(message),
                    }
                };
                let mut initial = Vec::new();
                if entry.timing.raw_midi_frames > 0 && midi {
                    for channel in 0..16 {
                        match self.reads.initial(id, version, slot, channel) {
                            Outcome::Done(mut packets) => initial.append(&mut packets),
                            Outcome::Reading => return Ok(self.capture_pending(index, ids.len())),
                            Outcome::Failed(message) => bail!(message),
                        }
                    }
                }
                if initial.len() > 16_384 {
                    bail!("MIDI initial state exceeds its bounded packet count");
                }
                let initial_bytes = initial.iter().map(|p| p.len() as u64 + 4).sum::<u64>();
                let midi_limit = self
                    .resource_limits
                    .media_bytes
                    .checked_sub(bytes)
                    .and_then(|n| n.checked_sub(audio_bytes))
                    .and_then(|n| n.checked_sub(initial_bytes))
                    .ok_or_else(|| {
                        anyhow!("session capture exceeds the remaining host media budget")
                    })?;
                let events = if entry.timing.raw_midi_frames == 0 || !midi {
                    empty_midi(version, midi)
                } else {
                    match self.reads.midi_limited(id, version, slot, midi_limit) {
                        Outcome::Done(midi) => self.midi_timing(id, midi),
                        Outcome::Reading => return Ok(self.capture_pending(index, ids.len())),
                        Outcome::Failed(message) => bail!(message),
                    }
                };
                let entry = &self.mirror.loops[&id];
                let content = BackendLoopContent {
                    recorded_processor_state: entry.recorded_state(),
                    source_id: id.raw(),
                    length,
                    gain: entry.gain,
                    balance: entry.balance,
                    audio: audio
                        .channels
                        .into_iter()
                        .enumerate()
                        .map(|(index, channel)| BackendAudioContent {
                            mode: match dry {
                                Some(dry) if index < dry => BackendChannelMode::Dry,
                                Some(_) => BackendChannelMode::Wet,
                                None => BackendChannelMode::Direct,
                            },
                            samples: channel.samples,
                            gain: f32::from_bits(entry.timing.windows.audio[index].gain_bits),
                            start_offset: entry.timing.windows.audio[index].start_offset,
                            capture_alignment_frames: entry.timing.windows.audio[index]
                                .capture_alignment,
                            preplay: entry.timing.windows.audio[index].preplay,
                        })
                        .collect(),
                    midi: events
                        .channels
                        .into_iter()
                        .map(|channel| BackendMidiContent {
                            mode: if dry.is_some() {
                                BackendChannelMode::Dry
                            } else {
                                BackendChannelMode::Direct
                            },
                            length: channel.length,
                            start_state: initial.clone(),
                            events: channel.events,
                            start_offset: entry.timing.windows.midi.start_offset,
                            capture_alignment_frames: entry.timing.windows.midi.capture_alignment,
                            preplay: entry.timing.windows.midi.preplay,
                        })
                        .collect(),
                };
                self.canonical_content.insert(id, Arc::new(content));
                self.reads.forget(id);
            }
            let image = Arc::clone(&self.canonical_content[&id]);
            event_count = image.midi.iter().try_fold(event_count, |count, channel| {
                count
                    .checked_add(channel.events.len())
                    .ok_or_else(|| anyhow!("MIDI event count overflow"))
            })?;
            if event_count > self.resource_limits.midi_events {
                bail!("capture exceeds host MIDI event capacity");
            }
            if !self
                .prepared_content
                .get(&id)
                .is_some_and(|p| p.matches(&image))
            {
                if !self
                    .capture_validation
                    .as_ref()
                    .is_some_and(|v| Arc::ptr_eq(v.image(), &image))
                {
                    self.capture_validation = Some(Validation::new(
                        image,
                        usize::from(self.limits().strip_audio_channels),
                    )?);
                }
                if !self.capture_validation.as_mut().unwrap().step(256 * 1024)? {
                    return Ok(self.capture_pending(index, ids.len()));
                }
                let proof = self.capture_validation.take().unwrap().finish()?;
                self.prepared_content.insert(id, proof);
            }
            bytes = bytes
                .checked_add(self.prepared_content[&id].bytes())
                .ok_or_else(|| anyhow!("capture size overflow"))?;
            if bytes > self.resource_limits.media_bytes {
                bail!("session capture exceeds the host media budget");
            }
        }
        let port = |id: BackendPortId| -> BackendSessionPort {
            BackendSessionPort {
                source_id: id.raw(),
                descriptor: self.mirror.ports[&id].descriptor.clone(),
                external_connections: self
                    .mirror
                    .links
                    .union(&self.mirror.desired_links)
                    .filter(|link| link.application_port_id == id)
                    .map(|link| link.host_port_id.clone())
                    .collect(),
            }
        };
        let ports = self
            .mirror
            .ports
            .keys()
            .map(|id| (*id, port(*id)))
            .collect::<alloc::collections::BTreeMap<_, _>>();
        let tracks = self
            .mirror
            .tracks
            .iter_mut()
            .filter(|(_, track)| {
                track.confirmed
                    && !track.removing
                    && only.is_none_or(|id| track.loops.contains(&id))
            })
            .map(|(id, track)| {
                let loops = track
                    .loops
                    .iter()
                    .filter(|id| only.is_none_or(|target| **id == target))
                    .map(|id| {
                        let entry = &self.mirror.loops[id];
                        let mut content = if entry.program.is_some() {
                            BackendLoopContent {
                                recorded_processor_state: None,
                                source_id: id.raw(),
                                length: 0,
                                gain: entry.gain,
                                balance: entry.balance,
                                audio: Vec::new(),
                                midi: Vec::new(),
                            }
                        } else {
                            (*self.canonical_content[id]).clone()
                        };
                        content.source_id = id.raw();
                        content.gain = entry.gain;
                        content.balance = entry.balance;
                        content
                    })
                    .collect();
                let state = Mirror::track_state(track);
                let (mut fx_maps, mut synth_maps) = (Vec::new(), Vec::new());
                if let Some(editor) = state.fx.as_ref().and_then(|fx| fx.editor.as_ref()) {
                    match editor {
                        shoop_app_api::TrackProcessorEditorState::BuiltInFx(fx) => {
                            fx_maps = fx
                                .midi_cc_assignments
                                .iter()
                                .map(|a| BackendBuiltInFxMidiCcAssignment {
                                    parameter: a.parameter,
                                    channel: a.channel,
                                    controller: a.controller,
                                })
                                .collect()
                        }
                        shoop_app_api::TrackProcessorEditorState::OxiSynth(synth) => {
                            synth_maps = synth
                                .midi_cc_assignments
                                .iter()
                                .map(|a| BackendOxiSynthMidiCcAssignment {
                                    parameter: a.parameter,
                                    channel: a.channel,
                                    controller: a.controller,
                                })
                                .collect()
                        }
                    }
                }
                let processor_state =
                    crate::processor_controls::track_kind(track).and_then(|kind| {
                        let mut state = shoop_processor_protocol::state::State::new(kind).ok()?;
                        state.values = track.processor_snapshot?.values;
                        Some(state.encode())
                    });
                BackendSessionTrack {
                    source_id: id.raw(),
                    port_name_base: track.port_name_base.clone(),
                    topology: track.topology.clone(),
                    state,
                    loops,
                    ports: track.ports.iter().map(|id| ports[id].clone()).collect(),
                    processor_state,
                    builtin_fx_midi_cc_assignments: fx_maps,
                    oxisynth_midi_cc_assignments: synth_maps,
                }
            })
            .collect();
        Ok(BackendAsyncResult::Ready(BackendSessionData {
            composites: self
                .mirror
                .loops
                .iter()
                .filter_map(|(id, entry)| {
                    let d = entry.program?;
                    if only.is_some_and(|target| target != *id) {
                        return None;
                    }
                    Some(BackendSessionComposite {
                        source_id: id.raw(),
                        version: d.version,
                        plan: BackendPreparedCompositePlan {
                            kind: match d.kind {
                                shoop_rt_protocol::CompositeKind::Regular => {
                                    BackendCompositeKind::Regular
                                }
                                shoop_rt_protocol::CompositeKind::Explicit => {
                                    BackendCompositeKind::Script
                                }
                            },
                            sync_source: entry.sync,
                            unit_frames: d.unit_frames,
                            iterations: d.iterations,
                            entries: d.entries,
                            spans: entry
                                .program_spans
                                .iter()
                                .map(|span| {
                                    let target = self.mirror.slot_loops[&span.target];
                                    BackendCompositeSpan {
                                        target: if self.mirror.loops[&target].program.is_some() {
                                            BackendCompositeTarget::Composite(
                                                BackendCompositeId::from_raw(target.raw()),
                                            )
                                        } else {
                                            BackendCompositeTarget::Loop(target)
                                        },
                                        start: span.start,
                                        end: span.end,
                                        occurrence_start: span.occurrence_start,
                                        ordinal: span.ordinal,
                                        mode: crate::mirror::app_mode(span.mode),
                                    }
                                })
                                .collect(),
                        },
                    })
                })
                .collect(),
            loop_sync: self
                .mirror
                .loops
                .iter()
                .filter_map(|(id, entry)| {
                    entry
                        .sync
                        .filter(|_| only.is_none_or(|target| target == *id))
                        .map(|source| (id.raw(), source.raw()))
                })
                .collect(),
            repeat_sync: self
                .mirror
                .loops
                .iter()
                .filter_map(|(id, entry)| {
                    (entry.repeat_sync && only.is_none_or(|target| target == *id))
                        .then_some(id.raw())
                })
                .collect(),
            sample_rate: self.driver.map_or(0, |driver| driver.sample_rate),
            tracks,
            buses: self
                .mirror
                .buses
                .iter()
                .map(|(id, bus)| BackendSessionBus {
                    source_id: id.raw(),
                    name: bus.name.clone(),
                    channels: bus
                        .channels
                        .iter()
                        .map(|c| BackendSessionBusChannel {
                            source_id: c.id.raw(),
                            label: c.label.clone(),
                            output_port: ports[&c.output_port_id].clone(),
                        })
                        .collect(),
                    gain_db: bus.gain_db,
                    balance: bus.balance,
                    muted: bus.muted,
                    processor_type: None,
                    processor_state: None,
                    builtin_fx_midi_cc_assignments: Vec::new(),
                })
                .collect(),
            mixer_routes: self
                .mirror
                .routes
                .iter()
                .map(|route| BackendSessionMixerRoute {
                    source_port_id: route.source_port_id.raw(),
                    destination_channel_id: route.destination_channel_id.raw(),
                })
                .collect(),
            global_ports: vec![ports[&GLOBAL_FX_PORT_ID].clone()],
            use_legacy_browser_default_routes: false,
        }))
    }
}
