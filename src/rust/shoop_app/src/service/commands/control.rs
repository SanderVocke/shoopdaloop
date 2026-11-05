use alloc::vec::Vec;

use super::*;

impl Service {
    pub(super) fn control_operation(&mut self, operation: ControlOperation) -> CommandResult {
        use ControlOperation::*;
        match operation {
            Transition {
                loops,
                mode,
                cycles_delay,
                align_to_sync_at,
            } => {
                let mode = control_mode(mode)?;
                for id in &loops {
                    self.loop_entry(*id)?;
                    self.require_restored_composite(*id, mode)?;
                    if self.composites.contains_key(id) {
                        if mode != BackendLoopMode::Stopped && mode != BackendLoopMode::Playing {
                            return Err(CommandError::invalid(
                                "a composite's outer mode is Play or Stop",
                            ));
                        }
                        self.composite_transport_iteration(*id, align_to_sync_at)?;
                        if align_to_sync_at.is_some() && mode != BackendLoopMode::Playing {
                            return Err(CommandError::invalid(
                                "composite iteration alignment requires Play",
                            ));
                        }
                    } else if align_to_sync_at.is_some() {
                        return Err(CommandError::invalid(
                            "aligned primitive transitions are unavailable",
                        ));
                    }
                }
                for id in loops {
                    if self.composites.contains_key(&id) {
                        self.transition_authored_composite(
                            id,
                            mode,
                            cycles_delay,
                            align_to_sync_at,
                        )?;
                        continue;
                    }
                    self.backend.transition_loop_aligned(
                        self.loops[&id].backend,
                        mode,
                        cycles_delay,
                        align_to_sync_at,
                    )?;
                }
            }
            Trigger { loops, mode } => self.transition(
                loops,
                control_mode(mode)?,
                self.global.sync.then(|| self.target_delay()),
                Some(self.global.apply_n_cycles),
            )?,
            TriggerDefaultPlayback { loops } => self.transition_default(
                loops,
                self.global.sync.then(|| self.target_delay()),
                Some(self.global.apply_n_cycles),
            )?,
            RecordN {
                loops,
                n_cycles,
                cycles_delay,
            } => self.transition(
                loops,
                BackendLoopMode::Recording,
                Some(cycles_delay),
                Some(n_cycles),
            )?,
            RecordWithTargeted { loops } => {
                return self.execute(Command::RecordWithTarget { loops })
            }
            SetLoopGain { loops, gain } => {
                for id in &loops {
                    self.loop_entry(*id)?;
                }
                for id in loops {
                    self.backend.set_loop_gain(self.loops[&id].backend, gain)?;
                }
            }
            SetLoopBalance { loops, balance } => {
                for id in &loops {
                    self.loop_entry(*id)?;
                }
                for id in loops {
                    self.backend
                        .set_loop_balance(self.loops[&id].backend, balance)?;
                }
            }
            SetLoopSelection {
                loops,
                selected,
                clear_others,
            } => {
                for id in &loops {
                    self.loop_entry(*id)?;
                }
                if clear_others {
                    for entry in self.loops.values_mut() {
                        entry.view.selected = false;
                    }
                }
                for id in loops {
                    let view = &mut self.loops.get_mut(&id).unwrap().view;
                    view.selected = selected;
                    if selected {
                        view.targeted = false;
                    }
                }
            }
            SetTarget { target } => return self.execute(Command::SetTarget(target)),
            ClearLoops { loops } => {
                return self.clear_composite_slots(loops);
            }
            SetRepeatSync { loops, active } => {
                for id in &loops {
                    self.loop_entry(*id)?;
                }
                for id in loops {
                    self.backend
                        .set_loop_repeat_sync(self.loops[&id].backend, active)?;
                }
            }
            Grab { .. } | AdoptRingbuffers { .. } => {
                return Err(CommandError::unsupported(BackendCapability::Grab))
            }
            ComposeAddToEnd {
                target,
                add,
                parallel,
            } => {
                return self.edit_composite(shoop_app_protocol::composite::Edit::Append {
                    target,
                    add,
                    parallel,
                    convert_if_empty: true,
                })
            }
            SetTrackGain { tracks, gain_db } => {
                self.control_tracks(tracks, TrackAction::OutputGainChanged(gain_db))?
            }
            SetTrackBalance { tracks, balance } => {
                self.control_tracks(tracks, TrackAction::OutputBalanceChanged(balance))?
            }
            SetTrackMuted { tracks, muted } => {
                self.control_tracks(tracks, TrackAction::OutputMuteChanged(muted))?
            }
            SetTrackInputGain { tracks, gain_db } => {
                self.control_tracks(tracks, TrackAction::InputGainChanged(gain_db))?
            }
            SetTrackInputBalance { tracks, balance } => {
                self.control_tracks(tracks, TrackAction::InputBalanceChanged(balance))?
            }
            SetTrackInputMuted {
                tracks,
                muted,
                respect_auto_mute,
            } => {
                for id in &tracks {
                    self.track(*id)?;
                }
                if !muted && respect_auto_mute && self.global.auto_mute_other_track_inputs {
                    let others: Vec<_> = self
                        .track_order
                        .iter()
                        .filter(|id| !tracks.contains(id) && !self.tracks[id].view.is_sync)
                        .copied()
                        .collect();
                    self.control_tracks(
                        others,
                        TrackAction::InputMonitoringChanged {
                            enabled: false,
                            respect_auto_mute: false,
                        },
                    )?;
                }
                self.control_tracks(
                    tracks,
                    TrackAction::InputMonitoringChanged {
                        enabled: !muted,
                        respect_auto_mute: false,
                    },
                )?;
            }
            SetBusGain { buses, gain_db } => {
                return self.control_buses(buses, BusAction::GainChanged(gain_db))
            }
            SetBusBalance { buses, balance } => {
                return self.control_buses(buses, BusAction::BalanceChanged(balance))
            }
            SetBusMuted { buses, muted } => {
                return self.control_buses(buses, BusAction::MuteChanged(muted))
            }
            SetApplyNCycles(value) => {
                self.global_action(GlobalControlAction::SetApplyNCycles(value))?
            }
            SetSolo(value) => self.global_action(GlobalControlAction::SetSolo(value))?,
            SetSyncActive(value) => self.global_action(GlobalControlAction::SetSync(value))?,
            SetPlayAfterRecord(value) => {
                self.global_action(GlobalControlAction::SetPlayAfterRecord(value))?
            }
            SetAutoMuteOtherTrackInputs(value) => {
                self.global_action(GlobalControlAction::SetAutoMuteOtherTrackInputs(value))?
            }
            SetDefaultRecordingAction(value) => {
                self.global_action(GlobalControlAction::SetDefaultRecordingAction(value))?
            }
        }
        Ok(Effect::None)
    }

