use crate::carla_processor::{
    CarlaExternalUi, CarlaMidiBuffer, CarlaProcessor, CarlaProcessorInfo,
};
use crate::FXChainType;
use anyhow::{bail, Result};
use shoop_carla_native::api::ChainType;
use std::sync::Arc;

pub use shoop_carla_native::{
    carla_runtime_availability, carla_runtime_path, configure_application_ui_dispatcher,
    smoke_test_carla_runtime, smoke_test_carla_ui, CarlaMainThreadUiDispatcher,
    CarlaMainThreadUiService, CARLA_MAX_BUFFER_SIZE, CARLA_MIDI_BUFFER_CAPACITY,
};

#[cfg(test)]
pub(crate) fn lock_carla_test() -> impl Drop {
    static LOCK: crate::realtime_lock_guard::Mutex<()> = crate::realtime_lock_guard::Mutex::new(());
    LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

fn native_kind(kind: FXChainType) -> Result<ChainType> {
    Ok(match kind {
        FXChainType::CarlaRack => ChainType::CarlaRack,
        FXChainType::CarlaPatchbay => ChainType::CarlaPatchbay,
        FXChainType::CarlaPatchbay16x => ChainType::CarlaPatchbay16x,
        _ => bail!("{kind:?} is not a Carla chain type"),
    })
}

#[derive(Debug)]
pub struct CarlaNativeHost(shoop_carla_native::CarlaNativeHost);
impl CarlaNativeHost {
    pub fn instantiate(kind: FXChainType, rate: u32, frames: u32) -> Result<Self> {
        Ok(Self(shoop_carla_native::CarlaNativeHost::instantiate(
            native_kind(kind)?,
            rate,
            frames,
        )?))
    }
    pub fn instantiate_for_application(kind: FXChainType, rate: u32, frames: u32) -> Result<Self> {
        Ok(Self(
            shoop_carla_native::CarlaNativeHost::instantiate_for_application(
                native_kind(kind)?,
                rate,
                frames,
            )?,
        ))
    }
    pub(crate) fn instantiate_with_ui_dispatcher(
        kind: FXChainType,
        rate: u32,
        frames: u32,
        dispatcher: CarlaMainThreadUiDispatcher,
    ) -> Result<Self> {
        Ok(Self(
            shoop_carla_native::CarlaNativeHost::instantiate_with_ui_dispatcher(
                native_kind(kind)?,
                rate,
                frames,
                dispatcher,
            )?,
        ))
    }
}

#[derive(Debug)]
struct Ui(Arc<dyn shoop_carla_native::api::CarlaExternalUi>);
impl CarlaExternalUi for Ui {
    fn set_visible(&self, visible: bool) -> Result<()> {
        self.0.set_visible(visible)
    }
    fn refresh(&self) {
        self.0.refresh();
    }
    fn is_visible(&self) -> bool {
        self.0.is_visible()
    }
    fn ui_was_closed(&self) -> bool {
        self.0.ui_was_closed()
    }
}
impl CarlaProcessor for CarlaNativeHost {
    fn info(&self) -> CarlaProcessorInfo {
        let i = self.0.info();
        let chain_type = match i.chain_type {
            ChainType::CarlaRack => FXChainType::CarlaRack,
            ChainType::CarlaPatchbay => FXChainType::CarlaPatchbay,
            ChainType::CarlaPatchbay16x => FXChainType::CarlaPatchbay16x,
        };
        CarlaProcessorInfo {
            chain_type,
            audio_inputs: i.audio_inputs,
            audio_outputs: i.audio_outputs,
            midi_inputs: i.midi_inputs,
            midi_outputs: i.midi_outputs,
        }
    }
    fn external_ui(&self) -> Option<Arc<dyn CarlaExternalUi>> {
        self.0
            .external_ui()
            .map(|ui| Arc::new(Ui(ui)) as Arc<dyn CarlaExternalUi>)
    }
    fn idle(&mut self) {
        self.0.idle();
    }
    fn set_active(&mut self, active: bool) {
        self.0.set_active(active);
    }
    fn is_active(&self) -> bool {
        self.0.is_active()
    }
    fn set_visible(&mut self, visible: bool) -> Result<()> {
        self.0.set_visible(visible)
    }
    fn is_visible(&mut self) -> bool {
        self.0.is_visible()
    }
    fn save_state(&mut self) -> Result<String> {
        self.0.save_state()
    }
    fn restore_state(&mut self, state: &str) -> Result<()> {
        self.0.restore_state(state)
    }
    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        self.0.audio_input_mut(index)
    }
    fn audio_output(&self, index: usize) -> Option<&[f32]> {
        self.0.audio_output(index)
    }
    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
        self.0.set_midi_input_events(index, events)
    }
    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        self.0.midi_output_events(index)
    }
    fn fill_midi_output_events(
        &mut self,
        index: usize,
        destination: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        destination.clear();
        self.0
            .visit_midi_output(index, |frame, data| destination.push(frame, data))
    }
    fn process(&mut self, frames: usize) -> Result<()> {
        self.0.process(frames)
    }
}
