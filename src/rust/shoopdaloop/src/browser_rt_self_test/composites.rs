//! Public composite canary on actual Web Audio, including a stalled main thread.
use super::*;
use shoop_egui::{BackendCapability, CompositeEventId, CompositeKind, ScriptKind};
use shoop_session::{MediaPayload, SessionBundle};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Stage {
    #[default]
    Start,
    Nested,
    Convert,
    Basis,
    Insert,
    Duration,
    Kind,
    Mode,
    Playing,
    Stall,
    CopyPrimitive,
    OverwritePrimitive,
    CopyComposite,
    CopiesReady,
    Saving,
    Loaded,
    Resaving,
    Playback,
    Stopped,
    Passed,
}

#[derive(Default)]
pub struct Canary {
    stage: Stage,
    track: Option<TrackId>,
    loops: Vec<shoop_egui::LoopId>,
    event: u64,
    before: Option<SessionBundle>,
    after: Option<SessionBundle>,
    bytes: Option<Arc<Vec<u8>>>,
    error: Option<String>,
    before_frames: u64,
    since: Option<Instant>,
}

impl Canary {
    pub fn file_output(&mut self, output: &crate::application::ApplicationFileOutput) {
        match shoop_session::decode_session(&output.bytes) {
            Ok(bundle) if self.stage == Stage::Saving => {
                self.before = Some(bundle);
                self.bytes = Some(output.bytes.clone());
            }
            Ok(bundle) if self.stage == Stage::Resaving => self.after = Some(bundle),
            Ok(_) => self.error = Some("unexpected composite file output".into()),
            Err(error) => self.error = Some(format!("composite file decode: {error}")),
        }
    }

