use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use shoop_engine::app_backend::{
    AudioDriver, AudioDriverSettings, BackendSession, DummyAudioDriverSettings, Loop as EngineLoop,
};
use shoop_engine::{AudioDriverType, LoopMode};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendLoopId(u64);

impl BackendLoopId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackendStatus {
    pub dsp_load_percent: f32,
    pub xruns: u32,
    pub buffer_size: u32,
    pub sample_rate: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendLoopMode {
    #[default]
    Unknown,
    Stopped,
    Playing,
    Recording,
    Replacing,
    PlayingDryThroughWet,
    RecordingDryIntoWet,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendLoopState {
    pub mode: BackendLoopMode,
    pub length: u32,
    pub position: u32,
    pub next_mode: Option<BackendLoopMode>,
    pub next_transition_delay: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendSnapshot {
    pub status: BackendStatus,
    pub loops: BTreeMap<BackendLoopId, BackendLoopState>,
}

pub trait Backend: Send {
    fn create_loop(&mut self) -> Result<BackendLoopId>;
    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()>;
    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()>;
    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()>;
    fn poll(&mut self) -> Result<BackendSnapshot>;
    fn wait_idle(&mut self);
}

pub struct EngineBackend {
    session: BackendSession,
    driver: AudioDriver,
    loops: BTreeMap<BackendLoopId, EngineLoop>,
    next_loop_id: u64,
}

impl EngineBackend {
    pub fn new_dummy(sample_rate: u32, buffer_size: u32) -> Result<Self> {
        let driver = AudioDriver::new(AudioDriverType::Dummy, None)?;
        driver.start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
            client_name: "ShoopDaLoop-egui".to_owned(),
            sample_rate,
            buffer_size,
        }))?;
        let session = BackendSession::new()?;
        session.set_audio_driver(&driver)?;
        driver.wait_process();
        Ok(Self {
            session,
            driver,
            loops: BTreeMap::new(),
            next_loop_id: 1,
        })
    }

    fn engine_loop(&self, id: BackendLoopId) -> Result<&EngineLoop> {
        self.loops
            .get(&id)
            .ok_or_else(|| anyhow!("unknown backend loop {id:?}"))
    }
}

impl Backend for EngineBackend {
    fn create_loop(&mut self) -> Result<BackendLoopId> {
        let engine_loop = self.session.create_loop()?;
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(id, engine_loop);
        Ok(id)
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        let target = self.engine_loop(loop_id)?;
        let source = source.map(|id| self.engine_loop(id)).transpose()?;
        target.set_sync_source(source)?;
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        let delay = cycles_delay
            .map(|delay| i32::try_from(delay).unwrap_or(i32::MAX))
            .unwrap_or(-1);
        self.engine_loop(loop_id)?
            .transition(to_engine_mode(mode), delay, -1)?;
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.engine_loop(loop_id)?.clear(0)?;
        Ok(())
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        let driver = self.driver.get_state();
        let mut loops = BTreeMap::new();
        for (id, engine_loop) in &self.loops {
            if let Some(state) = engine_loop.poll_state() {
                loops.insert(
                    *id,
                    BackendLoopState {
                        mode: from_engine_mode(state.mode),
                        length: state.length,
                        position: state.position,
                        next_mode: state.maybe_next_mode.map(from_engine_mode),
                        next_transition_delay: state.maybe_next_mode_delay,
                    },
                );
            }
        }
        Ok(BackendSnapshot {
            status: BackendStatus {
                dsp_load_percent: driver.dsp_load_percent,
                xruns: driver.xruns_since_last,
                buffer_size: driver.buffer_size,
                sample_rate: driver.sample_rate,
            },
            loops,
        })
    }

    fn wait_idle(&mut self) {
        self.driver.wait_process();
    }
}

