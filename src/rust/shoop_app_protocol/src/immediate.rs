//! UI value commands. Submission has no operation identity or execution receipt.
use crate::{Command as Action, SessionSnapshot};
use alloc::{vec, vec::Vec};
use serde::{Deserialize, Serialize};
use shoop_app_api::*;

pub const COMMANDS: usize = 32;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Capabilities {
    pub enabled: bool,
    /// Direct audio loops supporting immediate transitions.
    pub playback: Vec<LoopId>,
    pub grab: Vec<LoopId>,
    pub refused: u64,
    pub plans: Vec<LoopTransitionPlan>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct LoopTransitionPlan {
    pub loop_id: LoopId,
    pub plan: TransitionPlan,
    pub failures: u32,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum Command {
    LoopGain(LoopId, f32),
    LoopBalance(LoopId, f32),
    Transition(LoopId, TransitionPlan),
    ClearPlannedTransitions(LoopId, ClearPlanned),
    AdoptRingbuffers {
        loop_id: LoopId,
        reverse_cycle_start: u32,
        cycles_length: u32,
        go_to_cycle: u32,
        mode: LoopMode,
    },
    Grab {
        loop_id: LoopId,
        sync: LoopId,
        target: Option<LoopId>,
        cycles: u32,
        play_after_record: bool,
        synced: bool,
    },
    InputGainDb(TrackId, f32),
    InputBalance(TrackId, f32),
    OutputGainDb(TrackId, f32),
    OutputBalance(TrackId, f32),
    OutputMute(TrackId, bool),
    BusGainDb(BusId, f32),
    BusBalance(BusId, f32),
    BusMute(BusId, bool),
}

pub fn bounded(commands: &[Command]) -> bool {
    !commands.is_empty() && commands.len() <= COMMANDS
}

/// Return a whole eligible UI action, or leave the entire action on its existing path.
pub fn group(action: &Action, state: &SessionSnapshot) -> Option<Vec<Command>> {
    if !state.immediate.enabled {
        return None;
    }
    let find = |id| {
        state
            .tracks
            .iter()
            .find_map(|t| t.loops.iter().find(|l| l.id == id))
    };
    let selected = || {
        state
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .filter(|l| l.selected)
            .map(|l| l.id)
            .collect::<Vec<_>>()
    };
    let default_delay = state.global.sync.then(|| {
        let sync = state
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .find(|l| l.sync)
            .map_or(0, |l| l.length_frames);
        state
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .find(|l| l.targeted)
            .map_or(0, |l| {
                if let Some(delay) = l.next_transition_delay {
                    delay
                } else if sync == 0 {
                    0
                } else {
                    u32::try_from(
                        l.length_frames.saturating_sub(libm::round(
                            f64::from(l.position) * l.length_frames as f64,
                        ) as u64)
                            / sync,
                    )
                    .unwrap_or(u32::MAX)
                }
            })
    });
    let planned = |ids: Vec<LoopId>,
                   mode,
                   solo: bool,
                   delay: Option<u32>,
                   cycles: u32|
     -> Option<Vec<Command>> {
        let mode = match mode {
            LoopMode::Playing => TransitionMode::Playing,
            LoopMode::Recording => TransitionMode::Recording,
            LoopMode::Stopped => TransitionMode::Stopped,
            _ => return None,
        };
        if ids.iter().any(|id| !state.immediate.playback.contains(id)) {
            return None;
        }
        if mode == TransitionMode::Playing
            && delay.is_some()
            && ids.iter().any(|id| {
                find(*id).is_some_and(|l| {
                    l.channel_timings
                        .iter()
                        .any(|channel| channel.preplay_samples > 0)
                })
            })
        {
            return None;
        }
        if mode == TransitionMode::Recording
            && cycles > 0
            && (delay.is_none() || ids.iter().any(|id| find(*id).is_some_and(|l| l.sync)))
        {
            return None;
        }
        let mut commands = Vec::new();
        let plan = |mode| -> Option<TransitionPlan> {
            let mut plan = TransitionPlan::default();
            let step = PlannedTransition {
                mode,
                skip: delay.unwrap_or(0),
            };
            if delay.is_some() {
                plan.first = Some(step);
            } else {
                plan.now = Some(mode);
            }
            if mode == TransitionMode::Recording && cycles > 0 {
                let finish = PlannedTransition {
                    mode: if state.global.play_after_record {
                        TransitionMode::Playing
                    } else {
                        TransitionMode::Stopped
                    },
                    skip: delay.unwrap_or(0).checked_add(cycles)?,
                };
                if delay.is_some() {
                    plan.second = Some(finish);
                } else {
                    plan.first = Some(finish);
                }
            }
            Some(plan)
        };
        if solo && state.global.solo && mode != TransitionMode::Stopped {
            for track in &state.tracks {
                if track.loops.iter().any(|l| ids.contains(&l.id)) {
                    for other in track.loops.iter().filter(|l| !ids.contains(&l.id)) {
                        if !state.immediate.playback.contains(&other.id) {
                            return None;
                        }
                        commands.push(Command::Transition(
                            other.id,
                            plan(TransitionMode::Stopped)?,
                        ));
                    }
                }
            }
        }
        for id in ids {
            commands.push(Command::Transition(id, plan(mode)?));
        }
        Some(commands)
    };
    let playback =
        |ids, mode, solo| planned(ids, mode, solo, default_delay, state.global.apply_n_cycles);
    let grab = |ids: Vec<LoopId>| -> Option<Vec<Command>> {
        if ids.iter().any(|id| !state.immediate.grab.contains(id)) {
            return None;
        }
        let sync = state
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .find(|l| l.sync)?
            .id;
        let target = state
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .find(|l| l.targeted)
            .map(|l| l.id);
        if find(sync).is_none_or(|l| l.composite_kind != CompositeKind::None)
            || target
                .is_some_and(|id| find(id).is_none_or(|l| l.composite_kind != CompositeKind::None))
        {
            return None;
        }
        let mut commands = Vec::new();
        if state.global.solo {
            for track in &state.tracks {
                if track.loops.iter().any(|l| ids.contains(&l.id)) {
                    for other in track.loops.iter().filter(|l| !ids.contains(&l.id)) {
                        if !state.immediate.playback.contains(&other.id)
                            && !state.immediate.grab.contains(&other.id)
                        {
                            return None;
                        }
                        commands.push(Command::Transition(
                            other.id,
                            TransitionMode::Stopped.into(),
                        ));
                    }
                }
            }
        }
        commands.extend(ids.into_iter().map(|loop_id| Command::Grab {
            loop_id,
            sync,
            target,
            cycles: state.global.apply_n_cycles.max(1),
            play_after_record: state.global.play_after_record,
            synced: state.global.sync,
        }));
        Some(commands)
    };
    let track = |id, action: &TrackAction| -> Option<Command> {
        Some(match *action {
            TrackAction::InputGainChanged(v) => Command::InputGainDb(id, v),
            TrackAction::InputBalanceChanged(v) => Command::InputBalance(id, v),
            TrackAction::OutputGainChanged(v) => Command::OutputGainDb(id, v),
            TrackAction::OutputBalanceChanged(v) => Command::OutputBalance(id, v),
            TrackAction::OutputMuteChanged(v) => Command::OutputMute(id, v),
            _ => return None,
        })
    };
    let bus = |id, action: &BusAction| -> Option<Command> {
        Some(match *action {
            BusAction::GainChanged(v) => Command::BusGainDb(id, v),
            BusAction::BalanceChanged(v) => Command::BusBalance(id, v),
            BusAction::MuteChanged(v) => Command::BusMute(id, v),
            _ => return None,
        })
    };
    let commands = match action {
        Action::Loop {
            loop_id,
            action: LoopAction::DefaultClicked,
            ..
        } if find(*loop_id).is_some_and(|l| {
            l.mode == LoopMode::Stopped
                && l.next_transition_delay.is_some()
                && !matches!(l.next_mode, LoopMode::Unknown | LoopMode::Stopped)
        }) && state.immediate.playback.contains(loop_id) =>
        {
            vec![Command::ClearPlannedTransitions(
                *loop_id,
                ClearPlanned::Both,
            )]
        }
        Action::Loop {
            loop_id,
            action: LoopAction::GrabClicked,
            ..
        } => {
            let entry = find(*loop_id)?;
            grab(if entry.selected {
                selected()
            } else {
                vec![*loop_id]
            })?
        }
        Action::Loop {
            loop_id,
            action: LoopAction::DefaultClicked,
            ..
        } if state.global.default_recording_action == DefaultRecordingAction::Grab
            && find(*loop_id).is_some_and(|l| l.empty && l.mode == LoopMode::Stopped) =>
        {
            grab(vec![*loop_id])?
        }

        Action::Loop {
            loop_id,
            action: LoopAction::GainChanged(v),
            ..
        } => vec![Command::LoopGain(*loop_id, *v)],
        Action::Loop {
            loop_id,
            action: LoopAction::BalanceChanged(v),
            ..
        } => vec![Command::LoopBalance(*loop_id, *v)],
        Action::Track { track: id, action } => vec![track(*id, action)?],
        Action::Bus { bus: id, action } => vec![bus(*id, action)?],
        Action::Loop {
            loop_id, action, ..
        } => {
            let entry = find(*loop_id)?;
            let (ids, mode) = match action {
                LoopAction::PlayClicked | LoopAction::StopClicked | LoopAction::RecordClicked => (
                    if entry.selected {
                        selected()
                    } else {
                        vec![*loop_id]
                    },
                    if matches!(action, LoopAction::PlayClicked) {
                        LoopMode::Playing
                    } else if matches!(action, LoopAction::RecordClicked) {
                        LoopMode::Recording
                    } else {
                        LoopMode::Stopped
                    },
                ),
                LoopAction::DefaultClicked => (
                    vec![*loop_id],
                    if entry.next_transition_delay.is_some()
                        && !matches!(entry.next_mode, LoopMode::Unknown | LoopMode::Stopped)
                    {
                        LoopMode::Stopped
                    } else if entry.mode == LoopMode::Recording {
                        LoopMode::Playing
                    } else if entry.mode == LoopMode::Stopped && entry.empty {
                        if state.global.default_recording_action == DefaultRecordingAction::Grab {
                            return grab(vec![*loop_id]);
                        }
                        LoopMode::Recording
                    } else if entry.mode == LoopMode::Stopped {
                        LoopMode::Playing
                    } else {
                        LoopMode::Stopped
                    },
                ),
                _ => return None,
            };
            playback(ids, mode, true)?
        }
        Action::PlaySelected => playback(selected(), LoopMode::Playing, true)?,
        Action::Global(GlobalControlAction::StopAll) => playback(
            state
                .tracks
                .iter()
                .flat_map(|t| &t.loops)
                .map(|l| l.id)
                .collect(),
            LoopMode::Stopped,
            true,
        )?,
        Action::Control(operation) => match operation {
            ControlOperation::SetTransitionPlan { loops, plan } => loops
                .iter()
                .map(|id| Command::Transition(*id, *plan))
                .collect(),
            ControlOperation::ClearPlannedTransitions { loops, which } => loops
                .iter()
                .map(|id| Command::ClearPlannedTransitions(*id, *which))
                .collect(),
            ControlOperation::RecordN {
                loops,
                n_cycles,
                cycles_delay,
            } => planned(
                loops.clone(),
                LoopMode::Recording,
                true,
                Some(*cycles_delay),
                *n_cycles,
            )?,

            ControlOperation::Grab { loops } => grab(loops.clone())?,
            ControlOperation::AdoptRingbuffers {
                loops,
                reverse_cycle_start,
                cycles_length,
                go_to_cycle,
                go_to_mode,
            } if matches!(
                go_to_mode,
                LoopMode::Stopped | LoopMode::Playing | LoopMode::Recording | LoopMode::Unknown
            ) && loops.iter().all(|id| state.immediate.grab.contains(id)) =>
            {
                loops
                    .iter()
                    .map(|loop_id| Command::AdoptRingbuffers {
                        loop_id: *loop_id,
                        reverse_cycle_start: *reverse_cycle_start,
                        cycles_length: *cycles_length,
                        go_to_cycle: *go_to_cycle,
                        mode: if *go_to_mode == LoopMode::Unknown {
                            LoopMode::Stopped
                        } else {
                            *go_to_mode
                        },
                    })
                    .collect()
            }

            ControlOperation::SetLoopGain { loops, gain } => loops
                .iter()
                .map(|id| Command::LoopGain(*id, *gain))
                .collect(),
            ControlOperation::SetLoopBalance { loops, balance } => loops
                .iter()
                .map(|id| Command::LoopBalance(*id, *balance))
                .collect(),
            ControlOperation::SetTrackGain { tracks, gain_db } => tracks
                .iter()
                .map(|id| Command::OutputGainDb(*id, *gain_db))
                .collect(),
            ControlOperation::SetTrackBalance { tracks, balance } => tracks
                .iter()
                .map(|id| Command::OutputBalance(*id, *balance))
                .collect(),
            ControlOperation::SetTrackMuted { tracks, muted } => tracks
                .iter()
                .map(|id| Command::OutputMute(*id, *muted))
                .collect(),
            ControlOperation::SetTrackInputGain { tracks, gain_db } => tracks
                .iter()
                .map(|id| Command::InputGainDb(*id, *gain_db))
                .collect(),
            ControlOperation::SetTrackInputBalance { tracks, balance } => tracks
                .iter()
                .map(|id| Command::InputBalance(*id, *balance))
                .collect(),
            ControlOperation::SetBusGain { buses, gain_db } => buses
                .iter()
                .map(|id| Command::BusGainDb(*id, *gain_db))
                .collect(),
            ControlOperation::SetBusBalance { buses, balance } => buses
                .iter()
                .map(|id| Command::BusBalance(*id, *balance))
                .collect(),
            ControlOperation::SetBusMuted { buses, muted } => buses
                .iter()
                .map(|id| Command::BusMute(*id, *muted))
                .collect(),
            ControlOperation::Transition {
                loops,
                mode,
                cycles_delay: None,
                align_to_sync_at: None,
            } => planned(loops.clone(), *mode, false, None, 0)?,
            ControlOperation::Trigger { loops, mode } => playback(loops.clone(), *mode, true)?,
            ControlOperation::TriggerDefaultPlayback { loops } => {
                playback(loops.clone(), LoopMode::Playing, true)?
            }
            _ => return None,
        },
        _ => return None,
    };
    if !bounded(&commands)
        || commands.iter().any(|command| match command {
            Command::LoopGain(id, _) | Command::LoopBalance(id, _) => {
                state.composites.iter().any(|c| c.loop_id == *id)
            }
            _ => false,
        })
    {
        return None;
    }
    Some(commands)
}