    fn next(&mut self, stage: Stage) {
        self.stage = stage;
        self.since = Some(Instant::now());
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("runtime_status"))
        {
            let _ = element.set_attribute("data-composite-self-test", &format!("{stage:?}"));
        }
        tracing::info!(?stage, "frontend.composites.browser_probe");
    }

    pub fn advance(
        &mut self,
        snapshot: &AppSnapshot,
        pending: bool,
        send: &mut impl FnMut(AppIntent) -> Result<(), String>,
    ) -> Result<bool, String> {
        let timeout = if matches!(self.stage, Stage::Saving | Stage::Loaded | Stage::Resaving) {
            Duration::from_secs(120)
        } else {
            STEP_TIMEOUT
        };
        if self.since.is_some_and(|since| since.elapsed() > timeout) {
            return Err(format!(
                "composite {:?} timed out: {:?}",
                self.stage, snapshot.io_task
            ));
        }
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        if pending {
            return Ok(false);
        }
        let track = snapshot.tracks.get(1).ok_or("composite track missing")?;
        let details = snapshot.details.as_ref().and_then(|d| d.composite.as_ref());
        let action = |id, action| AppIntent::Loop {
            track_id: track.id,
            loop_id: id,
            action,
        };
        match self.stage {
            Stage::Start => {
                if !snapshot
                    .capabilities
                    .supports(BackendCapability::CompositeLoops)
                {
                    return Err("composite capability missing".into());
                }
                if track.loops.len() < 8 {
                    return Err("composite canary requires eight loop slots".into());
                }
                self.track = Some(track.id);
                self.loops = track.loops.iter().map(|l| l.id).collect();
                send(AppIntent::AddScriptSource {
                    name: "composite-canary.lua".into(), kind: ScriptKind::User, enabled: true,
                    source: Arc::from("shoop_announce_api_version(1,0); local c=require('shoop_control'); c.loop_compose_add_to_end({0,2}, {{0,0}}, false); c.loop_compose_add_to_end({0,3}, {{0,2}}, false); c.loop_trigger({0,3}, c.constants.LoopMode_Playing)"),
                })?;
                self.next(Stage::Nested);
            }
            Stage::Nested => {
                if track.loops[3].mode != LoopMode::Playing {
                    return Ok(false);
                }
                if track.loops[3].length_frames != track.loops[2].length_frames
                    || track.loops[3].length_frames == 0
                {
                    return Err("nested public Lua geometry differs".into());
                }
                send(action(self.loops[4], LoopAction::ConvertToComposite))?;
                send(action(
                    self.loops[4],
                    LoopAction::IconClicked(SelectionModifiers::default()),
                ))?;
                self.next(Stage::Convert);
            }
            Stage::Convert => {
                if details.is_none() {
                    return Ok(false);
                }
                send(AppIntent::SetCompositeTimingBasis {
                    target_loop_id: self.loops[4],
                    sample_rate: snapshot.status.sample_rate,
                    unit_frames: 128,
                })?;
                self.next(Stage::Basis);
            }
            Stage::Basis => {
                if !details.is_some_and(|d| d.timing.is_some_and(|t| t.basis_frames == 128)) {
                    return Ok(false);
                }
                send(AppIntent::ComposeLoopAt {
                    target_loop_id: self.loops[4],
                    source_loop_id: self.loops[5],
                    start_iteration: 400,
                })?;
                self.next(Stage::Insert);
            }
            Stage::Insert => {
                let Some(event) = details.and_then(|d| d.events.first()) else {
                    return Ok(false);
                };
                self.event = event.instance_id;
                send(AppIntent::SetCompositeLoopCycles {
                    target_loop_id: self.loops[4],
                    event: CompositeEventId {
                        instance_id: self.event,
                    },
                    n_cycles: Some(32),
                })?;
                self.next(Stage::Duration);
            }
            Stage::Duration => {
                if !details.is_some_and(|d| d.events[0].forced_n_cycles == Some(32)) {
                    return Ok(false);
                }
                send(AppIntent::SetCompositeKind {
                    target_loop_id: self.loops[4],
                    kind: CompositeKind::Script,
                })?;
                self.next(Stage::Kind);
            }
            Stage::Kind => {
                if !details.is_some_and(|d| d.kind == CompositeKind::Script) {
                    return Ok(false);
                }
                send(AppIntent::SetCompositeEventMode {
                    target_loop_id: self.loops[4],
                    event: CompositeEventId {
                        instance_id: self.event,
                    },
                    mode: LoopMode::Recording,
                })?;
                self.next(Stage::Mode);
            }
            Stage::Mode => {
                if !details.is_some_and(|d| {
                    d.events[0].mode.as_deref() == Some("recording")
                        && d.execution.pending_version.is_none()
                }) {
                    return Ok(false);
                }
                send(action(self.loops[4], LoopAction::PlayClicked))?;
                self.next(Stage::Playing);
            }
            Stage::Playing => {
                if track.loops[4].mode != LoopMode::Playing {
                    return Ok(false);
                }
                self.before_frames = snapshot.status.processed_frames;
                self.next(Stage::Stall);
            }
            Stage::Stall => {
                let blocked = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("runtime_status"))
                    .is_some_and(|e| {
                        e.get_attribute("data-composite-stalled").as_deref() == Some("true")
                    });
                if !blocked {
                    return Ok(false);
                }
                if track.loops[4].mode != LoopMode::Stopped || track.loops[5].length_frames != 4096
                {
                    return Ok(false);
                }
                if snapshot.status.processed_frames <= self.before_frames
                    || track.loops[3].mode != LoopMode::Playing
                {
                    return Err("AudioWorklet did not continue through main-thread stall".into());
                }
                send(AppIntent::Global(GlobalControlAction::StopAll))?;
                self.next(Stage::CopyPrimitive);
            }
            Stage::CopyPrimitive => {
                if track.loops.iter().any(|l| l.mode != LoopMode::Stopped) {
                    return Ok(false);
                }
                send(action(
                    self.loops[0],
                    LoopAction::DuplicateTo(self.loops[6]),
                ))?;
                self.next(Stage::OverwritePrimitive);
            }
            Stage::OverwritePrimitive => {
                if track.loops[6].name != track.loops[0].name {
                    return Ok(false);
                }
                send(action(
                    self.loops[1],
                    LoopAction::DuplicateTo(self.loops[6]),
                ))?;
                self.next(Stage::CopyComposite);
            }
            Stage::CopyComposite => {
                if track.loops[6].name != track.loops[1].name {
                    return Ok(false);
                }
                send(action(
                    self.loops[3],
                    LoopAction::DuplicateTo(self.loops[7]),
                ))?;
                self.next(Stage::CopiesReady);
            }
            Stage::CopiesReady => {
                if track.loops[7].name != track.loops[3].name {
                    return Ok(false);
                }
                if [6, 7].iter().any(|i| {
                    track.loops[*i].mode != LoopMode::Stopped || track.loops[*i].position != 0.
                }) {
                    return Err("copies did not finish stopped at zero".into());
                }
                send(AppIntent::RequestSaveSession)?;
                self.next(Stage::Saving);
            }
            Stage::Saving => {
                let Some(before) = &self.before else {
                    return Ok(false);
                };
                if !output_completed(snapshot)? {
                    return Ok(false);
                }
                crate::copy_probe::verify(before)?;
                let child = &before
                    .document
                    .track_groups
                    .iter()
                    .flat_map(|g| &g.tracks)
                    .nth(1)
                    .ok_or("saved track missing")?
                    .loops[5];
                let audio = child
                    .channels
                    .iter()
                    .filter_map(|c| c.media_id.as_ref())
                    .filter_map(|id| before.media.get(id))
                    .filter_map(|p| {
                        if let MediaPayload::Audio(a) = p {
                            Some(a)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                if child.length_frames != 4096
                    || audio.is_empty()
                    || audio.iter().any(|a| {
                        a.samples.len() != 4096 || !a.samples.iter().any(|s| s.abs() > 0.001)
                    })
                {
                    return Err(
                        "stalled composite failed to capture exact audible microphone section"
                            .into(),
                    );
                }
                send(AppIntent::LoadSessionBytes {
                    name: "composites.shoop".into(),
                    bytes: self.bytes.as_ref().unwrap().clone(),
                })?;
                self.next(Stage::Loaded);
            }
            Stage::Loaded => {
                if track.id == self.track.unwrap()
                    || !snapshot.io_task.as_ref().is_some_and(|t| {
                        t.kind == shoop_egui::IoTaskKind::LoadSession
                            && t.status == IoTaskStatus::Completed
                    })
                {
                    return Ok(false);
                }
                if snapshot
                    .tracks
                    .iter()
                    .flat_map(|t| &t.loops)
                    .any(|l| l.mode != LoopMode::Stopped)
                {
                    return Err("composite load started transport".into());
                }
                send(AppIntent::RequestSaveSession)?;
                self.next(Stage::Resaving);
            }
            Stage::Resaving => {
                let Some(after) = &self.after else {
                    return Ok(false);
                };
                if !output_completed(snapshot)? {
                    return Ok(false);
                }
                crate::copy_probe::verify(after)?;
                let before = self.before.as_ref().unwrap();
                if before.media.values().collect::<Vec<_>>()
                    != after.media.values().collect::<Vec<_>>()
                {
                    return Err("composite file round trip changed PCM or exact MIDI".into());
                }
                let old_loops = before
                    .document
                    .track_groups
                    .iter()
                    .flat_map(|g| &g.tracks)
                    .flat_map(|t| &t.loops)
                    .collect::<Vec<_>>();
                let new_loops = after
                    .document
                    .track_groups
                    .iter()
                    .flat_map(|g| &g.tracks)
                    .flat_map(|t| &t.loops)
                    .collect::<Vec<_>>();
                let remap = old_loops
                    .iter()
                    .zip(&new_loops)
                    .map(|(a, b)| (a.id, b.id))
                    .collect::<std::collections::BTreeMap<_, _>>();
                for (old, new) in old_loops.iter().zip(new_loops) {
                    let mut expected = old.composite.clone();
                    if let Some(c) = &mut expected {
                        for e in &mut c.instances {
                            e.loop_id = remap[&e.loop_id];
                        }
                        for s in &mut c.timing.as_mut().ok_or("frozen timing missing")?.spans {
                            s.target = remap[&s.target];
                        }
                    }
                    if expected != new.composite {
                        return Err(
                            "composite round trip changed frozen timing or event IDs".into()
                        );
                    }
                }
                send(action(track.loops[3].id, LoopAction::PlayClicked))?;
                self.next(Stage::Playback);
            }
            Stage::Playback => {
                if track.loops[3].mode != LoopMode::Playing
                    || track.loops[0].mode != LoopMode::Playing
                {
                    return Ok(false);
                }
                send(AppIntent::Global(GlobalControlAction::StopAll))?;
                self.next(Stage::Stopped);
            }
            Stage::Stopped => {
                if snapshot
                    .tracks
                    .iter()
                    .flat_map(|t| &t.loops)
                    .any(|l| l.mode != LoopMode::Stopped)
                {
                    return Ok(false);
                }
                self.next(Stage::Passed);
            }
            Stage::Passed => return Ok(true),
        }
        Ok(false)
    }
}
