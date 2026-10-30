//! The `Backend` trait in terms of the mirror and queued requests. Everything outside the
//! core set is refused with the capability's explanation.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use shoop_app_api::{
    AudioDriverConfig, AudioDriverDescriptor, AudioDriverKind, AudioDriverRuntimeState,
    BackendCapabilities, BackendCapability, CpalAudioDriverConfig, DummyAudioDriverConfig,
    ResolvedAudioDriverConfig,
};
use shoop_backend::{
    Backend, BackendAsyncResult, BackendAudioData, BackendAudioDataChunk, BackendBusChannelId,
    BackendBusControl, BackendBusCreation, BackendBusFxControl, BackendBusId, BackendBusRequest,
    BackendCompositeConfig, BackendCompositeId, BackendConnectionSnapshot,
    BackendDefaultPlaybackMode, BackendDriverState, BackendGrabRequest, BackendLoopContentUpdate,
    BackendLoopId, BackendLoopMode, BackendMidiData, BackendMidiEvent, BackendMixerSnapshot,
    BackendOperationKind, BackendOperationProgress, BackendPortId,
    BackendProcessorLatencyAdjustment, BackendRecordingOffsetAdjustment, BackendSessionData,
    BackendSessionReplacement, BackendSnapshot, BackendStatus, BackendTrackControl,
    BackendTrackCreation, BackendTrackFxControl, BackendTrackId, BackendTrackTopology,
    DirectTrackRequest, TrackRequest, MASTER_BUS_ID,
};
use shoop_rt_protocol::carrier::Carrier;
use shoop_rt_protocol::{Capabilities, DriverKind, DriverRunState, LoopMode, MidiMessage, Request};

use crate::mirror::Mirror;
use crate::reads::{empty_audio, empty_midi, Outcome};
use crate::requests::Purpose;
use crate::{unavailable, EngineClock, Link, RtBackend};

/// Most events in one MIDI input injection, as in the legacy backends.
const MAX_INJECTED_EVENTS: usize = 128;

fn protocol_mode(mode: BackendLoopMode) -> Result<LoopMode> {
    match mode {
        BackendLoopMode::Stopped => Ok(LoopMode::Stopped),
        BackendLoopMode::Playing => Ok(LoopMode::Playing),
        BackendLoopMode::Recording => Ok(LoopMode::Recording),
        BackendLoopMode::Replacing => Err(unavailable(BackendCapability::ReplacingMode)),
        BackendLoopMode::PlayingDryThroughWet | BackendLoopMode::RecordingDryIntoWet => {
            Err(unavailable(BackendCapability::DryWetModes))
        }
        BackendLoopMode::Unknown => Err(anyhow!("a loop cannot transition to an unknown mode")),
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    fn driver_state(&self) -> BackendDriverState {
        match (&self.link, self.driver) {
            (Link::Disconnected(_), _) => BackendDriverState::Failed,
            (Link::Connecting | Link::Resyncing, _) | (_, None) => BackendDriverState::Starting,
            (Link::Ready, Some(driver)) => match driver.state {
                DriverRunState::Starting => BackendDriverState::Starting,
                DriverRunState::Running if driver.kind == DriverKind::Dummy => {
                    BackendDriverState::Dummy
                }
                DriverRunState::Running => BackendDriverState::Running,
                DriverRunState::Stopped => BackendDriverState::Stopped,
                DriverRunState::Failed => BackendDriverState::Failed,
            },
        }
    }

    fn driver_runtime_state(&self) -> AudioDriverRuntimeState {
        let Some(driver) = self.driver else {
            return AudioDriverRuntimeState::default();
        };
        let (kind, configured) = match driver.kind {
            DriverKind::Dummy => (
                AudioDriverKind::Dummy,
                AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate: driver.sample_rate,
                    buffer_size: driver.buffer_frames,
                }),
            ),
            DriverKind::Cpal => (
                AudioDriverKind::Cpal,
                AudioDriverConfig::Cpal(CpalAudioDriverConfig {
                    sample_rate: driver.sample_rate,
                    buffer_size: driver.buffer_frames,
                    ..Default::default()
                }),
            ),
            DriverKind::WebAudio => (AudioDriverKind::WebAudio, AudioDriverConfig::WebAudio),
        };
        AudioDriverRuntimeState {
            supported: false,
            catalog: Arc::from([AudioDriverDescriptor {
                kind,
                available: true,
                ..Default::default()
            }]),
            active: Some(ResolvedAudioDriverConfig {
                configured,
                sample_rate: driver.sample_rate,
                buffer_size: driver.buffer_frames,
                instance_name: "New engine".to_owned(),
            }),
            switch: Default::default(),
        }
    }

    fn live_loop_slot(&self, loop_id: BackendLoopId) -> Result<shoop_rt_protocol::LoopSlotId> {
        self.mirror
            .live_loop(loop_id)
            .map(|entry| entry.slot)
            .ok_or_else(|| anyhow!("unknown loop {loop_id:?}"))
    }

    /// The loop's audio content, `None` while a read is in progress.
    fn read_audio(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendAudioData>> {
        self.ready()?;
        let entry = self
            .mirror
            .live_loop(loop_id)
            .ok_or_else(|| anyhow!("unknown loop {loop_id:?}"))?;
        let channels = self.mirror.tracks[&entry.track].audio_channels as usize;
        if entry.view.mode == LoopMode::Recording {
            return Ok(None);
        }
        if entry.view.length == 0 {
            return Ok(Some(empty_audio(channels)));
        }
        let (version, slot) = (entry.content, entry.slot);
        let outcome = self.reads.audio(loop_id, version, slot, channels);
        self.pump();
        match outcome {
            Outcome::Done(data) => Ok(Some(data)),
            Outcome::Reading => Ok(None),
            Outcome::Failed(message) => {
                tracing::warn!(message = %message, "backend.rt.read_failed");
                Err(anyhow!("reading loop audio failed: {message}"))
            }
        }
    }
}

