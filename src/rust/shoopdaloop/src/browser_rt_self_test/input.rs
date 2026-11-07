//! Public input edits with render-owned completion while the main thread is stalled.
use super::*;
use shoop_egui::{BackendCapability, DefaultRecordingAction, ScriptKind};
use shoop_session::SessionBundle;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Stage {
    #[default]
    Start,
    Clock,
    SelectAudio,
    SelectBoth,
    Grabbed,
    Playback,
    TransportStopped,
    Prefix,
    Stall,
    UiReplacing,
    UiStopped,
    Replacing,
    Armed,
    Note,
    Released,
    Stopped,
    Saving,
    Loaded,
    Resaving,
    Edited,
    Passed,
}
#[derive(Default)]
pub struct Canary {
    stage: Stage,
    since: Option<Instant>,
    period: u64,
    bytes: Option<Arc<Vec<u8>>>,
    before: Option<SessionBundle>,
    after: Option<SessionBundle>,
    error: Option<String>,
}
impl Canary {
    pub fn file_output(&mut self, output: &crate::application::ApplicationFileOutput) {
        match shoop_session::decode_session(&output.bytes) {
            Ok(bundle) if self.stage == Stage::Saving => {
                self.before = Some(bundle);
                self.bytes = Some(output.bytes.clone());
            }
            Ok(bundle) if self.stage == Stage::Resaving => self.after = Some(bundle),
            Ok(_) => self.error = Some("unexpected input file output".into()),
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn next(&mut self, stage: Stage) {
        self.stage = stage;
        self.since = Some(Instant::now());
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("runtime_status"))
        {
            let _ = element.set_attribute("data-input-self-test", &format!("{stage:?}"));
        }
        tracing::info!(?stage, "frontend.input.browser_probe");
    }
    pub fn advance(
        &mut self,
        snapshot: &AppSnapshot,
        pending: bool,
        send: &mut impl FnMut(AppIntent) -> Result<(), String>,
    ) -> Result<bool, String> {
        if self
            .since
            .is_some_and(|since| since.elapsed() > Duration::from_secs(120))
        {
            return Err(format!(
                "input {:?} timed out: {:?}",
                self.stage, snapshot.notifications
            ));
        }
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        if pending || snapshot.tracks.len() < 3 {
            return Ok(false);
        }
        let ids = Ids {
            sync: Some((snapshot.tracks[0].id, snapshot.tracks[0].loops[0].id)),
            audio: Some((snapshot.tracks[1].id, snapshot.tracks[1].loops[0].id)),
            keys: Some((snapshot.tracks[2].id, snapshot.tracks[2].loops[0].id)),
            master: None,
        };
        let lua = |name: &str, source: &str| AppIntent::AddScriptSource {
            name: name.into(),
            source: source.into(),
            kind: ScriptKind::User,
            enabled: true,
        };
        match self.stage {
            Stage::Start => {
                if !snapshot.capabilities.supports(BackendCapability::Grab)
                    || !snapshot
                        .capabilities
                        .supports(BackendCapability::ReplacingMode)
                {
                    return Err("input capabilities missing".into());
                }
                self.period = first_loop(snapshot, ids.sync)?.length_frames;
                send(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
                send(loop_intent(ids.sync, LoopAction::PlayClicked)?)?;
                send(AppIntent::Global(GlobalControlAction::DeselectAll))?;
                self.next(Stage::Clock);
            }
            Stage::Clock => {
                if first_loop(snapshot, ids.sync)?.mode != LoopMode::Playing {
                    return Ok(false);
                }
                for setting in [
                    GlobalControlAction::SetSync(true),
                    GlobalControlAction::SetApplyNCycles(1),
                    GlobalControlAction::SetPlayAfterRecord(false),
                    GlobalControlAction::SetDefaultRecordingAction(DefaultRecordingAction::Grab),
                ] {
                    send(AppIntent::Global(setting))?;
                }
                send(loop_intent(
                    ids.audio,
                    LoopAction::IconClicked(Default::default()),
                )?)?;
                self.next(Stage::SelectAudio);
            }
            Stage::SelectAudio => {
                if !first_loop(snapshot, ids.audio)?.selected {
                    return Ok(false);
                }
                send(loop_intent(
                    ids.keys,
                    LoopAction::IconClicked(SelectionModifiers { additive: true }),
                )?)?;
                self.next(Stage::SelectBoth);
            }
            Stage::SelectBoth => {
                if !first_loop(snapshot, ids.audio)?.selected
                    || !first_loop(snapshot, ids.keys)?.selected
                {
                    return Ok(false);
                }
                send(loop_intent(ids.audio, LoopAction::GrabClicked)?)?;
                self.next(Stage::Grabbed);
            }
            Stage::Grabbed => {
                if [ids.audio, ids.keys].iter().any(|id| {
                    first_loop(snapshot, *id).is_ok_and(|l| {
                        l.mode != LoopMode::Stopped || l.length_frames != self.period
                    })
                }) {
                    return Ok(false);
                }
                send(loop_intent(ids.audio, LoopAction::PlayClicked)?)?;
                self.next(Stage::Playback);
            }
            Stage::Playback => {
                if [ids.audio, ids.keys]
                    .iter()
                    .any(|id| first_loop(snapshot, *id).is_ok_and(|l| l.mode != LoopMode::Playing))
                {
                    return Ok(false);
                }
                send(loop_intent(ids.audio, LoopAction::StopClicked)?)?;
                self.next(Stage::TransportStopped);
            }
            Stage::TransportStopped => {
                if [ids.audio, ids.keys]
                    .iter()
                    .any(|id| first_loop(snapshot, *id).is_ok_and(|l| l.mode != LoopMode::Stopped))
                {
                    return Ok(false);
                }
                send(AppIntent::Global(GlobalControlAction::DeselectAll))?;
                for setting in [
                    GlobalControlAction::SetSync(false),
                    GlobalControlAction::SetApplyNCycles(2),
                    GlobalControlAction::SetPlayAfterRecord(true),
                ] {
                    send(AppIntent::Global(setting))?;
                }
                send(AppIntent::Loop {
                    track_id: snapshot.tracks[1].id,
                    loop_id: snapshot.tracks[1].loops[1].id,
                    action: LoopAction::GrabClicked,
                })?;
                self.next(Stage::Prefix);
            }
            Stage::Prefix => {
                let prefix = &snapshot.tracks[1].loops[1];
                if prefix.mode != LoopMode::Recording || prefix.next_transition_delay.is_none() {
                    return Ok(false);
                }
                self.next(Stage::Stall);
            }
            Stage::Stall => {
                let stalled = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("runtime_status"))
                    .is_some_and(|e| e.has_attribute("data-input-stalled"));
                let prefix = &snapshot.tracks[1].loops[1];
                if !stalled
                    || prefix.mode != LoopMode::Playing
                    || prefix.length_frames != 2 * self.period
                {
                    return Ok(false);
                }
                send(loop_intent(ids.audio, LoopAction::ReplaceClicked)?)?;
                self.next(Stage::UiReplacing);
            }
            Stage::UiReplacing => {
                if first_loop(snapshot, ids.audio)?.mode != LoopMode::Replacing {
                    return Ok(false);
                }
                send(loop_intent(ids.audio, LoopAction::StopClicked)?)?;
                self.next(Stage::UiStopped);
            }
            Stage::UiStopped => {
                if first_loop(snapshot, ids.audio)?.mode != LoopMode::Stopped {
                    return Ok(false);
                }
                send(lua("browser-replace.lua", "shoop_announce_api_version(1,0); local c=require('shoop_control'); c.loop_trigger({{0,0},{1,0}}, c.constants.LoopMode_Replacing)"))?;
                self.next(Stage::Replacing);
            }
            Stage::Replacing => {
                if [ids.audio, ids.keys].iter().any(|id| {
                    first_loop(snapshot, *id).is_ok_and(|l| {
                        l.mode != LoopMode::Replacing || l.length_frames != self.period
                    })
                }) {
                    return Ok(false);
                }
                if self.since.unwrap().elapsed().as_secs_f64()
                    < 3.0 * self.period as f64 / snapshot.status.sample_rate as f64
                {
                    return Ok(false);
                }
                send(loop_intent(
                    ids.keys,
                    LoopAction::IconClicked(Default::default()),
                )?)?;
                send(AppIntent::Track {
                    track_id: ids.keys.unwrap().0,
                    action: TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                })?;
                self.next(Stage::Armed);
            }
            Stage::Armed => {
                if !snapshot.tracks[2].controls.input_monitoring {
                    return Ok(false);
                }
                send(AppIntent::Piano(PianoAction::Press(
                    MidiNote::new(60).unwrap(),
                )))?;
                self.next(Stage::Note);
            }
            Stage::Note => {
                if self.since.unwrap().elapsed() < NOTE_HOLD {
                    return Ok(false);
                }
                send(AppIntent::Piano(PianoAction::Release(
                    MidiNote::new(60).unwrap(),
                )))?;
                self.next(Stage::Released);
            }
            Stage::Released => {
                if self.since.unwrap().elapsed() < Duration::from_millis(50) {
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
                if snapshot.tracks[1].loops[0].length_frames != self.period
                    || snapshot.tracks[2].loops[0].length_frames != self.period
                {
                    return Err("replacement changed the period".into());
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
                if !before.media.values().any(|m| matches!(m, shoop_session::MediaPayload::Midi(m) if m.events.iter().any(|e| e.data.first().is_some_and(|s| s & 0xf0 == 0x90)))) { return Err("replacement lost MIDI input".into()); }
                send(AppIntent::LoadSessionBytes {
                    name: "input.shoop".into(),
                    bytes: self.bytes.as_ref().unwrap().clone(),
                })?;
                self.next(Stage::Loaded);
            }
            Stage::Loaded => {
                if !snapshot.io_task.as_ref().is_some_and(|t| {
                    t.kind == shoop_egui::IoTaskKind::LoadSession
                        && t.status == IoTaskStatus::Completed
                }) {
                    return Ok(false);
                }
                if snapshot.global_controls.default_recording_action != DefaultRecordingAction::Grab
                    || snapshot
                        .tracks
                        .iter()
                        .flat_map(|t| &t.loops)
                        .any(|l| l.mode != LoopMode::Stopped)
                {
                    return Err("input reload lost Grab or stopped modes".into());
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
                if self
                    .before
                    .as_ref()
                    .unwrap()
                    .media
                    .values()
                    .collect::<Vec<_>>()
                    != after.media.values().collect::<Vec<_>>()
                {
                    return Err("input file round trip changed PCM or MIDI".into());
                }
                send(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
                send(loop_intent(ids.audio, LoopAction::ReplaceClicked)?)?;
                self.next(Stage::Edited);
            }
            Stage::Edited => {
                if first_loop(snapshot, ids.audio)?.mode != LoopMode::Replacing {
                    return Ok(false);
                }
                send(loop_intent(ids.audio, LoopAction::StopClicked)?)?;
                self.next(Stage::Passed);
            }
            Stage::Passed => return Ok(first_loop(snapshot, ids.audio)?.mode == LoopMode::Stopped),
        }
        Ok(false)
    }
}
