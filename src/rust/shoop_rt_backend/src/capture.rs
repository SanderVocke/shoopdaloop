//! Capture settled core state while its media cannot be changed by admitted work.
use crate::{
    mirror::Mirror,
    prepared::{canonical_bytes, MAX_MEDIA_BYTES},
    reads::{empty_audio, empty_midi, Outcome},
    EngineClock, RtBackend,
};
use anyhow::{anyhow, bail, Result};
use shoop_backend::*;
use shoop_rt_protocol::LoopMode;
use shoop_rt_transport::carrier::Carrier;
use std::sync::Arc;

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
    pub(crate) fn capture_core(&mut self) -> Result<BackendAsyncResult<BackendSessionData>> {
        self.pump();
        self.ready()?;
        if !self.requests.is_empty() {
            return Ok(self.capture_pending(0, self.mirror.loops.len()));
        }
        if self.mirror.loops.values().any(|entry| {
            entry.view.mode == LoopMode::Recording
                || entry.view.next_mode == Some(LoopMode::Recording)
        }) {
            bail!("stop recording and settle pending recording transitions before saving or exporting");
        }
        self.canonical_content
            .retain(|id, _| self.mirror.loops.contains_key(id));
        let ids = self.mirror.loops.keys().copied().collect::<Vec<_>>();
        let mut bytes = 0u64;
        for (index, id) in ids.iter().copied().enumerate() {
            if !self.canonical_content.contains_key(&id) {
                let entry = &self.mirror.loops[&id];
                let (slot, version, length) = (entry.slot, entry.content, entry.view.length as u32);
                let track = &self.mirror.tracks[&entry.track];
                let (channels, midi) = (track.audio_channels as usize, track.midi);
                let audio = if length == 0 || channels == 0 {
                    empty_audio(channels)
                } else {
                    match self.reads.audio(id, version, slot, channels) {
                        Outcome::Done(audio) => audio,
                        Outcome::Reading => return Ok(self.capture_pending(index, ids.len())),
                        Outcome::Failed(message) => bail!(message),
                    }
                };
                let events = if length == 0 || !midi {
                    empty_midi(version, midi)
                } else {
                    match self.reads.midi(id, version, slot) {
                        Outcome::Done(midi) => midi,
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
                            samples: channel.samples.to_vec(),
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
            bytes = bytes
                .checked_add(canonical_bytes(&self.canonical_content[&id])?)
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
            .collect::<std::collections::BTreeMap<_, _>>();
        let tracks = self
            .mirror
            .tracks
            .iter_mut()
            .filter(|(_, track)| track.confirmed && !track.removing)
            .map(|(id, track)| {
                let loops = track
                    .loops
                    .iter()
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
                    topology: BackendTrackTopology::Direct {
                        audio_channels: track.audio_channels,
                        midi: track.midi,
                    },
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
            buses: vec![BackendSessionBus {
                source_id: MASTER_BUS_ID.raw(),
                name: MASTER_BUS_NAME.to_owned(),
                channels: MASTER_BUS_CHANNEL_IDS
                    .into_iter()
                    .zip(MASTER_BUS_CHANNEL_LABELS)
                    .zip(MASTER_BUS_OUTPUT_PORT_IDS)
                    .map(|((id, label), output)| BackendSessionBusChannel {
                        source_id: id.raw(),
                        label: label.to_owned(),
                        output_port: ports[&output].clone(),
                    })
                    .collect(),
                gain_db: self.mirror.master.gain_db,
                balance: self.mirror.master.balance,
                muted: self.mirror.master.muted,
                processor_type: None,
                processor_state: None,
                builtin_fx_midi_cc_assignments: Vec::new(),
            }],
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
