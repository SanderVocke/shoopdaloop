//! The `Backend` trait in terms of the mirror and queued requests. Everything outside the
//! core set is refused with the capability's explanation.

use alloc::borrow::ToOwned;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::time::Duration;

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
    BackendPortId, BackendProcessorLatencyAdjustment, BackendRecordingOffsetAdjustment,
    BackendSessionData, BackendSessionReplacement, BackendSnapshot, BackendStatus,
    BackendTrackControl, BackendTrackCreation, BackendTrackFxControl, BackendTrackId,
    BackendTrackTopology, DirectTrackRequest, TrackRequest,
};
use shoop_rt_protocol::{Capabilities, DriverKind, DriverRunState, LoopMode, MidiMessage, Request};
use shoop_rt_transport::carrier::Carrier;

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
            DriverKind::Jack => (
                AudioDriverKind::Jack,
                AudioDriverConfig::Jack(Default::default()),
            ),
        };
        let mut active = ResolvedAudioDriverConfig {
            configured,
            sample_rate: driver.sample_rate,
            buffer_size: driver.buffer_frames,
            instance_name: "New engine".to_owned(),
        };
        if let Some(profile) = &self.driver_profile {
            if profile.configured.kind() == kind {
                active.configured = profile.configured.clone();
                active.instance_name = profile.instance_name.clone();
            }
        }
        AudioDriverRuntimeState {
            supported: false,
            catalog: Arc::from([AudioDriverDescriptor {
                kind,
                available: true,
                ..Default::default()
            }]),
            active: Some(active),
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
        if self.canonical_imports.contains(&loop_id) {
            let (version, slot) = (entry.content, entry.slot);
            if let Outcome::Done(data) = self.reads.audio(loop_id, version, slot, channels) {
                return Ok(Some(data));
            }
            let content = &self.canonical_content[&loop_id];
            if content
                .audio
                .iter()
                .map(|channel| channel.samples.len() as u64 * 4)
                .sum::<u64>()
                > 32 * 1024 * 1024
            {
                return Err(anyhow!("audio view exceeds 32 MiB; use media export"));
            }
            let data = BackendAudioData {
                channels: content
                    .audio
                    .iter()
                    .map(|channel| shoop_backend::BackendAudioChannelData {
                        samples: Arc::from(channel.samples.clone()),
                        start_offset: channel.start_offset,
                        capture_alignment_frames: channel.capture_alignment_frames,
                        preplay: channel.preplay,
                    })
                    .collect(),
            };
            self.reads
                .remember_audio(loop_id, version, slot, data.clone());
            return Ok(Some(data));
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
    fn mutation_cursor(&self) -> u64 {
        self.requests.last_sequence()
    }
    fn mutations_pending_through(&self, sequence: u64) -> bool {
        !self.requests.answered_through(sequence)
    }
    fn mutation_capacity(&self) -> usize {
        crate::MAX_TRACKED_MUTATIONS
            .saturating_sub(self.requests.outstanding() + self.mutation_completions.len())
    }

    fn close_connection(&mut self) {
        self.client.close();
        self.lose("the service closed its engine connection".to_owned());
    }

    fn cancel_media_reads(&mut self) {
        self.reads.clear();
        self.capture_validation = None;
        self.prepared_content
            .retain(|id, _| self.canonical_imports.contains(id));
        self.read_cancel_needed = true;
        self.canonical_content
            .retain(|id, _| self.canonical_imports.contains(id));
    }

    fn bus_limits(&self) -> shoop_app_api::BusLimits {
        let Some(limits) = self.limits else {
            return shoop_app_api::BusLimits {
                buses: 0,
                channels: 0,
                total_channels: 0,
            };
        };
        shoop_app_api::BusLimits {
            buses: u32::from(limits.buses),
            channels: u32::from(limits.bus_channels),
            total_channels: u32::from(limits.buses) * u32::from(limits.bus_channels),
        }
    }
    fn supports_audio_preview(&self) -> bool {
        self.is_ready()
    }
    fn start_bus_audio_preview_async(
        &mut self,
        bus: BackendBusId,
        request_id: u64,
        rate: u32,
        samples: Arc<Vec<f32>>,
    ) -> Result<BackendAsyncResult<()>> {
        self.bus_preview(bus, request_id, rate, samples)
    }
    fn audio_preview_playing(&self) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|p| !p.stopped && p.job.id == self.preview_playing)
    }
    fn stop_audio_preview(&mut self) -> Result<()> {
        self.stop_preview();
        Ok(())
    }
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::NONE
            .with(BackendCapability::BusManagement, self.is_ready())
            .with(
                BackendCapability::MidiInjection,
                self.capabilities.contains(Capabilities::MIDI_INJECTION),
            )
            .with(
                BackendCapability::SessionCapture,
                self.capabilities.contains(Capabilities::CONTENT_UPLOAD),
            )
            .with(
                BackendCapability::SessionContentLoad,
                self.capabilities
                    .contains(Capabilities::COMPLETE_REPLACEMENT),
            )
            .with(
                BackendCapability::LoopContentImport,
                self.capabilities.contains(Capabilities::CONTENT_UPLOAD),
            )
    }

    fn track_processor_catalog(
        &mut self,
    ) -> Result<Arc<[shoop_app_api::TrackProcessorDescriptor]>> {
        self.ready()?;
        Ok(self.processor_descriptors())
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
            topology => {
                if !self.capabilities.contains(Capabilities::DRY_WET) {
                    return Err(unavailable(BackendCapability::DryWetTracks));
                }
                self.create_track_topology(&request.port_name_base, topology, request.initial_loops)
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
        let track = self
            .mirror
            .live_track(track_id)
            .ok_or_else(|| anyhow!("unknown track {track_id:?}"))?;
        if mode == BackendDefaultPlaybackMode::DryThroughWet
            && matches!(track.topology, BackendTrackTopology::Direct { .. })
        {
            return Err(unavailable(BackendCapability::DryWetModes));
        }
        self.requests.push(
            Request::StripDefaultPlayback {
                strip: track.strip,
                dry_through_wet: mode == BackendDefaultPlaybackMode::DryThroughWet,
            },
            Purpose::DefaultPlayback(track_id, mode),
        );
        Ok(())
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
                (event.frame == 0 && event.data.len() <= 4)
                    .then(|| MidiMessage::new(&event.data))
                    .flatten()
                    .ok_or_else(|| anyhow!("invalid MIDI input injection event"))
            })
            .collect::<Result<Vec<_>>>()?;
        self.require_mutation_capacity(messages.len())?;
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
        track_id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        self.processor_control(track_id, control)
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
                .map(|channel| channel.samples.to_vec().into())
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
            .unwrap_or_else(|| Arc::new(Vec::new()));
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
            start_offset: data
                .channels
                .get(channel)
                .map_or(0, |channel| channel.start_offset),
            capture_alignment_frames: data
                .channels
                .get(channel)
                .map_or(0, |channel| channel.capture_alignment_frames),
            preplay: data
                .channels
                .get(channel)
                .map_or(0, |channel| channel.preplay),
        })
    }

    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>> {
        self.ready()?;
        let entry = self
            .mirror
            .live_loop(loop_id)
            .ok_or_else(|| anyhow!("unknown loop {loop_id:?}"))?;
        let midi = self.mirror.tracks[&entry.track].midi;
        if entry.view.mode != LoopMode::Recording && self.canonical_imports.contains(&loop_id) {
            let (version, slot) = (entry.content, entry.slot);
            if let Outcome::Done(data) = self.reads.midi(loop_id, version, slot) {
                return Ok(Some(data));
            }
            let content = &self.canonical_content[&loop_id];
            let bytes = self
                .prepared_content
                .get(&loop_id)
                .filter(|p| p.matches(content))
                .ok_or_else(|| anyhow!("imported content lacks a validation proof"))?
                .events_bytes();
            if bytes > 32 * 1024 * 1024 {
                return Err(anyhow!("MIDI view exceeds 32 MiB; use media export"));
            }
            let data = BackendMidiData {
                channels: content
                    .midi
                    .iter()
                    .map(|channel| shoop_backend::BackendMidiChannelData {
                        content_revision: version,
                        mode: channel.mode,
                        length: channel.length,
                        events: channel.events.clone(),
                        start_offset: channel.start_offset,
                        capture_alignment_frames: channel.capture_alignment_frames,
                        preplay: channel.preplay,
                    })
                    .collect(),
            };
            self.reads
                .remember_midi(loop_id, version, slot, data.clone());
            return Ok(Some(data));
        }
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
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<()> {
        for _ in 0..crate::MAX_WAITS {
            if let BackendAsyncResult::Ready(()) = self.import_content(loop_id, update)? {
                return Ok(());
            }
            if !self.clock.wait() {
                return Err(anyhow!(
                    "content replacement is pending; use replace_loop_content_async"
                ));
            }
        }
        Err(anyhow!("content replacement did not settle"))
    }

    fn replace_loop_content_async(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        self.import_content(loop_id, update)
    }

    fn replace_loop_family_async(
        &mut self,
        update: &shoop_backend::BackendLoopFamilyUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        self.import_family(update)
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
        for _ in 0..crate::MAX_WAITS {
            if let BackendAsyncResult::Ready(capture) = self.capture_core(None)? {
                return Ok(capture);
            }
            if !self.clock.wait() {
                return Err(anyhow!(
                    "session capture is pending; use capture_session_async"
                ));
            }
        }
        Err(anyhow!("session capture did not settle"))
    }

    fn capture_session_async(&mut self) -> Result<BackendAsyncResult<BackendSessionData>> {
        self.capture_core(None)
    }
    fn capture_loop_content_async(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<BackendAsyncResult<shoop_backend::BackendLoopContent>> {
        match self.capture_core(Some(loop_id))? {
            BackendAsyncResult::Pending(progress) => Ok(BackendAsyncResult::Pending(progress)),
            BackendAsyncResult::Ready(session) => session
                .tracks
                .into_iter()
                .flat_map(|track| track.loops)
                .find(|content| content.source_id == loop_id.raw())
                .map(BackendAsyncResult::Ready)
                .ok_or_else(|| anyhow!("capture omitted target loop")),
        }
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        for _ in 0..crate::MAX_WAITS {
            if let BackendAsyncResult::Ready(mapping) = self.replace_session_async(session)? {
                return Ok(mapping);
            }
            if !self.clock.wait() {
                return Err(anyhow!(
                    "session replacement is pending; use replace_session_async"
                ));
            }
        }
        Err(anyhow!("session replacement did not settle"))
    }

    fn replace_session_async(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendAsyncResult<BackendSessionReplacement>> {
        if self.replacement.is_none() && self.preview.is_some() {
            self.stop_preview();
            return Ok(BackendAsyncResult::Pending(
                shoop_backend::BackendOperationProgress {
                    key: 0,
                    kind: shoop_backend::BackendOperationKind::SessionReplacement,
                    completed: 0,
                    total: None,
                },
            ));
        }
        if self.replacement.is_none() {
            self.start_replacement(session)?;
        }
        self.persistence_progress()
    }

    fn cancel_session_replacement(&mut self) -> Result<()> {
        if let Some(replacement) = &mut self.replacement {
            replacement.job.cancel()?;
        }
        Ok(())
    }
    fn replace_session_with_sync_async(
        &mut self,
        session: &BackendSessionData,
        sync: &alloc::collections::BTreeMap<u64, u64>,
    ) -> Result<BackendAsyncResult<BackendSessionReplacement>> {
        if self.replacement.is_none() && self.preview.is_some() {
            self.stop_preview();
            return Ok(BackendAsyncResult::Pending(
                shoop_backend::BackendOperationProgress {
                    key: 0,
                    kind: shoop_backend::BackendOperationKind::SessionReplacement,
                    completed: 0,
                    total: None,
                },
            ));
        }
        if self.replacement.is_none() {
            self.start_replacement_with_sync(session, sync)?;
        }
        self.persistence_progress()
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

    fn create_bus(&mut self, request: BackendBusRequest) -> Result<BackendBusCreation> {
        self.ready()?;
        let request = request.normalized()?;
        if request.fx.is_some() {
            return Err(unavailable(BackendCapability::BusFx));
        }
        if request.channel_count > u32::from(self.limits().bus_channels) {
            return Err(anyhow!("bus exceeds prepared channel limit"));
        }
        self.require_mutation_capacity(1)?;
        let creation = self
            .mirror
            .create_bus(request.name, request.channel_count as u8)
            .ok_or_else(|| anyhow!("prepared bus capacity exhausted"))?;
        let slot = self.mirror.buses[&creation.bus_id].slot;
        self.requests.push(
            Request::CreateBus {
                bus: slot,
                channels: request.channel_count as u8,
            },
            Purpose::CreateBus(creation.bus_id),
        );
        Ok(creation)
    }
    fn remove_bus(&mut self, id: BackendBusId) -> Result<()> {
        self.ready()?;
        self.require_mutation_capacity(1)?;
        let bus = self
            .mirror
            .buses
            .get_mut(&id)
            .filter(|b| !b.removing)
            .ok_or_else(|| anyhow!("unknown bus"))?;
        bus.removing = true;
        let slot = bus.slot;
        self.requests
            .push(Request::RemoveBus { bus: slot }, Purpose::RemoveBus(id));
        Ok(())
    }
    fn set_bus_control(&mut self, id: BackendBusId, control: BackendBusControl) -> Result<()> {
        self.ready()?;
        self.queue_bus_control(id, control)
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
            quantum_frames: self.limits.map_or(0, |limits| limits.quantum),
            driver_state: self.driver_state(),
            callback_count: self.status.callbacks,
            processed_frames: self.status.frame,
            input_peak: self.status.input_peak,
            output_peak: self.status.output_peak,
            ..Default::default()
        };
        let mirror = &mut self.mirror;
        let mut tracks = alloc::collections::BTreeMap::new();
        for (id, track) in mirror.tracks.iter_mut() {
            if track.confirmed {
                tracks.insert(*id, Mirror::track_state(track));
            }
        }
        let mut loops = alloc::collections::BTreeMap::new();
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
            failures: core::mem::take(&mut self.connection_failures),
        };
        let buses = mirror.bus_states();
        let mixer = BackendMixerSnapshot {
            revision: mirror.mixer_revision,
            buses,
            confirmed_links: mirror.routes.clone(),
            failures: core::mem::take(&mut self.mixer_failures),
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
            mutation_failures: core::mem::take(&mut self.mutation_failures),
            mutation_completions: core::mem::take(&mut self.mutation_completions),
        })
    }

    fn wait_idle(&mut self) {
        self.wait_until_idle();
    }
}
