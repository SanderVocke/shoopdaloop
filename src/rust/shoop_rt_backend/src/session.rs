//! Replacing the session with a structure-only document of the core set. A document is
//! checked completely before anything changes, so one that needs anything else is rejected
//! whole.

use alloc::borrow::ToOwned;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::vec::Vec;

use anyhow::{anyhow, Result};
use shoop_app_api::BackendCapability;
use shoop_backend::{
    Backend, BackendBusControl, BackendChannelMode, BackendConnectionFailure,
    BackendDefaultPlaybackMode, BackendPortDataType, BackendPortDirection, BackendPortOwner,
    BackendPortRole, BackendSessionBus, BackendSessionData, BackendSessionReplacement,
    BackendSessionTrack, BackendTrackControl, BackendTrackTopology, GLOBAL_FX_PORT_ID,
};
use shoop_rt_transport::carrier::Carrier;

use crate::{unavailable, EngineClock, RtBackend};

fn track_ports(
    topology: &BackendTrackTopology,
) -> Vec<(BackendPortDataType, BackendPortDirection, BackendPortRole)> {
    use BackendPortDataType::{Audio, Midi};
    use BackendPortDirection::{Input, Output};
    use BackendPortRole::*;
    let mut ports = Vec::new();
    let direct = matches!(topology, BackendTrackTopology::Direct { .. });
    let external = matches!(topology, BackendTrackTopology::DryWetExternal { .. });
    for _ in 0..topology.dry_audio_channels() {
        ports.push((Audio, Input, AudioInput));
        if direct {
            ports.push((Audio, Output, AudioOutput));
        } else if external {
            ports.push((Audio, Output, AudioSend));
        }
    }
    if !direct {
        for _ in 0..topology.wet_audio_channels() {
            if external {
                ports.push((Audio, Input, AudioReturn));
            }
            ports.push((Audio, Output, AudioOutput));
        }
    }
    if topology.has_midi() {
        ports.push((Midi, Input, MidiInput));
        if direct {
            ports.push((Midi, Output, MidiOutput));
        } else if external {
            ports.push((Midi, Output, MidiSend));
        }
    }
    ports
}

fn check_bus(bus: &BackendSessionBus, max_channels: usize) -> Result<()> {
    if bus.name.trim().is_empty() || bus.channels.is_empty() || bus.channels.len() > max_channels {
        return Err(unavailable(BackendCapability::BusManagement));
    }
    if bus.processor_type.is_some()
        || bus.processor_state.is_some()
        || !bus.builtin_fx_midi_cc_assignments.is_empty()
    {
        return Err(unavailable(BackendCapability::BusFx));
    }
    let owner = BackendPortOwner::Bus(shoop_backend::BackendBusId::from_raw(bus.source_id));
    let shaped = bus.channels.iter().all(|channel| {
        let port = &channel.output_port.descriptor;
        !channel.label.is_empty()
            && port.owner == owner
            && port.data_type == BackendPortDataType::Audio
            && port.direction == BackendPortDirection::Output
            && port.role == BackendPortRole::AudioOutput
    });
    if !shaped || !bus.gain_db.is_finite() || !bus.balance.is_finite() {
        return Err(anyhow!("a session bus is malformed"));
    }
    Ok(())
}

