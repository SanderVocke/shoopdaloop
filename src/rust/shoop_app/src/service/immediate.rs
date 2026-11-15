use super::*;
use shoop_app_protocol::immediate::{self, Command as Immediate};

impl Service {
    pub(super) fn immediate_capabilities(&self) -> immediate::Capabilities {
        let enabled = self.backend.supports_immediate_commands();
        let eligible: Vec<_> = self
            .loops
            .iter()
            .filter_map(|(id, entry)| {
                let track = self.tracks.get(&entry.track)?;
                let state = self.observed.loops.get(&entry.backend)?;
                (enabled
                    && matches!(track.view.topology, TrackTopology::Direct)
                    && track.view.fx.is_none()
                    && track.audio_channels > 0
                    && !track.midi
                    && track.view.default_playback_mode == DefaultPlaybackMode::Regular
                    && track.view.structural_state == StructuralState::Confirmed
                    && entry.view.structural_state == StructuralState::Confirmed
                    && !self.composites.contains_key(id)
                    && matches!(
                        state.mode,
                        BackendLoopMode::Stopped
                            | BackendLoopMode::Playing
                            | BackendLoopMode::Recording
                    )
                    && state.next_mode.is_none()
                    && !state.recording_armed)
                    .then_some((*id, state))
            })
            .collect();
        let playback = eligible
            .iter()
            .filter(|(_, state)| !state.repeat_sync)
            .map(|(id, _)| *id)
            .collect();
        let grab = eligible
            .iter()
            .filter(|(_, state)| {
                self.backend
                    .capabilities()
                    .supports(BackendCapability::Grab)
                    && state.mode != BackendLoopMode::Recording
            })
            .map(|(id, _)| *id)
            .collect();
        immediate::Capabilities {
            enabled,
            playback,
            grab,
            refused: self
                .immediate_refused
                .saturating_add(self.backend.immediate_refusals()),
        }
    }

    pub(super) fn immediate_commands(
        &mut self,
        epoch: u64,
        commands: &[Immediate],
    ) -> Result<(), Refusal> {
        if !self.lease.as_ref().is_some_and(|lease| lease.ready)
            || !self.is_ready()
            || self.shutdown.is_some()
        {
            return Err(Refusal::NotReady);
        }
        if epoch != self.persistence_epoch {
            return Err(Refusal::Identity);
        }
        if !immediate::bounded(commands) {
            return Err(Refusal::Invalid);
        }
        if !self.backend.supports_immediate_commands() {
            return Err(Refusal::Unsupported);
        }
        let playback = commands.iter().any(|command| {
            matches!(
                command,
                Immediate::Start(_)
                    | Immediate::Stop(_)
                    | Immediate::Record(_)
                    | Immediate::Grab { .. }
                    | Immediate::AdoptRingbuffers { .. }
            )
        });
        if self.duplication.is_some()
            || self.persistence_immediate_busy(playback)
            || self.audio_driver_busy(None)
            || (playback && self.composite_structure.is_some())
        {
            return Err(Refusal::Busy);
        }
        let values = commands
            .iter()
            .map(|command| {
                use BackendImmediateCommand as B;
                let loop_id = |id| {
                    self.loops
                        .get(&id)
                        .filter(|entry| entry.view.structural_state == StructuralState::Confirmed)
                        .map(|entry| entry.backend)
                        .unwrap_or(BackendLoopId::from_raw(0))
                };
                let track = |id| {
                    self.tracks
                        .get(&id)
                        .filter(|entry| entry.view.structural_state == StructuralState::Confirmed)
                        .map(|entry| entry.backend)
                        .unwrap_or(BackendTrackId::from_raw(0))
                };
                let bus = |id| {
                    self.buses
                        .get(&id)
                        .filter(|b| b.view.structural_state == StructuralState::Confirmed)
                        .map(|b| b.backend)
                        .unwrap_or(BackendBusId::from_raw(0))
                };
                match *command {
                    Immediate::LoopGain(id, v) => B::LoopGain(loop_id(id), v),
                    Immediate::LoopBalance(id, v) => B::LoopBalance(loop_id(id), v),
                    Immediate::Start(id) => B::Start(loop_id(id)),
                    Immediate::Stop(id) => B::Stop(loop_id(id)),
                    Immediate::Record(id) => B::Record(loop_id(id)),
                    Immediate::AdoptRingbuffers {
                        loop_id: id,
                        reverse_cycle_start,
                        cycles_length,
                        go_to_cycle,
                        mode,
                    } => B::Grab(BackendGrabRequest {
                        loop_id: loop_id(id),
                        reverse_start_cycle: Some(i32::try_from(reverse_cycle_start).unwrap_or(-1)),
                        cycles_length: Some(i32::try_from(cycles_length).unwrap_or(-1)),
                        go_to_cycle: Some(i32::try_from(go_to_cycle).unwrap_or(-1)),
                        go_to_mode: match mode {
                            LoopMode::Playing => BackendLoopMode::Playing,
                            LoopMode::Recording => BackendLoopMode::Recording,
                            LoopMode::Stopped | LoopMode::Unknown => BackendLoopMode::Stopped,
                            _ => BackendLoopMode::Replacing,
                        },
                    }),
                    Immediate::Grab {
                        loop_id: id,
                        sync,
                        target,
                        cycles,
                        play_after_record,
                    } => B::GrabGesture {
                        loop_id: loop_id(id),
                        sync: loop_id(sync),
                        target: target.map(loop_id),
                        cycles,
                        play_after_record,
                    },
                    Immediate::InputGainDb(id, v) => B::InputGainDb(track(id), v),
                    Immediate::InputBalance(id, v) => B::InputBalance(track(id), v),
                    Immediate::OutputGainDb(id, v) => B::OutputGainDb(track(id), v),
                    Immediate::OutputBalance(id, v) => B::OutputBalance(track(id), v),
                    Immediate::OutputMute(id, v) => B::OutputMute(track(id), v),
                    Immediate::BusGainDb(id, v) => B::BusGainDb(bus(id), v),
                    Immediate::BusBalance(id, v) => B::BusBalance(bus(id), v),
                    Immediate::BusMute(id, v) => B::BusMute(bus(id), v),
                }
            })
            .collect::<Vec<_>>();
        match self.backend.submit_immediate_commands(&values) {
            Ok(true) => Ok(()),
            Ok(false) => Err(Refusal::Overloaded),
            Err(_) => Err(Refusal::Invalid),
        }
    }
}