fn from_engine_mode(mode: LoopMode) -> BackendLoopMode {
    match mode {
        LoopMode::Unknown => BackendLoopMode::Unknown,
        LoopMode::Stopped => BackendLoopMode::Stopped,
        LoopMode::Playing => BackendLoopMode::Playing,
        LoopMode::Recording => BackendLoopMode::Recording,
        LoopMode::Replacing => BackendLoopMode::Replacing,
        LoopMode::PlayingDryThroughWet => BackendLoopMode::PlayingDryThroughWet,
        LoopMode::RecordingDryIntoWet => BackendLoopMode::RecordingDryIntoWet,
    }
}

fn to_engine_mode(mode: BackendLoopMode) -> LoopMode {
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

#[derive(Debug)]
pub struct FakeBackend {
    status: BackendStatus,
    loops: BTreeMap<BackendLoopId, BackendLoopState>,
    sync_sources: BTreeMap<BackendLoopId, Option<BackendLoopId>>,
    next_loop_id: u64,
    operations: Vec<FakeOperation>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FakeOperation {
    CreateLoop(BackendLoopId),
    SetSyncSource(BackendLoopId, Option<BackendLoopId>),
    Transition(BackendLoopId, BackendLoopMode, Option<u32>),
    Clear(BackendLoopId),
}

impl Default for FakeBackend {
    fn default() -> Self {
        Self {
            status: BackendStatus {
                buffer_size: 256,
                sample_rate: 48_000,
                ..Default::default()
            },
            loops: BTreeMap::new(),
            sync_sources: BTreeMap::new(),
            next_loop_id: 1,
            operations: Vec::new(),
        }
    }
}

impl FakeBackend {
    pub fn operations(&self) -> &[FakeOperation] {
        &self.operations
    }

    fn require_loop(&self, id: BackendLoopId) -> Result<()> {
        self.loops
            .contains_key(&id)
            .then_some(())
            .ok_or_else(|| anyhow!("unknown fake loop {id:?}"))
    }
}

impl Backend for FakeBackend {
    fn create_loop(&mut self) -> Result<BackendLoopId> {
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(
            id,
            BackendLoopState {
                mode: BackendLoopMode::Stopped,
                ..Default::default()
            },
        );
        self.sync_sources.insert(id, None);
        self.operations.push(FakeOperation::CreateLoop(id));
        Ok(id)
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        self.require_loop(loop_id)?;
        if let Some(source) = source {
            self.require_loop(source)?;
        }
        self.sync_sources.insert(loop_id, source);
        self.operations
            .push(FakeOperation::SetSyncSource(loop_id, source));
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        state.mode = mode;
        state.next_mode = None;
        state.next_transition_delay = None;
        self.operations
            .push(FakeOperation::Transition(loop_id, mode, cycles_delay));
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        *state = BackendLoopState {
            mode: BackendLoopMode::Stopped,
            ..Default::default()
        };
        self.operations.push(FakeOperation::Clear(loop_id));
        Ok(())
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        Ok(BackendSnapshot {
            status: self.status,
            loops: self.loops.clone(),
        })
    }

    fn wait_idle(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend_contract(backend: &mut dyn Backend) {
        let sync = backend.create_loop().unwrap();
        let follower = backend.create_loop().unwrap();
        backend.wait_idle();
        backend
            .transition_loop(follower, BackendLoopMode::Playing, None)
            .unwrap();
        let snapshot = backend.poll().unwrap();
        assert!(snapshot.loops.contains_key(&sync));
        assert_eq!(
            snapshot.loops.get(&follower).unwrap().mode,
            BackendLoopMode::Playing
        );
        backend.set_loop_sync_source(follower, Some(sync)).unwrap();
        backend.wait_idle();
    }

    #[test]
    fn fake_backend_satisfies_basic_contract() {
        backend_contract(&mut FakeBackend::default());
    }

    #[test]
    fn engine_dummy_backend_satisfies_basic_contract() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        backend_contract(&mut backend);
    }
}