impl<C: Carrier, K: EngineClock> Backend for RtBackend<C, K> {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::NONE.with(
            BackendCapability::MidiInjection,
            self.capabilities.contains(Capabilities::MIDI_INJECTION),
        )
    }

    fn audio_driver_state(&mut self) -> Result<AudioDriverRuntimeState> {
        Ok(self.driver_runtime_state())
    }

    fn preflight_audio_driver(
        &mut self,
        _config: &AudioDriverConfig,
    ) -> Result<ResolvedAudioDriverConfig> {
        Err(unavailable(BackendCapability::AudioDriverSwitching))
    }

    fn switch_audio_driver(
        &mut self,
        _config: &AudioDriverConfig,
        _confirmed_sample_rate: u32,
        _session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        Err(unavailable(BackendCapability::AudioDriverSwitching))
    }

    fn create_loop(&mut self) -> Result<BackendLoopId> {
        Err(anyhow!("loops of this audio backend belong to a track"))
    }

    fn create_composite_loop(&mut self) -> Result<BackendCompositeId> {
        Err(unavailable(BackendCapability::CompositeLoops))
    }

    fn configure_composite_loop(
        &mut self,
        _composite_id: BackendCompositeId,
        _config: &BackendCompositeConfig,
    ) -> Result<u64> {
        Err(unavailable(BackendCapability::CompositeLoops))
    }

    fn transition_composite_loop(
        &mut self,
        _composite_id: BackendCompositeId,
        _mode: BackendLoopMode,
        _cycles_delay: Option<u32>,
        _align_to_iteration: Option<i64>,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::CompositeLoops))
    }

    fn set_composite_play_after_record(
        &mut self,
        _composite_id: BackendCompositeId,
        _enabled: bool,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::CompositeLoops))
    }

    fn remove_composite_loop(&mut self, _composite_id: BackendCompositeId) -> Result<Option<u64>> {
        Err(unavailable(BackendCapability::CompositeLoops))
    }

    fn create_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        match request.topology {
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } => self.create_direct_track(DirectTrackRequest {
                port_name_base: request.port_name_base,
                audio_channels,
                midi,
                initial_loops: request.initial_loops,
            }),
            BackendTrackTopology::DryWetExternal { .. }
            | BackendTrackTopology::DryWetProcessor { .. } => {
                Err(unavailable(BackendCapability::DryWetTracks))
            }
        }
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        let created = self.create_strip(
            &request.port_name_base,
            request.audio_channels,
            request.midi,
            request.initial_loops,
        )?;
        Ok(created)
    }

    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        self.ready()?;
        self.remove_strip(track_id);
        Ok(())
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        let loop_id = self.add_loop(track_id)?;
        Ok(loop_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        self.ready()?;
        self.queue_track_control(track_id, control)?;
        Ok(())
    }

    fn set_track_default_playback_mode(
        &mut self,
        track_id: BackendTrackId,
        mode: BackendDefaultPlaybackMode,
    ) -> Result<()> {
        self.ready()?;
        self.mirror
            .live_track(track_id)
            .ok_or_else(|| anyhow!("unknown track {track_id:?}"))?;
        match mode {
            BackendDefaultPlaybackMode::Regular => Ok(()),
            BackendDefaultPlaybackMode::DryThroughWet => {
                Err(unavailable(BackendCapability::DryWetModes))
            }
        }
    }

    fn set_track_latency(
        &mut self,
        _track_id: BackendTrackId,
        _adjustment: BackendRecordingOffsetAdjustment,
        _processor_adjustment: BackendProcessorLatencyAdjustment,
        _processor_manual_frames: i32,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::LatencyCompensation))
    }

    fn set_take_alignment(
        &mut self,
        _loop_id: BackendLoopId,
        _capture_alignment_frames: i32,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::TakeAlignment))
    }

    fn set_take_processor_alignment(
        &mut self,
        _loop_id: BackendLoopId,
        _processor_alignment_frames: u32,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::TakeAlignment))
    }

    fn inject_midi_input(
        &mut self,
        track_id: BackendTrackId,
        events: &[BackendMidiEvent],
    ) -> Result<()> {
        if !self.capabilities.contains(Capabilities::MIDI_INJECTION) {
            return Err(unavailable(BackendCapability::MidiInjection));
        }
        self.ready()?;
        let track = self
            .mirror
            .live_track(track_id)
            .ok_or_else(|| anyhow!("unknown track {track_id:?}"))?;
        if !track.midi {
            return Err(anyhow!("track {track_id:?} has no MIDI input"));
        }
        let strip = track.strip;
        if events.len() > MAX_INJECTED_EVENTS {
            return Err(anyhow!("MIDI input injection exceeds capacity"));
        }
        let messages = events
            .iter()
            .map(|event| {
                (event.time == 0 && event.data.len() <= 4)
                    .then(|| MidiMessage::new(&event.data))
                    .flatten()
                    .ok_or_else(|| anyhow!("invalid MIDI input injection event"))
            })
            .collect::<Result<Vec<_>>>()?;
        for message in messages {
            self.requests.push(
                Request::InjectMidi { strip, message },
                Purpose::InjectMidi(track_id),
            );
        }
        Ok(())
    }

    fn set_track_fx_control(
        &mut self,
        _track_id: BackendTrackId,
        _control: BackendTrackFxControl,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::TrackFx))
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        self.ready()?;
        let slot = self.live_loop_slot(loop_id)?;
        if !gain.is_finite() {
            return Err(anyhow!("loop gain must be finite"));
        }
        let gain = gain.clamp(0.0, 1.0);
        self.requests.push(
            Request::LoopGain {
                loop_id: slot,
                gain,
            },
            Purpose::LoopGain(loop_id, gain),
        );
        Ok(())
    }

    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()> {
        self.ready()?;
        let slot = self.live_loop_slot(loop_id)?;
        if !balance.is_finite() {
            return Err(anyhow!("loop balance must be finite"));
        }
        let balance = balance.clamp(-1.0, 1.0);
        self.requests.push(
            Request::LoopBalance {
                loop_id: slot,
                balance,
            },
            Purpose::LoopBalance(loop_id, balance),
        );
        Ok(())
    }

    fn grab_loops(&mut self, _requests: &[BackendGrabRequest]) -> Result<()> {
        Err(unavailable(BackendCapability::Grab))
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        Ok(self.read_audio(loop_id)?.map(|data| {
            data.channels
                .into_iter()
                .map(|channel| channel.samples)
                .collect()
        }))
    }

    fn loop_audio_data_with_metadata(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<Option<BackendAudioData>> {
        self.read_audio(loop_id)
    }

    fn loop_audio_data_chunk(
        &mut self,
        loop_id: BackendLoopId,
        channel: usize,
        offset: usize,
        max_samples: usize,
    ) -> Result<BackendAudioDataChunk> {
        let data = self
            .read_audio(loop_id)?
            .ok_or_else(|| anyhow!("loop audio is still being read; retry later"))?;
        let samples = data
            .channels
            .get(channel)
            .map(|channel| Arc::clone(&channel.samples))
            .unwrap_or_else(|| Arc::from([]));
        let end = offset.saturating_add(max_samples).min(samples.len());
        Ok(BackendAudioDataChunk {
            content_revision: self
                .mirror
                .loops
                .get(&loop_id)
                .map_or(0, |entry| entry.content),
            channel,
            channel_count: data.channels.len(),
            offset,
            total_samples: samples.len(),
            samples: if offset < end {
                samples[offset..end].to_vec()
            } else {
                Vec::new()
            },
            ..Default::default()
        })
    }

    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>> {
        self.ready()?;
        let entry = self
            .mirror
            .live_loop(loop_id)
            .ok_or_else(|| anyhow!("unknown loop {loop_id:?}"))?;
        let midi = self.mirror.tracks[&entry.track].midi;
        if !midi || entry.view.length == 0 && entry.view.mode != LoopMode::Recording {
            return Ok(Some(empty_midi(entry.content, midi)));
        }
        if !self.capabilities.contains(Capabilities::BULK_MIDI) {
            return Err(anyhow!("this engine cannot read loop MIDI"));
        }
        if entry.view.mode == LoopMode::Recording {
            return Ok(None);
        }
        let (version, slot) = (entry.content, entry.slot);
        let outcome = self.reads.midi(loop_id, version, slot);
        self.pump();
        match outcome {
            Outcome::Done(data) => Ok(Some(data)),
            Outcome::Reading => Ok(None),
            Outcome::Failed(message) => {
                tracing::warn!(message = %message, "backend.rt.read_failed");
                Err(anyhow!("reading loop MIDI failed: {message}"))
            }
        }
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        self.ready()?;
        let slot = self.live_loop_slot(loop_id)?;
        let source_slot = source
            .map(|source| self.live_loop_slot(source))
            .transpose()?;
        self.requests.push(
            Request::LoopSync {
                loop_id: slot,
                source: source_slot,
            },
            Purpose::LoopSync(loop_id, source),
        );
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        let mode = protocol_mode(mode)?;
        self.ready()?;
        let slot = self.live_loop_slot(loop_id)?;
        self.requests.push(
            Request::LoopTransition {
                loop_id: slot,
                mode,
                delay: cycles_delay,
            },
            Purpose::Transition(loop_id),
        );
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.ready()?;
        let slot = self.live_loop_slot(loop_id)?;
        self.requests.push(
            Request::LoopClear { loop_id: slot },
            Purpose::Clear(loop_id),
        );
        Ok(())
    }

    fn replace_loop_content(
        &mut self,
        _loop_id: BackendLoopId,
        _update: &BackendLoopContentUpdate,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::LoopContentImport))
    }

    fn replace_loop_content_async(
        &mut self,
        _loop_id: BackendLoopId,
        _update: &BackendLoopContentUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        Err(unavailable(BackendCapability::LoopContentImport))
    }

    fn set_loop_length(&mut self, _loop_id: BackendLoopId, _length: u32) -> Result<()> {
        Err(unavailable(BackendCapability::LoopTimingEdits))
    }

    fn set_loop_timing(
        &mut self,
        _loop_id: BackendLoopId,
        _start_offset: Option<i32>,
        _preplay: Option<u32>,
        _length: Option<u32>,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::LoopTimingEdits))
    }

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        Err(unavailable(BackendCapability::SessionCapture))
    }

    fn capture_session_async(&mut self) -> Result<BackendAsyncResult<BackendSessionData>> {
        Err(unavailable(BackendCapability::SessionCapture))
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let mapping = self.start_replacement(session)?;
        Ok(mapping)
    }

    fn replace_session_async(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendAsyncResult<BackendSessionReplacement>> {
        if self.replacement.is_none() {
            let first = self.requests.last_sequence() + 1;
            let mapping = self.start_replacement(session)?;
            self.replacement = Some(crate::Replacement {
                mapping,
                first,
                last: self.requests.last_sequence(),
                refused: Vec::new(),
            });
        }
        self.pump();
        let replacement = self.replacement.as_ref().expect("a replacement");
        if !self.requests.answered_through(replacement.last) {
            let total = (replacement.last + 1).saturating_sub(replacement.first) as usize;
            let remaining = self
                .requests
                .outgoing
                .iter()
                .chain(self.requests.in_flight.values())
                .filter(|pending| {
                    (replacement.first..=replacement.last).contains(&pending.sequence)
                })
                .count();
            return Ok(BackendAsyncResult::Pending(BackendOperationProgress {
                key: replacement.first,
                kind: BackendOperationKind::SessionReplacement,
                completed: total - remaining,
                total: Some(total),
            }));
        }
        let replacement = self.replacement.take().expect("a replacement");
        if !replacement.refused.is_empty() {
            return Err(anyhow!(
                "the engine refused part of the new session: {}",
                replacement.refused.join("; ")
            ));
        }
        Ok(BackendAsyncResult::Ready(replacement.mapping))
    }

    fn set_loop_smoothing_ms(&mut self, milliseconds: u32) -> Result<()> {
        self.ready()?;
        let sample_rate = self.driver.map_or(0, |driver| driver.sample_rate);
        let frames = u64::from(milliseconds) * u64::from(sample_rate) / 1000;
        let limit = self.limits().smoothing_limit_frames;
        if frames > u64::from(limit) {
            return Err(anyhow!(
                "loop boundary smoothing is limited to {} ms on this engine",
                u64::from(limit) * 1000 / u64::from(sample_rate.max(1))
            ));
        }
        self.requests.push(
            Request::SetSmoothing {
                frames: u32::try_from(frames).unwrap_or(u32::MAX),
            },
            Purpose::Smoothing(milliseconds),
        );
        Ok(())
    }

    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        self.queue_connection(port_id, external_port, connected)?;
        Ok(())
    }

    fn set_mixer_route(
        &mut self,
        source_port_id: BackendPortId,
        destination_channel_id: BackendBusChannelId,
        connected: bool,
    ) -> Result<()> {
        self.queue_route(source_port_id, destination_channel_id, connected)?;
        Ok(())
    }

    fn create_bus(&mut self, _request: BackendBusRequest) -> Result<BackendBusCreation> {
        Err(unavailable(BackendCapability::BusManagement))
    }

    fn remove_bus(&mut self, _bus_id: BackendBusId) -> Result<()> {
        Err(unavailable(BackendCapability::BusManagement))
    }

    fn set_bus_control(&mut self, bus_id: BackendBusId, control: BackendBusControl) -> Result<()> {
        self.ready()?;
        if bus_id != MASTER_BUS_ID {
            return Err(anyhow!("unknown bus {bus_id:?}"));
        }
        self.queue_master_control(control)?;
        Ok(())
    }

    fn set_bus_fx_control(
        &mut self,
        _bus_id: BackendBusId,
        _control: BackendBusFxControl,
    ) -> Result<()> {
        Err(unavailable(BackendCapability::BusFx))
    }

    fn advance(&mut self, elapsed: Duration) {
        self.pump();
        self.clock.advance(elapsed);
        self.pump();
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        self.pump();
        let driver = self.driver;
        self.status.peaks_fresh = false;
        let status = BackendStatus {
            dsp_load_percent: self.status.load * 100.0,
            xruns: self.status.take_xruns(),
            buffer_size: driver.map_or(0, |driver| driver.buffer_frames),
            sample_rate: driver.map_or(0, |driver| driver.sample_rate),
            driver_state: self.driver_state(),
            callback_count: self.status.callbacks,
            processed_frames: self.status.frame,
            input_peak: self.status.input_peak,
            output_peak: self.status.output_peak,
            ..Default::default()
        };
        let mirror = &mut self.mirror;
        let mut tracks = std::collections::BTreeMap::new();
        for (id, track) in mirror.tracks.iter_mut() {
            if track.confirmed {
                tracks.insert(*id, Mirror::track_state(track));
            }
        }
        let mut loops = std::collections::BTreeMap::new();
        for (id, entry) in mirror.loops.iter_mut() {
            let Some(track) = mirror.tracks.get(&entry.track) else {
                continue;
            };
            if entry.confirmed && track.confirmed {
                loops.insert(*id, Mirror::loop_state(entry, track.audio_channels));
            }
        }
        let connections = BackendConnectionSnapshot {
            revision: mirror.connection_revision,
            available: self.link == Link::Ready,
            application_ports: mirror.application_ports(),
            host_ports: mirror.host_descriptors(),
            confirmed_links: mirror.links.clone(),
            failures: std::mem::take(&mut self.connection_failures),
        };
        let master = mirror.master_state();
        let mixer = BackendMixerSnapshot {
            revision: mirror.mixer_revision,
            buses: [(MASTER_BUS_ID, master)].into_iter().collect(),
            confirmed_links: mirror.routes.clone(),
            failures: std::mem::take(&mut self.mixer_failures),
        };
        Ok(BackendSnapshot {
            status,
            audio_drivers: self.driver_runtime_state(),
            tracks,
            loops,
            composites: Default::default(),
            removed_composites: Vec::new(),
            connections,
            mixer,
            mutation_failures: std::mem::take(&mut self.mutation_failures),
        })
    }

    fn wait_idle(&mut self) {
        self.wait_until_idle();
    }
}
