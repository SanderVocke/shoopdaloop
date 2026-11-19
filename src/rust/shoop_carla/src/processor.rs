//! Processor control and block access on the native worker owner.

use crate::native::{CarlaNativeHost, CarlaNativeUiHandle};
use anyhow::Result;
use std::{fmt::Debug, sync::Arc};

use shoop_plugin_protocol::PreparedMidiBuffer as CarlaMidiBuffer;
pub use shoop_plugin_protocol::{CarlaGenerationLog, CarlaProcessorInfo, CarlaProcessorLifecycle};

pub trait CarlaExternalUi: Send + Sync + Debug {
    fn set_visible(&self, visible: bool) -> Result<()>;
    fn refresh(&self) {}
    fn is_visible(&self) -> bool;
    fn ui_was_closed(&self) -> bool {
        false
    }
}

pub trait CarlaProcessor: Send + Debug {
    fn info(&self) -> CarlaProcessorInfo;
    fn external_ui(&self) -> Option<Arc<dyn CarlaExternalUi>> {
        None
    }
    fn is_ready(&mut self) -> bool {
        true
    }
    fn lifecycle(&self) -> CarlaProcessorLifecycle {
        CarlaProcessorLifecycle::Running
    }
    fn generation(&self) -> u64 {
        0
    }
    fn exit_kind(&self) -> shoop_plugin_protocol::WorkerExitKind {
        shoop_plugin_protocol::WorkerExitKind::None
    }
    fn crash_summary(&self) -> Option<String> {
        None
    }
    fn deadline_misses(&self) -> u64 {
        0
    }
    fn stale_completions(&self) -> u64 {
        0
    }
    fn recoveries(&self) -> u64 {
        0
    }
    fn generation_logs(&self) -> Vec<CarlaGenerationLog> {
        Vec::new()
    }
    fn clear_logs(&mut self) {}
    fn idle(&mut self) {}
    fn toggle_or_recover(&mut self) -> Result<()> {
        let visible = self.is_visible();
        self.set_visible(!visible)
    }
    fn recover(&mut self, _show_editor: bool) -> Result<()> {
        anyhow::bail!("this Carla processor has no child to restart")
    }
    fn set_active(&mut self, active: bool);
    fn is_active(&self) -> bool;
    fn set_visible(&mut self, visible: bool) -> Result<()>;
    fn is_visible(&mut self) -> bool;
    fn save_state(&mut self) -> Result<String>;
    fn restore_state(&mut self, state: &str) -> Result<()>;
    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]>;
    fn audio_output(&self, index: usize) -> Option<&[f32]>;
    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()>;
    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>>;
    fn fill_midi_output_events(
        &mut self,
        index: usize,
        destination: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        destination.clear();
        for (frame_offset, data) in self.midi_output_events(index)? {
            destination.push(frame_offset, &data)?;
        }
        Ok(())
    }
    fn process(&mut self, frames: usize) -> Result<()>;
}

impl CarlaExternalUi for CarlaNativeUiHandle {
    fn set_visible(&self, visible: bool) -> Result<()> {
        CarlaNativeUiHandle::set_visible(self, visible)
    }

    fn is_visible(&self) -> bool {
        CarlaNativeUiHandle::is_visible(self)
    }
}

impl CarlaProcessor for CarlaNativeHost {
    fn info(&self) -> CarlaProcessorInfo {
        let chain_type = self.chain_type();
        let channels = usize::from(chain_type.audio_channels());
        CarlaProcessorInfo {
            chain_type,
            audio_inputs: channels,
            audio_outputs: channels,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }

    fn external_ui(&self) -> Option<Arc<dyn CarlaExternalUi>> {
        self.external_ui()
            .map(|ui| Arc::new(ui) as Arc<dyn CarlaExternalUi>)
    }

    fn set_active(&mut self, active: bool) {
        CarlaNativeHost::set_active(self, active)
    }

    fn is_active(&self) -> bool {
        CarlaNativeHost::is_active(self)
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        CarlaNativeHost::set_visible(self, visible)
    }

    fn is_visible(&mut self) -> bool {
        CarlaNativeHost::is_visible(self)
    }

    fn save_state(&mut self) -> Result<String> {
        CarlaNativeHost::save_state(self)
    }

    fn restore_state(&mut self, state: &str) -> Result<()> {
        CarlaNativeHost::restore_state(self, state)
    }

    fn audio_input_mut(&mut self, channel: usize) -> Option<&mut [f32]> {
        CarlaNativeHost::audio_input_mut(self, channel)
    }

    fn audio_output(&self, channel: usize) -> Option<&[f32]> {
        CarlaNativeHost::audio_output(self, channel)
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        CarlaNativeHost::process(self, frames)
    }

    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
        CarlaNativeHost::set_midi_input_events(self, index, events)
    }

    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        let mut events = Vec::new();
        self.visit_midi_output(index, |offset, bytes| {
            events.push((offset, bytes.to_vec()));
            Ok(())
        })?;
        Ok(events)
    }

    fn fill_midi_output_events(
        &mut self,
        index: usize,
        output: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        output.clear();
        self.visit_midi_output(index, |offset, bytes| Ok(output.push(offset, bytes)?))
    }
}
