use std::collections::BTreeMap;
use std::fmt;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Result;
use shoop_app_api::{
    AppIntent, AppNotification, AppSnapshot, GlobalControlAction, LoopAction, LoopId, LoopMode,
    LoopState, NotificationLevel, StatusState, TrackId, TrackState,
};
use shoop_backend::{Backend, BackendLoopId, BackendLoopMode, BackendSnapshot};

const COMMAND_CAPACITY: usize = 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone)]
pub struct ApplicationHandle {
    sender: SyncSender<ApplicationMessage>,
    snapshot: Arc<RwLock<Arc<AppSnapshot>>>,
}

impl ApplicationHandle {
    pub fn dispatch(&self, intent: AppIntent) -> Result<(), DispatchError> {
        self.sender
            .try_send(ApplicationMessage::Intent(intent))
            .map_err(DispatchError::from)
    }

    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    Full,
    Disconnected,
}

impl fmt::Display for DispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => formatter.write_str("application command queue is full"),
            Self::Disconnected => formatter.write_str("application actor is disconnected"),
        }
    }
}

impl std::error::Error for DispatchError {}

impl From<TrySendError<ApplicationMessage>> for DispatchError {
    fn from(error: TrySendError<ApplicationMessage>) -> Self {
        match error {
            TrySendError::Full(_) => Self::Full,
            TrySendError::Disconnected(_) => Self::Disconnected,
        }
    }
}

pub struct ApplicationRuntime {
    handle: ApplicationHandle,
    join: Option<JoinHandle<()>>,
}

