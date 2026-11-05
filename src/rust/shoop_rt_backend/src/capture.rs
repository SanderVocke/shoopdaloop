//! Capture settled core state while its media cannot be changed by admitted work.
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    mirror::Mirror,
    prepared::{Validation, MAX_MEDIA_BYTES},
    reads::{empty_audio, empty_midi, Outcome},
    EngineClock, RtBackend,
};
use alloc::sync::Arc;
use anyhow::{anyhow, bail, Result};
use shoop_backend::*;
use shoop_rt_protocol::LoopMode;
use shoop_rt_transport::carrier::Carrier;

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
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
        if self
            .requests
            .outgoing
            .iter()
            .chain(self.requests.in_flight.values())
            .any(|pending| match pending.request {
                shoop_rt_protocol::Request::CreateBus { .. }
                | shoop_rt_protocol::Request::RemoveBus { .. }
                | shoop_rt_protocol::Request::CreateStrip { .. }
                | shoop_rt_protocol::Request::CreateProcessedStrip { .. }
                | shoop_rt_protocol::Request::RemoveStrip { .. }
                | shoop_rt_protocol::Request::CreateLoop { .. }
                | shoop_rt_protocol::Request::RemoveLoop { .. }
                | shoop_rt_protocol::Request::LoopClear { .. }
                | shoop_rt_protocol::Request::CancelBulk => true,
                shoop_rt_protocol::Request::LoopTransition {
                    loop_id,
                    mode: LoopMode::Recording | LoopMode::RecordingDryIntoWet,
                    ..
                } => only.is_none_or(|id| self.mirror.loops[&id].slot == loop_id),
                _ => false,
            })
        {
            return Ok(self.capture_pending(0, self.mirror.loops.len()));
        }
        if self.mirror.loops.iter().any(|(id, entry)| {
            only.is_none_or(|target| target == *id)
                && (entry.view.mode.records()
                    || entry.view.next_mode.is_some_and(LoopMode::records))
        }) {
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
        let mut bytes = 0u64;
        for (index, id) in ids.iter().copied().enumerate() {
            if !self.canonical_content.contains_key(&id) {
                let entry = &self.mirror.loops[&id];
                let (slot, version, length) = (entry.slot, entry.content, entry.view.length as u32);
                let track = &self.mirror.tracks[&entry.track];
                let (channels, midi) = (track.audio_channels as usize, track.midi);
                let audio_bytes = u64::from(length)
                    .checked_mul(channels as u64 * 4)
                    .ok_or_else(|| anyhow!("audio size overflow"))?;
                if audio_bytes > MAX_MEDIA_BYTES.saturating_sub(bytes) {
                    bail!("session capture exceeds the remaining 256 MiB budget");
                }
                let audio = if length == 0 || channels == 0 {
                    empty_audio(channels)
                } else {
                    match self.reads.audio(id, version, slot, channels) {
                        Outcome::Done(audio) => audio,
                        Outcome::Reading => return Ok(self.capture_pending(index, ids.len())),
                        Outcome::Failed(message) => bail!(message),
                    }
                };
                let mut initial = Vec::new();
                if length > 0 && midi {
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
                let midi_limit = MAX_MEDIA_BYTES
                    .checked_sub(bytes)
                    .and_then(|n| n.checked_sub(audio_bytes))
                    .and_then(|n| n.checked_sub(initial_bytes))
                    .ok_or_else(|| {
                        anyhow!("session capture exceeds the remaining 256 MiB budget")
                    })?;
                let events = if length == 0 || !midi {
                    empty_midi(version, midi)
                } else {
                    match self.reads.midi_limited(id, version, slot, midi_limit) {
                        Outcome::Done(midi) => midi,
                        Outcome::Reading => return Ok(self.capture_pending(index, ids.len())),
                        Outcome::Failed(message) => bail!(message),
                    }
                };
                let entry = &self.mirror.loops[&id];
                let content = BackendLoopContent {
                    source_id: id.raw(),
                    length,
                    gain: entry.gain,
                    balance: entry.balance,
                    audio: audio
                        .channels
                        .into_iter()
                        .map(|channel| BackendAudioContent {
                            mode: BackendChannelMode::Direct,
                            samples: channel.samples,
                            gain: 1.,
                            start_offset: channel.start_offset,
                            capture_alignment_frames: channel.capture_alignment_frames,
                            preplay: channel.preplay,
                        })
                        .collect(),
                    midi: events
                        .channels
                        .into_iter()
                        .map(|channel| BackendMidiContent {
                            mode: BackendChannelMode::Direct,
                            length: channel.length,
                            start_state: initial.clone(),
                            events: channel.events,
                            start_offset: channel.start_offset,
                            capture_alignment_frames: channel.capture_alignment_frames,
                            preplay: channel.preplay,
                        })
                        .collect(),
                };
                self.canonical_content.insert(id, Arc::new(content));
                self.reads.forget(id);
            }
            let image = Arc::clone(&self.canonical_content[&id]);
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
            if bytes > MAX_MEDIA_BYTES {
                bail!("session capture exceeds the 256 MiB media budget");
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
                        let mut content = (*self.canonical_content[id]).clone();
                        content.source_id = id.raw();
                        content.gain = entry.gain;
                        content.balance = entry.balance;
                        content
                    })
                    .collect();
                BackendSessionTrack {
                    source_id: id.raw(),
                    port_name_base: track.port_name_base.clone(),
                    topology: track.topology.clone(),
                    state: Mirror::track_state(track),
                    loops,
                    ports: track.ports.iter().map(|id| ports[id].clone()).collect(),
                    processor_state: None,
                    builtin_fx_midi_cc_assignments: Vec::new(),
                    oxisynth_midi_cc_assignments: Vec::new(),
                }
            })
            .collect();
        Ok(BackendAsyncResult::Ready(BackendSessionData {
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
