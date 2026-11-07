//! Conversion adapters for session documents.
use crate::{ConversionTarget, SessionError};
pub use shoop_format_conversion::{FrameScale, LoopConversion, LoopTiming, MediaTiming};
mod media;
mod session;
pub use media::{convert_audio, convert_midi, convert_midi_with_cancel, AudioConversion};
pub use session::SessionFormatPlan;

fn invalid(message: &str) -> SessionError {
    SessionError::Validation(message.to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversionPlan {
    pub target: ConversionTarget,
    pub loops: Vec<LoopConversion>,
    pub requires_confirmation: bool,
}
impl ConversionPlan {
    pub fn new(target: ConversionTarget, loops: &[LoopTiming]) -> Result<Self, SessionError> {
        let plan = shoop_format_conversion::ConversionPlan::new(target, loops)?;
        Ok(Self {
            target: plan.target,
            loops: plan.loops,
            requires_confirmation: plan.requires_confirmation,
        })
    }
    pub fn get(&self, id: u64) -> Option<&LoopConversion> {
        self.loops.iter().find(|loop_| loop_.id == id)
    }
}