impl ApplicationRuntime {
    pub fn start(mut backend: Box<dyn Backend>) -> Result<Self> {
        let model = ApplicationModel::initialize(&mut *backend)?;
        let initial = Arc::new(model.snapshot());
        let snapshot = Arc::new(RwLock::new(initial));
        let (sender, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let handle = ApplicationHandle {
            sender,
            snapshot: Arc::clone(&snapshot),
        };
        let join = thread::Builder::new()
            .name("shoop-application".to_owned())
            .spawn(move || run_actor(model, backend, receiver, snapshot))?;
        Ok(Self {
            handle,
            join: Some(join),
        })
    }

    pub fn handle(&self) -> ApplicationHandle {
        self.handle.clone()
    }
}

impl Drop for ApplicationRuntime {
    fn drop(&mut self) {
        let _ = self.handle.sender.send(ApplicationMessage::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

enum ApplicationMessage {
    Intent(AppIntent),
    Shutdown,
}

fn run_actor(
    mut model: ApplicationModel,
    mut backend: Box<dyn Backend>,
    receiver: Receiver<ApplicationMessage>,
    published: Arc<RwLock<Arc<AppSnapshot>>>,
) {
    loop {
        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(ApplicationMessage::Intent(intent)) => model.handle_intent(&mut *backend, intent),
            Ok(ApplicationMessage::Shutdown) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        match backend.poll() {
            Ok(snapshot) => model.apply_backend_snapshot(snapshot),
            Err(error) => model.notify_error(format!("backend poll failed: {error}")),
        }
        model.revision = model.revision.wrapping_add(1);
        *published.write().unwrap_or_else(|error| error.into_inner()) = Arc::new(model.snapshot());
    }
}

struct ApplicationModel {
    revision: u64,
    tracks: Vec<TrackModel>,
    loops: BTreeMap<LoopId, LoopModel>,
    global: shoop_app_api::GlobalControlState,
    status: StatusState,
    notifications: Vec<AppNotification>,
}

struct TrackModel {
    id: TrackId,
    name: String,
    is_sync: bool,
    loops: Vec<LoopId>,
}

struct LoopModel {
    id: LoopId,
    backend_id: BackendLoopId,
    track_id: TrackId,
    name: String,
    state: LoopState,
}

impl ApplicationModel {
    fn initialize(backend: &mut dyn Backend) -> Result<Self> {
        let backend_loop = backend.create_loop()?;
        backend.wait_idle();
        let track_id = TrackId::from_raw(1);
        let loop_id = LoopId::from_raw(1);
        let loop_model = LoopModel {
            id: loop_id,
            backend_id: backend_loop,
            track_id,
            name: "sync loop".to_owned(),
            state: LoopState {
                id: loop_id,
                name: "sync loop".to_owned(),
                sync: true,
                ..Default::default()
            },
        };
        Ok(Self {
            revision: 1,
            tracks: vec![TrackModel {
                id: track_id,
                name: "Sync".to_owned(),
                is_sync: true,
                loops: vec![loop_id],
            }],
            loops: BTreeMap::from([(loop_id, loop_model)]),
            global: Default::default(),
            status: Default::default(),
            notifications: Vec::new(),
        })
    }

    fn handle_intent(&mut self, backend: &mut dyn Backend, intent: AppIntent) {
        let result = match intent {
            AppIntent::Loop {
                track_id,
                loop_id,
                action,
            } => self.handle_loop_action(backend, track_id, loop_id, action),
            AppIntent::Global(action) => self.handle_global_action(backend, action),
            AppIntent::Track { track_id, .. } => Err(format!(
                "track controls are not available for track {track_id} yet"
            )),
            AppIntent::AddTrack(_) => Err("track creation is not available yet".to_owned()),
            AppIntent::AddLoop { track_id } => Err(format!(
                "loop creation is not available for track {track_id} yet"
            )),
        };
        if let Err(error) = result {
            self.notify_error(error);
        }
    }

    fn handle_loop_action(
        &mut self,
        backend: &mut dyn Backend,
        track_id: TrackId,
        loop_id: LoopId,
        action: LoopAction,
    ) -> Result<(), String> {
        let Some(loop_model) = self.loops.get(&loop_id) else {
            return Err(format!("stale or unknown loop {loop_id}"));
        };
        if loop_model.track_id != track_id {
            return Err(format!(
                "loop {loop_id} does not belong to track {track_id}"
            ));
        }
        match action {
            LoopAction::IconClicked(modifiers) => {
                if !modifiers.additive {
                    for model in self.loops.values_mut() {
                        model.state.selected = false;
                    }
                }
                if let Some(model) = self.loops.get_mut(&loop_id) {
                    model.state.selected = if modifiers.additive {
                        !model.state.selected
                    } else {
                        true
                    };
                }
                Ok(())
            }
            LoopAction::IconDoubleClicked => {
                let was_targeted = self
                    .loops
                    .get(&loop_id)
                    .is_some_and(|model| model.state.targeted);
                for model in self.loops.values_mut() {
                    model.state.targeted = false;
                }
                if !was_targeted {
                    if let Some(model) = self.loops.get_mut(&loop_id) {
                        model.state.targeted = true;
                    }
                }
                Ok(())
            }
            LoopAction::PlayClicked => {
                self.transition_targets(backend, loop_id, BackendLoopMode::Playing)
            }
            LoopAction::RecordClicked => {
                self.transition_targets(backend, loop_id, BackendLoopMode::Recording)
            }
            LoopAction::StopClicked => {
                self.transition_targets(backend, loop_id, BackendLoopMode::Stopped)
            }
            LoopAction::GainChanged(_) => Ok(()),
        }
    }

    fn transition_targets(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
        mode: BackendLoopMode,
    ) -> Result<(), String> {
        let initiating_selected = self
            .loops
            .get(&initiating_loop)
            .is_some_and(|model| model.state.selected);
        let targets: Vec<_> = self
            .loops
            .values()
            .filter(|model| {
                model.id == initiating_loop || initiating_selected && model.state.selected
            })
            .map(|model| (model.id, model.backend_id))
            .collect();
        let delay = self.global.sync.then_some(0);
        for (id, backend_id) in targets {
            backend
                .transition_loop(backend_id, mode, delay)
                .map_err(|error| format!("could not transition loop {id}: {error}"))?;
        }
        Ok(())
    }

    fn handle_global_action(
        &mut self,
        backend: &mut dyn Backend,
        action: GlobalControlAction,
    ) -> Result<(), String> {
        match action {
            GlobalControlAction::StopAll => {
                let targets: Vec<_> = self.loops.values().map(|model| model.backend_id).collect();
                for target in targets {
                    backend
                        .transition_loop(
                            target,
                            BackendLoopMode::Stopped,
                            self.global.sync.then_some(0),
                        )
                        .map_err(|error| format!("could not stop loop: {error}"))?;
                }
            }
            GlobalControlAction::DeselectAll => {
                for model in self.loops.values_mut() {
                    model.state.selected = false;
                }
            }
            GlobalControlAction::ClearRecordings { include_sync }
            | GlobalControlAction::ClearAll { include_sync } => {
                let targets: Vec<_> = self
                    .loops
                    .values()
                    .filter(|model| {
                        include_sync
                            || !self
                                .tracks
                                .iter()
                                .find(|track| track.id == model.track_id)
                                .is_some_and(|track| track.is_sync)
                    })
                    .map(|model| model.backend_id)
                    .collect();
                for target in targets {
                    backend
                        .clear_loop(target)
                        .map_err(|error| format!("could not clear loop: {error}"))?;
                }
            }
            GlobalControlAction::SetDefaultRecordingAction(value) => {
                self.global.default_recording_action = value;
            }
            GlobalControlAction::SetPlayAfterRecord(value) => {
                self.global.play_after_record = value;
            }
            GlobalControlAction::SetSync(value) => self.global.sync = value,
            GlobalControlAction::SetSolo(value) => self.global.solo = value,
            GlobalControlAction::SetApplyNCycles(value) => self.global.apply_n_cycles = value,
        }
        Ok(())
    }

    fn apply_backend_snapshot(&mut self, snapshot: BackendSnapshot) {
        self.status.dsp_load_percent = snapshot.status.dsp_load_percent;
        self.status.xruns = self.status.xruns.saturating_add(snapshot.status.xruns);
        self.status.buffer_size = snapshot.status.buffer_size;
        self.status.sample_rate = snapshot.status.sample_rate;
        for model in self.loops.values_mut() {
            let Some(backend_state) = snapshot.loops.get(&model.backend_id) else {
                continue;
            };
            model.state.mode = app_loop_mode(backend_state.mode);
            model.state.next_mode = backend_state
                .next_mode
                .map(app_loop_mode)
                .unwrap_or(model.state.mode);
            model.state.next_transition_delay = backend_state.next_transition_delay;
            model.state.empty = backend_state.length == 0;
            model.state.position = if backend_state.length == 0 {
                0.0
            } else {
                backend_state.position as f32 / backend_state.length as f32
            };
        }
    }

    fn snapshot(&self) -> AppSnapshot {
        AppSnapshot {
            revision: self.revision,
            tracks: self
                .tracks
                .iter()
                .map(|track| TrackState {
                    id: track.id,
                    name: track.name.clone(),
                    is_sync: track.is_sync,
                    loops: track
                        .loops
                        .iter()
                        .filter_map(|id| self.loops.get(id))
                        .map(|model| {
                            let mut state = model.state.clone();
                            state.name.clone_from(&model.name);
                            state
                        })
                        .collect(),
                    controls: Default::default(),
                })
                .collect(),
            global_controls: self.global.clone(),
            status: self.status.clone(),
            details: None,
            notifications: self.notifications.clone(),
        }
    }

    fn notify_error(&mut self, message: String) {
        self.notifications.push(AppNotification {
            level: NotificationLevel::Error,
            message,
        });
        const MAX_NOTIFICATIONS: usize = 32;
        if self.notifications.len() > MAX_NOTIFICATIONS {
            self.notifications
                .drain(..self.notifications.len() - MAX_NOTIFICATIONS);
        }
    }
}

fn app_loop_mode(mode: BackendLoopMode) -> LoopMode {
    match mode {
        BackendLoopMode::Unknown => LoopMode::Unknown,
        BackendLoopMode::Stopped => LoopMode::Stopped,
        BackendLoopMode::Playing => LoopMode::Playing,
        BackendLoopMode::Recording => LoopMode::Recording,
        BackendLoopMode::Replacing => LoopMode::Replacing,
        BackendLoopMode::PlayingDryThroughWet => LoopMode::PlayingDryThroughWet,
        BackendLoopMode::RecordingDryIntoWet => LoopMode::RecordingDryIntoWet,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use shoop_app_api::{SelectionModifiers, TrackAction};
    use shoop_backend::FakeBackend;

    use super::*;

    fn wait_for(
        handle: &ApplicationHandle,
        predicate: impl Fn(&AppSnapshot) -> bool,
    ) -> Arc<AppSnapshot> {
        let started = Instant::now();
        loop {
            let snapshot = handle.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn actor_initializes_a_distinct_sync_track() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let snapshot = runtime.handle().snapshot();
        assert_eq!(snapshot.tracks.len(), 1);
        assert!(snapshot.tracks[0].is_sync);
        assert_eq!(snapshot.tracks[0].loops.len(), 1);
        assert!(snapshot.tracks[0].loops[0].sync);
        assert!(snapshot.tracks[0].id.is_valid());
        assert!(snapshot.tracks[0].loops[0].id.is_valid());
    }

    #[test]
    fn actor_applies_intents_and_publishes_backend_state() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        let snapshot = handle.snapshot();
        let track = &snapshot.tracks[0];
        let loop_state = &track.loops[0];
        handle
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id: loop_state.id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        let updated = wait_for(&handle, |snapshot| {
            snapshot.tracks[0].loops[0].mode == LoopMode::Playing
        });
        assert!(updated.revision > snapshot.revision);
    }

    #[test]
    fn actor_rejects_stale_and_mismatched_ids_observably() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        handle
            .dispatch(AppIntent::Loop {
                track_id: TrackId::from_raw(900),
                loop_id: LoopId::from_raw(901),
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| !snapshot.notifications.is_empty());
        assert!(snapshot.notifications[0]
            .message
            .contains("stale or unknown"));

        handle
            .dispatch(AppIntent::Track {
                track_id: TrackId::from_raw(902),
                action: TrackAction::NameChanged("nope".to_owned()),
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| snapshot.notifications.len() >= 2);
        assert!(snapshot.notifications[1].message.contains("not available"));
    }

    #[test]
    fn snapshot_reads_are_independent_of_actor_progress() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        let held = handle.snapshot();
        handle
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        let updated = wait_for(&handle, |snapshot| !snapshot.global_controls.sync);
        assert!(held.global_controls.sync);
        assert!(!updated.global_controls.sync);
    }
}
