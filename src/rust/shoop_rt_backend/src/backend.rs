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

pub(crate) fn protocol_mode(mode: BackendLoopMode) -> Result<LoopMode> {
    match mode {
        BackendLoopMode::Stopped => Ok(LoopMode::Stopped),
        BackendLoopMode::Playing => Ok(LoopMode::Playing),
        BackendLoopMode::Recording => Ok(LoopMode::Recording),
        BackendLoopMode::Replacing => Ok(LoopMode::Replacing),
        BackendLoopMode::PlayingDryThroughWet => Ok(LoopMode::PlayingDryThroughWet),
        BackendLoopMode::RecordingDryIntoWet => Ok(LoopMode::RecordingDryIntoWet),
        BackendLoopMode::Unknown => Err(anyhow!("a loop cannot transition to an unknown mode")),
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn update_profiling_groups(&mut self) {
        use rt_profiling::Group;
        let view = self.profiling.view();
        let Some(topology) = view.topology else {
            return;
        };
        if self
            .profiling_groups
            .0
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &topology))
            && self.profiling_groups.2
        {
            return;
        }
        let groups: alloc::collections::BTreeSet<_> = topology
            .nodes
            .iter()
            .filter(|n| n.group != 0)
            .map(|n| (n.group, n.slot, n.owner_generation))
            .collect();
        let expected = groups.len();
        let groups: Vec<_> = groups
            .into_iter()
            .filter_map(|(kind, slot, generation)| {
                let entity = match kind {
                    1 => self.mirror.tracks.iter().find_map(|(id, t)| {
                        (t.strip.0.index == slot
                            && t.strip.0.generation == generation
                            && t.confirmed
                            && !t.removing)
                            .then_some(id.raw())
                    }),
                    2 => self.mirror.loops.iter().find_map(|(id, l)| {
                        (l.slot.0.index == slot && l.slot.0.generation == generation && l.confirmed)
                            .then_some(id.raw())
                    }),
                    3 => self.mirror.buses.iter().find_map(|(id, b)| {
                        (b.slot.0.index == slot
                            && b.slot.0.generation == generation
                            && b.confirmed
                            && !b.removing)
                            .then_some(id.raw())
                    }),
                    _ => None,
                }?;
                Some(Group {
                    kind,
                    slot,
                    entity,
                    name: Default::default(),
                })
            })
            .collect();
        let complete = groups.len() == expected;
        self.profiling_groups = (Some(topology), groups.into(), complete);
    }

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

    pub(crate) fn live_loop_slot(
        &self,
        loop_id: BackendLoopId,
    ) -> Result<shoop_rt_protocol::LoopSlotId> {
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
        if entry.timing.content_revision != entry.rt_content_revision {
            return Ok(None);
        }
        let track = &self.mirror.tracks[&entry
            .track
            .ok_or_else(|| anyhow!("composites have no media channels"))?];
        let wet = track.audio_channels as usize;
        let dry = (!matches!(track.topology, BackendTrackTopology::Direct { .. }))
            .then_some(track.topology.dry_audio_channels() as usize);
        let channels = wet + dry.unwrap_or(0);
        let (version, slot) = (entry.content, entry.slot);
        let read = |reads: &mut crate::reads::Reads| match dry {
            Some(dry) => reads.audio_roles(loop_id, version, slot, dry, wet),
            None => reads.audio(loop_id, version, slot, channels),
        };
        if entry.view.mode.records() || self.input.as_ref().is_some_and(|job| job.touches(loop_id))
        {
            return Ok(None);
        }
        if self.canonical_imports.contains(&loop_id) {
            let (version, slot) = (entry.content, entry.slot);
            if let Outcome::Done(data) = read(&mut self.reads) {
                return Ok(Some(self.audio_timing(loop_id, data)));
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
                .remember_audio(loop_id, version, slot, data.clone(), dry);
            return Ok(Some(self.audio_timing(loop_id, data)));
        }
        if entry.timing.raw_audio_frames == 0 {
            return Ok(Some(self.audio_timing(loop_id, empty_audio(channels))));
        }
        let outcome = read(&mut self.reads);
        self.pump();
        match outcome {
            Outcome::Done(data) => Ok(Some(self.audio_timing(loop_id, data))),
            Outcome::Reading => Ok(None),
            Outcome::Failed(message) => {
                tracing::warn!(message = %message, "backend.rt.read_failed");
                Err(anyhow!("reading loop audio failed: {message}"))
            }
        }
    }
}