    fn control_tracks(
        &mut self,
        tracks: Vec<TrackId>,
        action: TrackAction,
    ) -> Result<(), CommandError> {
        for id in &tracks {
            self.track(*id)?;
        }
        for id in tracks {
            self.track_action(id, action.clone())?;
        }
        Ok(())
    }
    fn control_buses(&mut self, buses: Vec<BusId>, action: BusAction) -> CommandResult {
        let control = match action {
            BusAction::GainChanged(value) => BackendBusControl::GainDb(value),
            BusAction::BalanceChanged(value) => BackendBusControl::Balance(value),
            BusAction::MuteChanged(value) => BackendBusControl::Mute(value),
            _ => return Err(CommandError::unsupported(BackendCapability::BusFx)),
        };
        for id in &buses {
            let bus = self
                .buses
                .get(id)
                .filter(|b| b.view.structural_state != StructuralState::Removing)
                .ok_or_else(|| CommandError::invalid("unknown bus"))?;
            control.normalized(bus.view.channels.len())?;
        }
        for bus in &buses {
            self.execute(Command::Bus {
                bus: *bus,
                action: action.clone(),
            })?;
        }
        Ok(Effect::BusControls(buses))
    }
}

fn control_mode(mode: LoopMode) -> Result<BackendLoopMode, CommandError> {
    match mode {
        LoopMode::Stopped => Ok(BackendLoopMode::Stopped),
        LoopMode::Playing => Ok(BackendLoopMode::Playing),
        LoopMode::Recording => Ok(BackendLoopMode::Recording),
        LoopMode::Replacing => Err(CommandError::unsupported(BackendCapability::ReplacingMode)),
        LoopMode::PlayingDryThroughWet => Ok(BackendLoopMode::PlayingDryThroughWet),
        LoopMode::RecordingDryIntoWet => Ok(BackendLoopMode::RecordingDryIntoWet),
        LoopMode::Unknown => Err(CommandError::invalid("unknown loop mode")),
    }
}