fn check_track(track: &BackendSessionTrack, max_channels: u32) -> Result<()> {
    let dry = track.topology.dry_audio_channels();
    let wet = track.topology.wet_audio_channels();
    let midi = track.topology.has_midi();
    let direct = matches!(track.topology, BackendTrackTopology::Direct { .. });
    if track.state.topology != track.topology
        || track.state.audio_channels != wet
        || track.state.midi != midi
    {
        return Err(anyhow!("the session's track topology is inconsistent"));
    }
    if dry > max_channels || wet > max_channels {
        return Err(anyhow!(
            "tracks of this engine have at most {max_channels} audio channels"
        ));
    }
    if track.state.default_playback_mode == BackendDefaultPlaybackMode::DryThroughWet
        && (direct || wet == 0)
    {
        return Err(anyhow!("this track cannot default to dry playback"));
    }
    if let BackendTrackTopology::DryWetProcessor { .. } = &track.topology {
        let kind = crate::processor_controls::topology_kind(&track.topology)
            .ok_or_else(|| unavailable(BackendCapability::TrackFx))?;
        if dry == 0
            || dry != wet
            || !midi
            || (kind == shoop_rt_protocol::ProcessorKind::OxiSynth && dry != 2)
        {
            return Err(anyhow!("invalid audio/MIDI shape for this processor"));
        }
    }
    crate::processor_controls::session_state(track)?;
    let expected = track_ports(&track.topology);
    if track.ports.len() != expected.len()
        || track.ports.iter().zip(&expected).any(|(port, shape)| {
            let port = &port.descriptor;
            port.owner != BackendPortOwner::Track
                || (port.data_type, port.direction, port.role) != *shape
        })
    {
        return Err(anyhow!(
            "the session's track ports do not match its topology"
        ));
    }
    for content in &track.loops {
        let audio_count = if direct { wet } else { dry + wet } as usize;
        if content.audio.len() != audio_count
            || content.midi.len() != usize::from(midi)
            || content.audio.iter().enumerate().any(|(i, channel)| {
                channel.mode
                    != if direct {
                        BackendChannelMode::Direct
                    } else if i < dry as usize {
                        BackendChannelMode::Dry
                    } else {
                        BackendChannelMode::Wet
                    }
            })
            || content.midi.iter().any(|channel| {
                channel.mode
                    != if direct {
                        BackendChannelMode::Direct
                    } else {
                        BackendChannelMode::Dry
                    }
            })
            || !content.gain.is_finite()
            || !content.balance.is_finite()
        {
            return Err(anyhow!(
                "the session's loop channels do not match their track"
            ));
        }
        crate::processor_controls::recorded_state(
            &track.topology,
            content.recorded_processor_state.as_deref(),
        )?;
    }
    let state = &track.state;
    if ![
        state.output_gain_db,
        state.output_balance,
        state.input_gain_db,
        state.input_balance,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        return Err(anyhow!("the session's track controls are not finite"));
    }
    Ok(())
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn start_replacement(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        self.start_replacement_with_sync(session, &BTreeMap::new())
    }
    pub(crate) fn start_replacement_with_sync(
        &mut self,
        session: &BackendSessionData,
        sync: &BTreeMap<u64, u64>,
    ) -> Result<BackendSessionReplacement> {
        use crate::{
            mirror::db_amplitude,
            persistence::{Job, Work},
            prepared::{Upload, MAX_MEDIA_BYTES},
            requests::Purpose,
        };
        use alloc::{collections::VecDeque, sync::Arc};
        use shoop_rt_protocol::{Capabilities, ReplacementEdit, Request, StripPreset};
        self.ready()?;
        if !self.requests.is_empty() || self.replacement.is_some() {
            return Err(anyhow!(
                "settle previous engine operations before replacing the session"
            ));
        }
        if !self
            .capabilities
            .contains(Capabilities::COMPLETE_REPLACEMENT)
        {
            return Err(unavailable(BackendCapability::SessionContentLoad));
        }
        self.check_session(session)?;
        if session
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .flat_map(|content| &content.midi)
            .map(|channel| channel.events.len())
            .sum::<usize>()
            > crate::prepared::MAX_MIDI_EVENTS
        {
            return Err(anyhow!("session exceeds the one-million-event MIDI budget"));
        }
        if self.mirror.loops.values().any(|entry| {
            entry.view.mode.records()
                || entry
                    .view
                    .next_mode
                    .is_some_and(shoop_rt_protocol::LoopMode::records)
        }) {
            return Err(anyhow!(
                "stop recording and settle pending recording transitions before loading a session"
            ));
        }
        let ids = session
            .tracks
            .iter()
            .flat_map(|track| track.loops.iter().map(|content| content.source_id))
            .collect::<BTreeSet<_>>();
        for (&target, &source) in sync {
            if !ids.contains(&target) || !ids.contains(&source) {
                return Err(anyhow!("session sync references an unknown loop"));
            }
            let mut current = source;
            for steps in 0..=ids.len() {
                if current == target || steps == ids.len() {
                    return Err(anyhow!("session sync contains a cycle"));
                }
                let Some(next) = sync.get(&current) else {
                    break;
                };
                current = *next;
            }
        }
        let image = self.mirror.replacement_image();
        let live = core::mem::replace(&mut self.mirror, image);
        let requests = core::mem::take(&mut self.requests);
        let failures = core::mem::take(&mut self.connection_failures);
        let mapping = self.build_structure(session);
        let staged = core::mem::replace(&mut self.requests, requests);
        let failures = core::mem::replace(&mut self.connection_failures, failures);
        let mut candidate = core::mem::replace(&mut self.mirror, live);
        self.mirror.reserve_image_identities(&candidate);
        let mapping = mapping?;
        for pending in &staged.outgoing {
            match &pending.purpose {
                Purpose::StripControl(id, control) => candidate
                    .tracks
                    .get_mut(id)
                    .expect("candidate track")
                    .controls
                    .apply(*control),
                Purpose::CreateBus(id) => {
                    candidate
                        .buses
                        .get_mut(id)
                        .expect("candidate bus")
                        .confirmed = true
                }
                Purpose::BusControl(id, control) => candidate
                    .buses
                    .get_mut(id)
                    .expect("candidate bus")
                    .apply(*control),
                Purpose::LoopGain(id, gain) => {
                    candidate.loops.get_mut(id).expect("candidate loop").gain = *gain
                }
                Purpose::LoopBalance(id, balance) => {
                    candidate.loops.get_mut(id).expect("candidate loop").balance = *balance
                }
                Purpose::Connect {
                    port,
                    host,
                    connected: true,
                } => {
                    candidate.links.insert(shoop_backend::BackendConfirmedLink {
                        application_port_id: *port,
                        host_port_id: host.clone(),
                    });
                }
                Purpose::Route {
                    link,
                    connected: true,
                } => {
                    candidate.routes.insert(link.clone());
                }
                _ => {}
            }
        }
        let id = self.next_persistence;
        self.next_persistence = id
            .checked_add(1)
            .ok_or_else(|| anyhow!("persistence identity exhausted"))?;
        let mut work = VecDeque::new();
        work.push_back(Work::Request(Request::ReplacementBegin { id }));
        let mut contents = BTreeMap::new();
        let mut uploads = Vec::new();
        let mut sources = BTreeMap::new();
        let mut prepared_bytes = 0u64;
        for (track, content) in session
            .tracks
            .iter()
            .flat_map(|track| track.loops.iter().map(move |content| (track, content)))
        {
            let loop_id = mapping.loops[&content.source_id];
            let canonical = Arc::new(content.clone());
            let roles = !matches!(track.topology, BackendTrackTopology::Direct { .. });
            let mut pair = [None; 3];
            if content.length > 0 {
                for medium in 0..if roles { 3 } else { 2 } {
                    let token = self.next_persistence;
                    self.next_persistence = token
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("persistence identity exhausted"))?;
                    let width = usize::from(self.limits().strip_audio_channels);
                    let upload = if roles && medium != 1 {
                        Upload::audio_role(
                            token,
                            Arc::clone(&canonical),
                            width,
                            if medium == 0 {
                                BackendChannelMode::Wet
                            } else {
                                BackendChannelMode::Dry
                            },
                        )?
                    } else {
                        Upload::new(token, medium == 1, Arc::clone(&canonical), width)?
                    };
                    prepared_bytes = prepared_bytes
                        .checked_add(upload.description.bytes)
                        .ok_or_else(|| anyhow!("prepared media size overflow"))?;
                    if prepared_bytes > MAX_MEDIA_BYTES {
                        return Err(anyhow!(
                            "session engine image exceeds the 256 MiB prepared media budget"
                        ));
                    }
                    pair[medium] = Some(token);
                    uploads.push(token);
                    work.push_back(Work::Upload {
                        upload,
                        begun: false,
                    });
                }
            }
            candidate
                .loops
                .get_mut(&loop_id)
                .expect("candidate loop")
                .view
                .length = u64::from(content.length);
            sources.insert(loop_id, pair);
            contents.insert(loop_id, canonical);
        }
        let processor_states = session
            .tracks
            .iter()
            .map(|track| {
                Ok((
                    mapping.tracks[&track.source_id].track_id,
                    crate::processor_controls::session_state(track)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        for (track_id, track) in &mut candidate.tracks {
            track.confirmed = true;
            let controls = track.controls;
            let shape = shoop_rt_protocol::StripShape {
                audio_inputs: track.topology.dry_audio_channels() as u8,
                audio_outputs: track.audio_channels as u8,
                midi_input: track.midi,
                midi_output: track.midi
                    && matches!(track.topology, BackendTrackTopology::Direct { .. }),
            };
            let controls = StripPreset {
                input_gain: db_amplitude(controls.input_gain_db),
                input_balance: controls.input_balance,
                monitoring: controls.input_monitoring,
                output_gain: db_amplitude(controls.output_gain_db),
                output_balance: controls.output_balance,
                output_mute: controls.output_muted,
            };
            let kind = match track.topology {
                BackendTrackTopology::Direct { .. } => None,
                BackendTrackTopology::DryWetExternal { .. } => {
                    Some(shoop_rt_protocol::ProcessorKind::External)
                }
                _ => crate::processor_controls::track_kind(track),
            };
            let edit = match kind {
                Some(kind) => ReplacementEdit::ProcessedStrip {
                    strip: track.strip,
                    shape,
                    controls,
                    kind,
                    default_dry: track.default_playback_mode
                        == BackendDefaultPlaybackMode::DryThroughWet,
                    state: processor_states[track_id],
                },
                None => ReplacementEdit::Strip {
                    strip: track.strip,
                    shape,
                    controls,
                },
            };
            work.push_back(Work::Request(Request::ReplacementEdit { id, edit }));
        }
        let sync_slots = sync
            .iter()
            .map(|(target, source)| {
                (
                    mapping.loops[target],
                    candidate.loops[&mapping.loops[source]].slot,
                )
            })
            .collect::<BTreeMap<_, _>>();
        for (&loop_id, entry) in &mut candidate.loops {
            entry.confirmed = true;
            entry.sync = sync.iter().find_map(|(target, source)| {
                (mapping.loops[target] == loop_id).then_some(mapping.loops[source])
            });
            let pair = sources[&loop_id];
            work.push_back(Work::Request(Request::ReplacementEdit {
                id,
                edit: ReplacementEdit::Loop {
                    loop_id: entry.slot,
                    strip: candidate.tracks[&entry.track].strip,
                    sync: sync_slots.get(&loop_id).copied(),
                    frames: entry.view.length,
                    dry_audio: pair[2],
                    recorded_state: crate::processor_controls::recorded_state(
                        &candidate.tracks[&entry.track].topology,
                        contents[&loop_id].recorded_processor_state.as_deref(),
                    )?,
                    audio: pair[0],
                    midi: pair[1],
                    gain: entry.gain,
                    balance: entry.balance,
                },
            }));
        }
        for pending in staged.outgoing {
            let edit = match pending.request {
                Request::Connect {
                    port,
                    host,
                    connected: true,
                } => Some(ReplacementEdit::Connect { port, host }),
                Request::Route {
                    port,
                    bus,
                    connected: true,
                } => Some(ReplacementEdit::Route { port, bus }),
                Request::CreateBus { bus, channels } => {
                    Some(ReplacementEdit::Bus { bus, channels })
                }
                Request::BusControl { bus, control } => {
                    Some(ReplacementEdit::BusControl { bus, control })
                }
                _ => None,
            };
            if let Some(edit) = edit {
                work.push_back(Work::Request(Request::ReplacementEdit { id, edit }));
            }
        }
        work.push_back(Work::Request(Request::ReplacementPrepare { id }));
        work.push_back(Work::Request(Request::ReplacementCommit { id }));
        self.replacement = Some(crate::Replacement {
            mapping: mapping.clone(),
            candidate: Some(candidate),
            job: Job::new(
                id,
                work,
                uploads,
                true,
                contents.values().cloned(),
                usize::from(self.limits().strip_audio_channels),
                MAX_MEDIA_BYTES,
            )?,
            content: contents,
            connection_failures: failures,
            loop_target: None,
            published: false,
        });
        Ok(mapping)
    }

    fn check_session(&self, session: &BackendSessionData) -> Result<()> {
        let sample_rate = self.driver.map_or(0, |driver| driver.sample_rate);
        if session.sample_rate != sample_rate {
            return Err(anyhow!(
                "the session's sample rate {} differs from the engine's {sample_rate}",
                session.sample_rate
            ));
        }
        match session.global_ports.as_slice() {
            [port]
                if port.descriptor.owner == BackendPortOwner::GlobalFxControl
                    && port.descriptor.data_type == BackendPortDataType::Midi
                    && port.descriptor.direction == BackendPortDirection::Input => {}
            _ => return Err(anyhow!("the session's global FX control port is invalid")),
        }
        if session.buses.len() > self.limits().buses as usize {
            return Err(anyhow!("session exceeds prepared bus count"));
        }
        let mut bus_ids = BTreeSet::new();
        let mut channel_ids = BTreeSet::new();
        for bus in &session.buses {
            check_bus(bus, self.limits().bus_channels as usize)?;
            if !bus_ids.insert(bus.source_id)
                || bus
                    .channels
                    .iter()
                    .any(|c| !channel_ids.insert(c.source_id))
            {
                return Err(anyhow!("duplicate bus or channel identity"));
            }
        }
        let limits = self.limits();
        for track in &session.tracks {
            check_track(track, u32::from(limits.strip_audio_channels))?;
        }
        for kind in [
            shoop_rt_protocol::ProcessorKind::BuiltInFx,
            shoop_rt_protocol::ProcessorKind::OxiSynth,
        ] {
            let requested = session
                .tracks
                .iter()
                .filter(|track| {
                    crate::processor_controls::topology_kind(&track.topology) == Some(kind)
                })
                .count();
            let available = self.processors.map_or(0, |limits| match kind {
                shoop_rt_protocol::ProcessorKind::BuiltInFx => limits.effective.fx_instances,
                _ => limits.effective.synth_instances,
            });
            if requested > available as usize {
                return Err(anyhow!("session requires {requested} {kind:?} instances; the host prepared {available}"));
            }
        }
        if session
            .tracks
            .iter()
            .any(|track| matches!(track.topology, BackendTrackTopology::DryWetExternal { .. }))
            && !self
                .capabilities
                .contains(shoop_rt_protocol::Capabilities::EXTERNAL_DRY_WET)
        {
            return Err(unavailable(BackendCapability::DryWetTracks));
        }
        let loops = session
            .tracks
            .iter()
            .map(|track| track.loops.len())
            .sum::<usize>();
        let track_ids = session
            .tracks
            .iter()
            .map(|track| track.source_id)
            .collect::<BTreeSet<_>>();
        let loop_ids = session
            .tracks
            .iter()
            .flat_map(|track| track.loops.iter().map(|content| content.source_id))
            .collect::<BTreeSet<_>>();
        if track_ids.len() != session.tracks.len() || loop_ids.len() != loops {
            return Err(anyhow!(
                "session contains duplicate track or loop identities"
            ));
        }
        if session.tracks.len() > self.mirror.strip_slots.capacity() {
            return Err(anyhow!(
                "the session has {} tracks; the engine has {} prepared track slots",
                session.tracks.len(),
                self.mirror.strip_slots.capacity()
            ));
        }
        if loops > self.mirror.loop_slots.capacity() {
            return Err(anyhow!(
                "the session has {loops} loops; the engine has {} prepared loop slots",
                self.mirror.loop_slots.capacity()
            ));
        }
        let mut ports = BTreeMap::new();
        for track in &session.tracks {
            for port in &track.ports {
                if ports.insert(port.source_id, port.descriptor.role).is_some() {
                    return Err(anyhow!("the session has duplicate port identities"));
                }
            }
        }
        let channels = session
            .buses
            .iter()
            .flat_map(|bus| bus.channels.iter().map(|channel| channel.source_id))
            .collect::<BTreeSet<_>>();
        for route in &session.mixer_routes {
            if ports.get(&route.source_port_id) != Some(&BackendPortRole::AudioOutput) {
                return Err(anyhow!("a session mixer route has an invalid source"));
            }
            if !channels.contains(&route.destination_channel_id) {
                return Err(anyhow!("a session mixer route has an invalid destination"));
            }
        }
        Ok(())
    }

    fn restore_connection(&mut self, port: shoop_backend::BackendPortId, external: &str) {
        self.mirror
            .desired_links
            .insert(shoop_backend::BackendConfirmedLink {
                application_port_id: port,
                host_port_id: external.to_owned(),
            });
        if let Err(error) = self.queue_connection(port, external, true) {
            self.connection_failures.push(BackendConnectionFailure {
                port_id: port,
                external_port: external.to_owned(),
                desired_connected: true,
                message: format!("could not restore the connection to {external}: {error}"),
            });
            self.mirror.connection_revision += 1;
        }
    }

    /// Checks the document, then queues the replacement and returns its identities.
    fn build_structure(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        self.ready()?;
        self.check_session(session)?;
        tracing::debug!(
            tracks = session.tracks.len(),
            "backend.rt.session_replacement"
        );
        let mut replacement = BackendSessionReplacement::default();

        let existing = self.mirror.tracks.keys().copied().collect::<Vec<_>>();
        for track in existing {
            self.remove_strip(track);
        }

        let global = &session.global_ports[0];
        replacement
            .global_ports
            .insert(global.source_id, GLOBAL_FX_PORT_ID);
        self.mirror
            .links
            .retain(|link| link.application_port_id != GLOBAL_FX_PORT_ID);
        for external in &global.external_connections {
            self.restore_connection(GLOBAL_FX_PORT_ID, external);
        }

        let existing = self.mirror.buses.keys().copied().collect::<Vec<_>>();
        for id in existing {
            self.remove_bus(id)?;
        }
        for bus in &session.buses {
            let created = self.create_bus(shoop_backend::BackendBusRequest {
                name: bus.name.clone(),
                channel_count: bus.channels.len() as u32,
                fx: None,
            })?;
            replacement.buses.insert(bus.source_id, created.bus_id);
            for control in [
                BackendBusControl::GainDb(bus.gain_db),
                BackendBusControl::Mute(bus.muted),
            ] {
                self.queue_bus_control(created.bus_id, control)?;
            }
            if bus.channels.len() == 2 {
                self.queue_bus_control(created.bus_id, BackendBusControl::Balance(bus.balance))?;
            }
            for (channel, live) in bus.channels.iter().zip(&created.channels) {
                replacement.bus_channels.insert(channel.source_id, live.id);
                replacement
                    .bus_output_ports
                    .insert(channel.source_id, live.output_port_id);
                replacement
                    .ports
                    .insert(channel.output_port.source_id, live.output_port_id);
                for external in &channel.output_port.external_connections {
                    self.restore_connection(live.output_port_id, external);
                }
            }
        }

        for track in &session.tracks {
            let mut created = self.create_track_topology(
                &track.port_name_base,
                track.topology.clone(),
                track.loops.len(),
            )?;
            self.mirror
                .tracks
                .get_mut(&created.track_id)
                .expect("candidate track")
                .default_playback_mode = track.state.default_playback_mode;
            let state = &track.state;
            for control in [
                BackendTrackControl::OutputGainDb(state.output_gain_db),
                BackendTrackControl::OutputBalance(state.output_balance),
                BackendTrackControl::OutputMute(state.output_muted),
                BackendTrackControl::InputGainDb(state.input_gain_db),
                BackendTrackControl::InputBalance(state.input_balance),
                BackendTrackControl::InputMonitoring(state.input_monitoring),
            ] {
                self.queue_track_control(created.track_id, control)?;
            }
            for (content, loop_id) in track.loops.iter().zip(&created.loops) {
                replacement.loops.insert(content.source_id, *loop_id);
                let slot = self.mirror.loops[loop_id].slot;
                let gain = content.gain;
                let balance = content.balance;
                self.requests.push(
                    shoop_rt_protocol::Request::LoopGain {
                        loop_id: slot,
                        gain,
                    },
                    crate::requests::Purpose::LoopGain(*loop_id, gain),
                );
                self.requests.push(
                    shoop_rt_protocol::Request::LoopBalance {
                        loop_id: slot,
                        balance,
                    },
                    crate::requests::Purpose::LoopBalance(*loop_id, balance),
                );
            }
            for (source, port) in track.ports.iter().zip(&mut created.ports) {
                port.name.clone_from(&source.descriptor.name);
                self.mirror
                    .ports
                    .get_mut(&port.id)
                    .expect("created port")
                    .descriptor
                    .name
                    .clone_from(&source.descriptor.name);
                replacement.ports.insert(source.source_id, port.id);
                for external in &source.external_connections {
                    self.restore_connection(port.id, external);
                }
            }
            replacement.tracks.insert(track.source_id, created);
        }

        for route in &session.mixer_routes {
            let source = replacement.ports[&route.source_port_id];
            let destination = replacement.bus_channels[&route.destination_channel_id];
            self.queue_route(source, destination, true)?;
        }
        Ok(replacement)
    }
}
