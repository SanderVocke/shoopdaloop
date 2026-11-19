//! Legacy engine adapter for the shared Carla Native host.

use crate::carla_processor::{
    CarlaExternalUi, CarlaMidiBuffer, CarlaProcessor, CarlaProcessorInfo,
};
use crate::FXChainType;
use anyhow::Result;
use shoop_carla::native::{CarlaNativeHost as NativeHost, CarlaNativeUiHandle};
use std::sync::Arc;

pub use shoop_carla::native::{
    carla_runtime_availability, carla_runtime_path, configure_application_ui_dispatcher,
    smoke_test_carla_runtime, smoke_test_carla_ui, CarlaMainThreadUiDispatcher,
    CarlaMainThreadUiService, CARLA_MAX_BUFFER_SIZE, CARLA_MIDI_BUFFER_CAPACITY,
};

#[cfg(test)]
use crate::realtime_lock_guard::Mutex;

#[cfg(test)]
static CARLA_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn lock_carla_test() -> impl Drop {
    CARLA_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug)]
pub struct CarlaNativeHost {
    host: NativeHost,
    chain_type: FXChainType,
}

impl CarlaNativeHost {
    pub fn instantiate(chain_type: FXChainType, rate: u32, frames: u32) -> Result<Self> {
        let host = NativeHost::instantiate(chain_type.try_into()?, rate, frames)?;
        Ok(Self { host, chain_type })
    }

    pub fn instantiate_for_application(
        chain_type: FXChainType,
        rate: u32,
        frames: u32,
    ) -> Result<Self> {
        let host = NativeHost::instantiate_for_application(chain_type.try_into()?, rate, frames)?;
        Ok(Self { host, chain_type })
    }

    pub(crate) fn instantiate_with_ui_dispatcher(
        chain_type: FXChainType,
        rate: u32,
        frames: u32,
        dispatcher: CarlaMainThreadUiDispatcher,
    ) -> Result<Self> {
        let host = NativeHost::instantiate_with_ui_dispatcher(
            chain_type.try_into()?,
            rate,
            frames,
            dispatcher,
        )?;
        Ok(Self { host, chain_type })
    }
}

impl CarlaExternalUi for CarlaNativeUiHandle {
    fn set_visible(&self, visible: bool) -> Result<()> {
        self.set_visible(visible)
    }

    fn is_visible(&self) -> bool {
        self.is_visible()
    }
}

impl CarlaProcessor for CarlaNativeHost {
    fn info(&self) -> CarlaProcessorInfo {
        let channels = usize::from(self.host.chain_type().audio_channels());
        CarlaProcessorInfo {
            chain_type: self.chain_type,
            audio_inputs: channels,
            audio_outputs: channels,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }

    fn external_ui(&self) -> Option<Arc<dyn CarlaExternalUi>> {
        self.host
            .external_ui()
            .map(|ui| Arc::new(ui) as Arc<dyn CarlaExternalUi>)
    }

    fn idle(&mut self) {
        self.host.idle();
    }

    fn set_active(&mut self, active: bool) {
        self.host.set_active(active);
    }

    fn is_active(&self) -> bool {
        self.host.is_active()
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        self.host.set_visible(visible)
    }

    fn is_visible(&mut self) -> bool {
        self.host.is_visible()
    }

    fn save_state(&mut self) -> Result<String> {
        self.host.save_state()
    }

    fn restore_state(&mut self, state: &str) -> Result<()> {
        self.host.restore_state(state)
    }

    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        self.host.audio_input_mut(index)
    }

    fn audio_output(&self, index: usize) -> Option<&[f32]> {
        self.host.audio_output(index)
    }

    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
        self.host.set_midi_input_events(index, events)
    }

    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        let mut events = Vec::new();
        self.host.visit_midi_output(index, |frame, bytes| {
            events.push((frame, bytes.to_vec()));
            Ok(())
        })?;
        Ok(events)
    }

    fn fill_midi_output_events(
        &mut self,
        index: usize,
        destination: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        destination.clear();
        self.host
            .visit_midi_output(index, |frame, bytes| destination.push(frame, bytes))
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        self.host.process(frames)
    }
}

#[cfg(all(test, feature = "carla-system-tests"))]
mod tests {
    use super::*;
    use shoop_carla::state::encode_state;
    use shoop_plugin_protocol::CarlaChainType;

    #[shoop_wasm_test_support::shoop_test]
    fn system_epiano_generates_audio_from_midi_when_opted_in() {
        if std::env::var_os("SHOOP_TEST_CARLA_EPIANO").is_none() {
            eprintln!("skipping system ePiano regression; set SHOOP_TEST_CARLA_EPIANO=1");
            return;
        }
        let _exclusive = lock_carla_test();
        let project = br#"<?xml version='1.0' encoding='UTF-8'?>
<!DOCTYPE CARLA-PROJECT>
<CARLA-PROJECT VERSION='2.5'>
 <EngineSettings>
  <ForceStereo>true</ForceStereo>
  <PreferPluginBridges>false</PreferPluginBridges>
  <PreferUiBridges>false</PreferUiBridges>
 </EngineSettings>
 <Plugin>
  <Info>
   <Type>LV2</Type>
   <Name>MDA ePiano</Name>
   <URI>http://drobilla.net/plugins/mda/EPiano</URI>
  </Info>
  <Data>
   <Active>Yes</Active>
   <ControlChannel>1</ControlChannel>
   <Options>0x0</Options>
  </Data>
 </Plugin>
</CARLA-PROJECT>"#;
        let state = encode_state(CarlaChainType::Rack, project).unwrap();
        let (mut ui_service, ui_dispatcher) = CarlaMainThreadUiService::new();
        let mut host = CarlaNativeHost::instantiate_with_ui_dispatcher(
            FXChainType::CarlaRack,
            48_000,
            64,
            ui_dispatcher,
        )
        .unwrap();
        host.restore_state(&state).unwrap();
        let (control, mut endpoint) =
            crate::carla_processor::spawn_processor_bridge(Box::new(host), 48_000, 64).unwrap();
        control.set_active(true);
        control.set_visible(true).unwrap();
        let mut heard = false;
        for block in 0..200 {
            let events: &[(u32, &[u8])] = if block == 0 {
                &[(0, &[0x90, 60, 100])]
            } else {
                &[]
            };
            endpoint.set_midi_input_events(0, events).unwrap();
            endpoint.process(64).unwrap();
            ui_service.pump();
            if endpoint.audio_output(0).unwrap()[..64]
                .iter()
                .any(|sample| sample.abs() > 0.001)
            {
                heard = true;
                break;
            }
        }
        assert!(heard, "MDA ePiano produced no audio from MIDI");
        assert!(control.lifecycle().is_operational());
        assert!(control.is_visible());
        control.set_visible(false).unwrap();
        drop(endpoint);
        drop(control);
        ui_service.pump();
    }
}
