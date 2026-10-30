//! Replacing the session with a structure-only document of the core set. A document is
//! checked completely before anything changes, so one that needs anything else is rejected
//! whole.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use shoop_app_api::BackendCapability;
use shoop_backend::{
    session_has_loop_content, BackendBusControl, BackendChannelMode, BackendConnectionFailure,
    BackendDefaultPlaybackMode, BackendPortDataType, BackendPortDirection, BackendPortOwner,
    BackendPortRole, BackendSessionBus, BackendSessionData, BackendSessionReplacement,
    BackendSessionTrack, BackendTrackControl, BackendTrackTopology, GLOBAL_FX_PORT_ID,
    MASTER_BUS_CHANNEL_IDS, MASTER_BUS_CHANNEL_LABELS, MASTER_BUS_ID, MASTER_BUS_NAME,
    MASTER_BUS_OUTPUT_PORT_IDS,
};
use shoop_rt_protocol::carrier::Carrier;

use crate::{unavailable, EngineClock, RtBackend};

/// The port layout of a direct track, in creation order.
fn direct_ports(
    audio_channels: u32,
    midi: bool,
) -> Vec<(BackendPortDataType, BackendPortDirection, BackendPortRole)> {
    use BackendPortDataType::{Audio, Midi};
    use BackendPortDirection::{Input, Output};
    let mut ports = Vec::new();
    for _ in 0..audio_channels {
        ports.push((Audio, Input, BackendPortRole::AudioInput));
        ports.push((Audio, Output, BackendPortRole::AudioOutput));
    }
    if midi {
        ports.push((Midi, Input, BackendPortRole::MidiInput));
        ports.push((Midi, Output, BackendPortRole::MidiOutput));
    }
    ports
}

fn check_master(bus: &BackendSessionBus) -> Result<()> {
    if bus.name != MASTER_BUS_NAME || bus.channels.len() != MASTER_BUS_CHANNEL_IDS.len() {
        return Err(unavailable(BackendCapability::BusManagement));
    }
    if bus.processor_type.is_some()
        || bus.processor_state.is_some()
        || !bus.builtin_fx_midi_cc_assignments.is_empty()
    {
        return Err(unavailable(BackendCapability::BusFx));
    }
    let owner = BackendPortOwner::Bus(shoop_backend::BackendBusId::from_raw(bus.source_id));
    let shaped = bus
        .channels
        .iter()
        .zip(MASTER_BUS_CHANNEL_LABELS)
        .all(|(channel, label)| {
            let port = &channel.output_port.descriptor;
            channel.label == label
                && port.owner == owner
                && port.data_type == BackendPortDataType::Audio
                && port.direction == BackendPortDirection::Output
                && port.role == BackendPortRole::AudioOutput
        });
    if !shaped || !bus.gain_db.is_finite() || !bus.balance.is_finite() {
        return Err(anyhow!("the session's Master bus is malformed"));
    }
    Ok(())
}

fn check_track(track: &BackendSessionTrack, max_channels: u32) -> Result<()> {
    let BackendTrackTopology::Direct {
        audio_channels,
        midi,
    } = track.topology
    else {
        return Err(unavailable(BackendCapability::DryWetTracks));
    };
    if track.state.topology != track.topology {
        return Err(anyhow!("the session's track topology is inconsistent"));
    }
    if track.processor_state.is_some()
        || track.state.fx.is_some()
        || !track.builtin_fx_midi_cc_assignments.is_empty()
        || !track.oxisynth_midi_cc_assignments.is_empty()
    {
        return Err(unavailable(BackendCapability::TrackFx));
    }
    if track.state.default_playback_mode != BackendDefaultPlaybackMode::Regular {
        return Err(unavailable(BackendCapability::DryWetModes));
    }
    if audio_channels > max_channels {
        return Err(anyhow!(
            "tracks of this engine have at most {max_channels} audio channels"
        ));
    }
    let expected = direct_ports(audio_channels, midi);
    if track.ports.len() != expected.len()
        || track.ports.iter().zip(&expected).any(|(port, shape)| {
            let port = &port.descriptor;
            port.owner != BackendPortOwner::Track
                || (port.data_type, port.direction, port.role) != *shape
        })
    {
        return Err(anyhow!(
            "the session's track ports do not match a direct track"
        ));
    }
    let direct = |mode: BackendChannelMode| mode == BackendChannelMode::Direct;
    for content in &track.loops {
        if content.audio.len() != audio_channels as usize
            || content.midi.len() != usize::from(midi)
            || !content.audio.iter().all(|channel| direct(channel.mode))
            || !content.midi.iter().all(|channel| direct(channel.mode))
            || !content.gain.is_finite()
            || !content.balance.is_finite()
        {
            return Err(anyhow!(
                "the session's loop channels do not match their track"
            ));
        }
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
    fn check_session(&self, session: &BackendSessionData) -> Result<()> {
        if session_has_loop_content(session) {
            return Err(unavailable(BackendCapability::SessionContentLoad));
        }
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
        match session.buses.as_slice() {
            [] => {}
            [bus] => check_master(bus)?,
            _ => return Err(unavailable(BackendCapability::BusManagement)),
        }
        let limits = self.limits();
        for track in &session.tracks {
            check_track(track, u32::from(limits.strip_audio_channels))?;
        }
        let loops = session
            .tracks
            .iter()
            .map(|track| track.loops.len())
            .sum::<usize>();
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
    pub(crate) fn start_replacement(
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

        let master = session.buses.first();
        let (gain_db, balance, muted) = master.map_or((0.0, 0.0, false), |bus| {
            (bus.gain_db, bus.balance, bus.muted)
        });
        for control in [
            BackendBusControl::GainDb(gain_db),
            BackendBusControl::Balance(balance),
            BackendBusControl::Mute(muted),
        ] {
            self.queue_master_control(control)?;
        }
        let mut master_links = BTreeSet::new();
        if let Some(bus) = master {
            replacement.buses.insert(bus.source_id, MASTER_BUS_ID);
            for (index, channel) in bus.channels.iter().enumerate() {
                let port = MASTER_BUS_OUTPUT_PORT_IDS[index];
                replacement
                    .bus_channels
                    .insert(channel.source_id, MASTER_BUS_CHANNEL_IDS[index]);
                replacement.bus_output_ports.insert(channel.source_id, port);
                replacement
                    .ports
                    .insert(channel.output_port.source_id, port);
                for external in &channel.output_port.external_connections {
                    master_links.insert((port, external.clone()));
                }
            }
        }
        let stale = self
            .mirror
            .links
            .iter()
            .filter(|link| MASTER_BUS_OUTPUT_PORT_IDS.contains(&link.application_port_id))
            .map(|link| (link.application_port_id, link.host_port_id.clone()))
            .filter(|link| !master_links.contains(link))
            .collect::<Vec<_>>();
        for (port, host) in stale {
            if self.queue_connection(port, &host, false).is_err() {
                self.mirror
                    .links
                    .remove(&shoop_backend::BackendConfirmedLink {
                        application_port_id: port,
                        host_port_id: host,
                    });
            }
        }
        for (port, host) in master_links {
            self.restore_connection(port, &host);
        }

        for track in &session.tracks {
            let BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } = track.topology
            else {
                unreachable!("checked topology");
            };
            let created = self.create_strip(
                &track.port_name_base,
                audio_channels,
                midi,
                track.loops.len(),
            )?;
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
                let gain = content.gain.clamp(0.0, 1.0);
                let balance = content.balance.clamp(-1.0, 1.0);
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
            for (source, port) in track.ports.iter().zip(&created.ports) {
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
