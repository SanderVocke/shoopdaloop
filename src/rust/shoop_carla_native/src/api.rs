use anyhow::Result;
use std::fmt::Debug;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ChainType {
    CarlaRack = 0,
    CarlaPatchbay = 1,
    CarlaPatchbay16x = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarlaProcessorInfo {
    pub chain_type: ChainType,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

pub trait CarlaExternalUi: Send + Sync + Debug {
    fn set_visible(&self, visible: bool) -> Result<()>;
    fn refresh(&self) {}
    fn is_visible(&self) -> bool;
    fn ui_was_closed(&self) -> bool {
        false
    }
}
