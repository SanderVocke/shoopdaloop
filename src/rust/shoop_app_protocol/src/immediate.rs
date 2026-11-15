//! UI value commands. Submission has no operation identity or execution receipt.
use crate::{Command as Action, SessionSnapshot};
use alloc::{vec, vec::Vec};
use serde::{Deserialize, Serialize};
use shoop_app_api::*;

pub const COMMANDS: usize = 32;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Capabilities {
    pub enabled: bool,
    pub playback: Vec<LoopId>,
    pub refused: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum Command {
    LoopGain(LoopId, f32),
    LoopBalance(LoopId, f32),
    Start(LoopId),
    Stop(LoopId),
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
    let playback = |ids: Vec<LoopId>, mode, solo: bool| -> Option<Vec<Command>> {
        if !matches!(mode, LoopMode::Playing | LoopMode::Stopped)
            || ids.iter().any(|id| {
                !state.immediate.playback.contains(id)
                    || (mode == LoopMode::Playing && find(*id).is_none_or(|l| l.empty))
            })
        {
            return None;
        }
        let mut commands = Vec::new();
        if solo && state.global.solo && mode == LoopMode::Playing {
            for track in &state.tracks {
                if track.loops.iter().any(|l| ids.contains(&l.id)) {
                    for other in track.loops.iter().filter(|l| !ids.contains(&l.id)) {
                        if !state.immediate.playback.contains(&other.id) {
                            return None;
                        }
                        commands.push(Command::Stop(other.id));
                    }
                }
            }
        }
        commands.extend(ids.into_iter().map(|id| {
            if mode == LoopMode::Playing {
                Command::Start(id)
            } else {
                Command::Stop(id)
            }
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
    let unsynced = !state.global.sync;
    let commands = match action {
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
        } if unsynced => {
            let entry = find(*loop_id)?;
            let (ids, mode) = match action {
                LoopAction::PlayClicked | LoopAction::StopClicked => (
                    if entry.selected {
                        selected()
                    } else {
                        vec![*loop_id]
                    },
                    if matches!(action, LoopAction::PlayClicked) {
                        LoopMode::Playing
                    } else {
                        LoopMode::Stopped
                    },
                ),
                LoopAction::DefaultClicked => (
                    vec![*loop_id],
                    if entry.mode == LoopMode::Stopped {
                        LoopMode::Playing
                    } else {
                        LoopMode::Stopped
                    },
                ),
                _ => return None,
            };
            playback(ids, mode, true)?
        }
        Action::PlaySelected if unsynced => playback(selected(), LoopMode::Playing, true)?,
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
            } => playback(loops.clone(), *mode, false)?,
            ControlOperation::Trigger { loops, mode } if unsynced => {
                playback(loops.clone(), *mode, true)?
            }
            ControlOperation::TriggerDefaultPlayback { loops } if unsynced => {
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
