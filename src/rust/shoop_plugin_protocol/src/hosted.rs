//! Shared processor metadata at the native host boundary.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarlaProcessorInfo<Chain = crate::CarlaChainType> {
    pub chain_type: Chain,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CarlaProcessorLifecycle {
    #[default]
    Stopped,
    Starting,
    Running,
    Degraded,
    Crashed,
    Restarting,
    Unavailable,
}

impl CarlaProcessorLifecycle {
    pub fn is_operational(self) -> bool {
        matches!(self, Self::Running | Self::Degraded)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CarlaGenerationLog {
    pub generation: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_dropped_bytes: u64,
    pub stderr_dropped_bytes: u64,
}