impl<C: Carrier, K: EngineClock> Backend for RtBackend<C, K> {
    fn release_host_port_identity(&mut self, id: &str) -> bool {
        if self.replacement.is_some() || self.host_identity_in_use(id) {
            return false;
        }
        let Some((slot, alias)) = self
            .mirror
            .host_aliases
            .iter_mut()
            .find(|(_, alias)| alias.id == id && !alias.available)
        else {
            return false;
        };
        let identity = core::mem::take(&mut alias.id);
        alias.name.clear();
        self.retired_host_ports.insert(*slot, identity);
        true
    }

    fn host_port_in_use(&self, id: &str) -> bool {
        self.host_identity_in_use(id)
    }
    fn mutation_cursor(&self) -> u64 {
        self.requests.last_sequence()
    }
    fn mutations_pending_through(&self, sequence: u64) -> bool {
        !self.requests.answered_through(sequence)
            || self
                .composites
                .jobs
                .iter()
                .any(|job| job.sequence <= sequence)
            || self
                .composites
                .controls
                .keys()
                .any(|pending| *pending <= sequence)
            || self
                .input
                .as_ref()
                .is_some_and(|job| job.sequence <= sequence)
    }
    fn mutation_capacity(&self) -> usize {
        crate::MAX_TRACKED_MUTATIONS.saturating_sub(
            self.requests.outstanding()
                + self.mutation_completions.len()
                + self.composites.jobs.len()
                + self.composites.controls.len()
                + self
                    .composites
                    .jobs
                    .iter()
                    .map(|job| job.cancel_waiters.len())
                    .sum::<usize>()
                + usize::from(self.input.is_some()),
        )
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
            .with(
                BackendCapability::LoopDuplication,
                self.is_ready() && self.capabilities.contains(Capabilities::CONTENT_UPLOAD),
            )
            .with(
                BackendCapability::Grab,
                self.capabilities.contains(Capabilities::INPUT_BATCH),
            )
            .with(
                BackendCapability::ReplacingMode,
                self.capabilities.contains(Capabilities::REPLACING),
            )
            .with(
                BackendCapability::CompositeLoops,
                self.is_ready()
                    && self.limits.is_some_and(|limits| {
                        limits.composite_entries > 0 && limits.composite_segments > 0
                    }),
            )
            .with(
                BackendCapability::LoopTimingEdits,
                self.capabilities.contains(Capabilities::LOOP_TIMING),
            )
            .with(BackendCapability::BusManagement, self.is_ready())
            .with(
                BackendCapability::BusFx,
                self.capabilities.contains(Capabilities::BUS_PROCESSORS),
            )
            .with(
                BackendCapability::DryWetTracks,
                self.capabilities.contains(Capabilities::DRY_WET),
            )
            .with(
                BackendCapability::DryWetModes,
                self.capabilities.contains(Capabilities::DRY_WET),
            )
            .with(
                BackendCapability::TrackFx,
                self.capabilities.contains(Capabilities::TRACK_PROCESSORS),
            )
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

    fn supports_composite_loops(&self) -> bool {
        self.capabilities()
            .supports(BackendCapability::CompositeLoops)
    }

    fn composite_plan_mutations_are_synchronous(&self) -> bool {
        false
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
        if !self
            .capabilities()
            .supports(BackendCapability::CompositeLoops)
        {
            return Err(unavailable(BackendCapability::CompositeLoops));
        }
        self.create_prepared_composite()
    }
    fn convert_empty_loop_to_prepared_composite(
        &mut self,
        id: BackendLoopId,
        plan: &shoop_backend::BackendPreparedCompositePlan,
    ) -> Result<(BackendCompositeId, u64)> {
        if self
            .mirror
            .live_loop(id)
            .is_none_or(|entry| entry.program.is_some())
        {
            return Err(anyhow!("target is not an empty primitive"));
        }
        let version = self.queue_composite(id, plan)?;
        Ok((BackendCompositeId::from_raw(id.raw()), version))
    }
    fn clear_composite_loop(&mut self, id: BackendCompositeId) -> Result<BackendLoopId> {
        self.reset_composite(id)
    }
    fn composite_limits(&self) -> Option<shoop_backend::BackendCompositeLimits> {
        self.composite_limits_core()
    }
    fn convert_empty_loop_to_composite(&mut self, id: BackendLoopId) -> Result<BackendCompositeId> {
        self.convert_composite(id)
    }
    fn configure_prepared_composite_loop(
        &mut self,
        id: BackendCompositeId,
        plan: &shoop_backend::BackendPreparedCompositePlan,
    ) -> Result<u64> {
        self.queue_composite(BackendLoopId::from_raw(id.raw()), plan)
    }
    fn cancel_composite_plan(&mut self, id: BackendCompositeId, version: u64) -> Result<()> {
        self.cancel_composite(id, version)
    }
    fn set_loop_repeat_sync(&mut self, id: BackendLoopId, enabled: bool) -> Result<()> {
        self.repeat_sync_core(id, enabled)
    }
    fn set_composite_repeat_sync(&mut self, id: BackendCompositeId, enabled: bool) -> Result<()> {
        self.repeat_sync_core(BackendLoopId::from_raw(id.raw()), enabled)
    }
    fn configure_composite_loop(
        &mut self,
        id: BackendCompositeId,
        config: &BackendCompositeConfig,
    ) -> Result<u64> {
        let plan = self.legacy_composite_plan(id, config)?;
        self.queue_composite(BackendLoopId::from_raw(id.raw()), &plan)
    }
    fn transition_composite_loop(
        &mut self,
        id: BackendCompositeId,
        mode: BackendLoopMode,
        delay: Option<u32>,
        align: Option<i64>,
    ) -> Result<()> {
        self.transition_composite(id, mode, delay, align)
    }
    fn set_composite_play_after_record(
        &mut self,
        id: BackendCompositeId,
        _enabled: bool,
    ) -> Result<()> {
        self.ready()?;
        let loop_id = BackendLoopId::from_raw(id.raw());
        if self
            .mirror
            .live_loop(loop_id)
            .is_none_or(|entry| entry.program.is_none() && !self.pending_conversion(loop_id))
        {
            return Err(anyhow!("unknown composite"));
        }
        // Play/Stop outer transport leaves recording follow-ups to authored actions.
        Ok(())
    }
    fn remove_composite_loop(&mut self, id: BackendCompositeId) -> Result<Option<u64>> {
        self.remove_composite(id)
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

    fn set_track_automatic_monitoring(
        &mut self,
        track_id: BackendTrackId,
        enabled: bool,
    ) -> Result<()> {
        self.ready()?;
        let strip = self
            .mirror
            .live_track(track_id)
            .ok_or_else(|| anyhow!("unknown track"))?
            .strip;
        self.requests.push(
            Request::StripControl {
                strip,
                control: shoop_rt_protocol::StripControl::AutomaticMonitoring(enabled),
            },
            Purpose::AutomaticMonitoring(track_id, enabled),
        );
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
        if mode == BackendDefaultPlaybackMode::DryThroughWet && (!track.topology.has_wet_channels())
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

    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        if !self.capabilities.contains(Capabilities::INPUT_BATCH) {
            return Err(unavailable(BackendCapability::Grab));
        }
        self.grab_input(requests)
    }

    fn input_edits(
        &mut self,
        edits: &[shoop_backend::BackendInputEdit],
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        RtBackend::input_edits(self, edits, cycles_delay)
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
        if entry.timing.content_revision != entry.rt_content_revision {
            return Ok(None);
        }
        let midi = self.mirror.tracks[&entry
            .track
            .ok_or_else(|| anyhow!("composites have no media channels"))?]
            .midi;
        if entry.view.mode.records() || self.input.as_ref().is_some_and(|job| job.touches(loop_id))
        {
            return Ok(None);
        }
        if self.canonical_imports.contains(&loop_id) {
            let (version, slot) = (entry.content, entry.slot);
            if let Outcome::Done(data) = self.reads.midi(loop_id, version, slot) {
                return Ok(Some(self.midi_timing(loop_id, data)));
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
            return Ok(Some(self.midi_timing(loop_id, data)));
        }
        if !midi || entry.timing.raw_midi_frames == 0 && entry.view.mode != LoopMode::Recording {
            return Ok(Some(
                self.midi_timing(loop_id, empty_midi(entry.content, midi)),
            ));
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
            Outcome::Done(data) => Ok(Some(self.midi_timing(loop_id, data))),
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
        if self.pending_kind_change(loop_id)
            || source.is_some_and(|id| self.pending_kind_change(id))
        {
            self.defer_composite_control(
                loop_id,
                crate::composites::control::Action::Sync(source),
                false,
            );
            return Ok(());
        }
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
        let dry = matches!(
            mode,
            BackendLoopMode::PlayingDryThroughWet | BackendLoopMode::RecordingDryIntoWet
        );
        if dry && !self.capabilities.contains(Capabilities::DRY_WET) {
            return Err(unavailable(BackendCapability::DryWetModes));
        }
        let mode = protocol_mode(mode)?;
        self.ready()?;
        let slot = self.live_loop_slot(loop_id)?;
        if dry
            && matches!(
                self.mirror.tracks[&self.mirror.loops[&loop_id]
                    .track
                    .ok_or_else(|| anyhow!("composites have no dry/wet topology"))?]
                    .topology,
                BackendTrackTopology::Direct { .. }
            )
        {
            return Err(unavailable(BackendCapability::DryWetModes));
        }
        if mode == LoopMode::Replacing {
            if !self.capabilities.contains(Capabilities::REPLACING) {
                return Err(unavailable(BackendCapability::ReplacingMode));
            }
            return self.begin_input(
                alloc::vec![(
                    loop_id,
                    shoop_rt_protocol::InputEdit {
                        loop_id: slot,
                        intent: shoop_rt_protocol::InputIntent::Mode {
                            mode,
                            position: None
                        },
                    }
                )],
                cycles_delay,
            );
        }
        if self.pending_clear(loop_id)
            || mode != shoop_rt_protocol::LoopMode::Recording && self.pending_kind_change(loop_id)
        {
            self.defer_composite_control(
                loop_id,
                crate::composites::control::Action::Transport {
                    mode,
                    delay: cycles_delay,
                    iteration: None,
                },
                mode == shoop_rt_protocol::LoopMode::Playing
                    && self.pending_conversion(loop_id)
                    && !self.pending_clear(loop_id),
            );
            return Ok(());
        }
        self.cancel_input_for(loop_id);
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
        self.cancel_input_for(loop_id);
        let slot = self.live_loop_slot(loop_id)?;
        if self.pending_kind_change(loop_id)
            || self
                .mirror
                .live_loop(loop_id)
                .is_some_and(|entry| entry.program.is_some())
        {
            self.queue_clear(loop_id);
        } else {
            self.requests.push(
                Request::LoopClear { loop_id: slot },
                Purpose::Clear(loop_id),
            );
        }
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

    fn load_loop_content_async(
        &mut self,
        loop_id: BackendLoopId,
        content: &shoop_backend::BackendLoopContent,
    ) -> Result<BackendAsyncResult<()>> {
        self.import_payload(loop_id, crate::import::Payload::Captured(content))
    }

    fn replace_loop_family_async(
        &mut self,
        update: &shoop_backend::BackendLoopFamilyUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        self.import_family(update)
    }

    fn set_loop_length(&mut self, loop_id: BackendLoopId, length: u32) -> Result<()> {
        self.set_loop_timing(loop_id, None, None, Some(length))
    }
    fn set_loop_timing(
        &mut self,
        loop_id: BackendLoopId,
        start_offset: Option<i32>,
        preplay: Option<u32>,
        length: Option<u32>,
    ) -> Result<()> {
        self.send_timing(
            loop_id,
            shoop_rt_protocol::TimingEdit {
                start_offset,
                preplay,
                length: length.map(u64::from),
                ..Default::default()
            },
        )
    }
    fn set_loop_channel_timings(
        &mut self,
        loop_id: BackendLoopId,
        updates: &[shoop_backend::BackendChannelTimingUpdate],
    ) -> Result<()> {
        if updates.len() > shoop_rt_protocol::MAX_TIMING_UPDATES {
            return Err(anyhow!("too many channel timing updates"));
        }
        let mut edit = shoop_rt_protocol::TimingEdit {
            count: updates.len() as u8,
            ..Default::default()
        };
        for (target, source) in edit.channels.iter_mut().zip(updates) {
            *target = shoop_rt_protocol::ChannelTimingUpdate {
                channel: u8::try_from(source.channel)?,
                midi: source.is_midi,
                start_offset: source.start_offset,
                preplay: source.preplay,
            };
        }
        self.send_timing(loop_id, edit)
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

    fn inspect_profiling(
        &mut self,
        request: shoop_app_api::profiling_data::CaptureRequest,
    ) -> Result<()> {
        self.ready()?;
        self.profiling.select(request);
        Ok(())
    }

    fn set_profiling(&mut self, enabled: bool) -> Result<()> {
        self.ready()?;
        if !self
            .capabilities
            .contains(shoop_rt_protocol::Capabilities::PROFILING)
        {
            if enabled {
                return Err(anyhow!("Runtime profiling is unavailable on this engine"));
            }
            return Ok(());
        }
        self.requests.push(
            Request::Diagnostic(rt_profiling::wire::subscribe(enabled)),
            Purpose::Profiling,
        );
        Ok(())
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
        if let Some(fx) = &request.fx {
            if !self.capabilities.contains(Capabilities::BUS_PROCESSORS)
                || fx.processor_type != shoop_app_api::TrackProcessorTypeId::BUILTIN_FX
            {
                return Err(unavailable(BackendCapability::BusFx));
            }
            fx.clone().normalized(request.channel_count as usize)?;
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
            if request.fx.is_some() {
                Request::CreateProcessedBus {
                    bus: slot,
                    channels: request.channel_count as u8,
                }
            } else {
                Request::CreateBus {
                    bus: slot,
                    channels: request.channel_count as u8,
                }
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

    fn bus_processor_catalog(&mut self) -> Result<Arc<[shoop_app_api::TrackProcessorDescriptor]>> {
        if !self.capabilities.contains(Capabilities::BUS_PROCESSORS) {
            return Ok(Arc::from([]));
        }
        let mut entries: Vec<_> = self
            .processor_descriptors()
            .iter()
            .filter(|entry| entry.id.as_str() == shoop_app_api::TrackProcessorTypeId::BUILTIN_FX)
            .cloned()
            .collect();
        for entry in &mut entries {
            entry.constraints.midi = shoop_app_api::TrackProcessorMidiPolicy::Unsupported;
        }
        Ok(entries.into())
    }
    fn set_bus_fx_control(
        &mut self,
        bus_id: BackendBusId,
        control: BackendBusFxControl,
    ) -> Result<()> {
        self.bus_processor_control(bus_id, control)
    }
    fn bus_fx_state_string(
        &mut self,
        bus_id: BackendBusId,
    ) -> Result<Option<alloc::string::String>> {
        let bus = self
            .mirror
            .buses
            .get(&bus_id)
            .ok_or_else(|| anyhow!("unknown bus"))?;
        bus.processor_snapshot
            .map(|snapshot| {
                crate::processor_controls::snapshot_state(
                    shoop_processor_protocol::Kind::BuiltInFx,
                    snapshot.values,
                )
                .map(|state| state.encode())
                .map_err(anyhow::Error::from)
            })
            .transpose()
    }

    fn advance(&mut self, elapsed: Duration) {
        self.pump();
        self.clock.advance(elapsed);
        self.pump();
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        self.refresh_host_ports()?;
        self.pump();
        self.reconcile_host_ports()?;
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
        let composites = self.composite_states();
        if !self.profiling_groups.2 {
            self.update_profiling_groups();
        }
        let processor_latency = self
            .processors
            .map_or(0, |limits| limits.builtin_latency_frames);
        let mirror = &mut self.mirror;
        let mut tracks = alloc::collections::BTreeMap::new();
        for (id, track) in mirror.tracks.iter_mut() {
            if track.confirmed {
                let mut state = Mirror::track_state(track);
                if crate::processor_controls::track_kind(track).is_some() {
                    state.latency.automatic_processor_advance_frames = Some(processor_latency);
                    state.latency.processor_adjustment =
                        BackendProcessorLatencyAdjustment::Automatic;
                    state.latency.effective_processor_advance_frames = Some(processor_latency);
                }
                tracks.insert(*id, state);
            }
        }
        let mut loops = alloc::collections::BTreeMap::new();
        for (id, entry) in mirror.loops.iter_mut() {
            let Some(track) = entry.track.and_then(|track| mirror.tracks.get(&track)) else {
                continue;
            };
            if entry.program.is_none() && entry.confirmed && track.confirmed {
                let state = Mirror::loop_state(
                    entry,
                    track.audio_channels,
                    track.topology.dry_audio_channels(),
                    track.midi,
                    !matches!(track.topology, BackendTrackTopology::Direct { .. }),
                );
                loops.insert(*id, state);
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
        let mut profiling = self.profiling.view();
        if profiling
            .topology
            .as_ref()
            .zip(self.profiling_groups.0.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
        {
            profiling.groups = self.profiling_groups.1.clone();
        }
        Ok(BackendSnapshot {
            composite_operations: self.composite_operations(),
            composite_receipts: core::mem::take(&mut self.composites.receipts),
            profiling,
            status,
            audio_drivers: self.driver_runtime_state(),
            tracks,
            loops,
            composites,
            removed_composites: core::mem::take(&mut self.composites.removed),
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

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_rt_engine::{testing::OfflineRender, EngineDescription, HostPorts, Segment};
    use shoop_wasm_test_support::shoop_test;

    struct Clock(OfflineRender);
    impl EngineClock for Clock {
        fn advance(&mut self, _: Duration) {}
        fn wait(&mut self) -> bool {
            self.0.run(16, |_, _| 0.).unwrap();
            true
        }
    }

    #[shoop_test]
    fn topology_owners_complete_after_a_delayed_creation_acknowledgement() {
        let description = EngineDescription {
            quantum: 1,
            sample_rate: 1000,
            max_frames: 16,
            host: HostPorts::numbered(2, 2, 0),
            segments: alloc::vec![Segment {
                strips: 1,
                loops: 1
            }],
            strip_channels: 2,
            master_channels: 2,
            resource_pages: 256,
            page_bytes: 256,
            ..Default::default()
        };
        let (render, client) = OfflineRender::new(&description).unwrap();
        let mut backend = RtBackend::connect(client, Clock(render)).unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "Delayed".into(),
                audio_channels: 2,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        backend.set_profiling(true).unwrap();
        backend.wait_idle();
        for _ in 0..8 {
            backend.pump();
        }
        let id = created.loops[0];
        backend.profiling_groups = Default::default();
        backend.mirror.loops.get_mut(&id).unwrap().confirmed = false;
        backend.update_profiling_groups();
        assert!(!backend.profiling_groups.2);
        assert!(!backend
            .profiling_groups
            .1
            .iter()
            .any(|g| g.kind == 2 && g.entity == id.raw()));
        backend.mirror.loops.get_mut(&id).unwrap().confirmed = true;
        let view = backend.poll().unwrap().profiling;
        assert!(backend.profiling_groups.2);
        assert!(view
            .groups
            .iter()
            .any(|g| g.kind == 2 && g.entity == id.raw()));
        backend.profiling_groups = Default::default();
        backend.mirror.loops.get_mut(&id).unwrap().slot.0.generation += 1;
        let view = backend.poll().unwrap().profiling;
        assert!(!view
            .groups
            .iter()
            .any(|g| g.kind == 2 && g.entity == id.raw()));
    }
}
