use super::*;
use shoop_egui::{CompositeEventId, CompositeKind};

#[derive(Default)]
pub struct Canary {
    stage: u8,
    mode: usize,
    event: u64,
    started: u64,
}

impl Canary {
    pub fn advance(
        &mut self,
        snapshot: &AppSnapshot,
        pending: bool,
        track: &TrackState,
        send: &mut impl FnMut(AppIntent) -> Result<(), String>,
    ) -> Result<bool, String> {
        if pending {
            return Ok(false);
        }
        let modes = [
            (LoopMode::PlayingDryThroughWet, "playing_dry_through_wet"),
            (LoopMode::RecordingDryIntoWet, "recording_dry_into_wet"),
            (LoopMode::Replacing, "replacing"),
        ];
        let action = |index: usize, action| AppIntent::Loop {
            track_id: track.id,
            loop_id: track.loops[index].id,
            action,
        };
        let details = snapshot.details.as_ref().and_then(|d| d.composite.as_ref());
        match self.stage {
            0 => {
                send(action(0, LoopAction::ComposeIntoEnd(track.loops[1].id)))?;
                send(action(
                    1,
                    LoopAction::IconClicked(SelectionModifiers::default()),
                ))?;
                self.stage = 1;
            }
            1 => {
                let Some(event) = details.and_then(|d| d.events.first()) else {
                    return Ok(false);
                };
                self.event = event.instance_id;
                send(AppIntent::SetCompositeKind {
                    target_loop_id: track.loops[1].id,
                    kind: CompositeKind::Script,
                })?;
                self.stage = 7;
            }
            7 => {
                if !details.is_some_and(|d| {
                    d.kind == CompositeKind::Script && d.execution.pending_version.is_none()
                }) {
                    return Ok(false);
                }
                send(AppIntent::SetCompositeLoopCycles {
                    target_loop_id: track.loops[1].id,
                    event: CompositeEventId {
                        instance_id: self.event,
                    },
                    n_cycles: Some(32),
                })?;
                self.stage = 2;
            }
            2 => {
                if !details.is_some_and(|d| {
                    d.kind == CompositeKind::Script && d.events[0].forced_n_cycles == Some(32)
                }) {
                    return Ok(false);
                }
                send(action(1, LoopAction::ComposeIntoEnd(track.loops[2].id)))?;
                self.stage = 3;
            }
            3 => {
                send(AppIntent::SetCompositeEventMode {
                    target_loop_id: track.loops[1].id,
                    event: CompositeEventId {
                        instance_id: self.event,
                    },
                    mode: modes[self.mode].0,
                })?;
                self.stage = 4;
            }
            4 => {
                if !details.is_some_and(|d| {
                    d.events[0].mode.as_deref() == Some(modes[self.mode].1)
                        && d.execution.pending_version.is_none()
                }) {
                    return Ok(false);
                }
                send(action(0, LoopAction::DuplicateTo(track.loops[5].id)))?;
                self.stage = 8;
            }
            8 => {
                if track.loops[5].composite_kind != CompositeKind::None
                    || track.loops[5].name != track.loops[0].name
                {
                    return Ok(false);
                }
                send(action(1, LoopAction::DuplicateTo(track.loops[5].id)))?;
                self.stage = 9;
            }
            9 => {
                if track.loops[5].composite_kind != CompositeKind::Script
                    || track.loops[5].name != track.loops[1].name
                {
                    return Ok(false);
                }
                send(action(2, LoopAction::DuplicateTo(track.loops[6].id)))?;
                self.stage = 10;
            }
            10 => {
                if track.loops[6].composite_kind != CompositeKind::Regular
                    || track.loops[6].name != track.loops[2].name
                {
                    return Ok(false);
                }
                send(action(6, LoopAction::DefaultClicked))?;
                self.started = snapshot.status.processed_frames;
                self.stage = 5;
            }
            5 => {
                if track.loops[0].mode != modes[self.mode].0
                    || track.loops[6].mode != LoopMode::Playing
                    || snapshot.status.processed_frames <= self.started + 512
                {
                    return Ok(false);
                }
                send(action(6, LoopAction::StopClicked))?;
                self.stage = 6;
            }
            6 => {
                if track.loops[..7].iter().any(|l| l.mode != LoopMode::Stopped) {
                    return Ok(false);
                }
                self.mode += 1;
                if self.mode == modes.len() {
                    return Ok(true);
                }
                self.stage = 3;
            }
            _ => return Err("invalid composite role probe stage".into()),
        }
        Ok(false)
    }
}
