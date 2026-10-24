//! Single-owner render endpoint. Final destruction requires stopped callbacks.

use crate::buffer::AudioBlock;
use crate::{Error, Frame, PreparedConfig};

pub struct RenderRuntime {
    config: PreparedConfig,
    next_frame: Frame,
}

impl RenderRuntime {
    /// Construct outside rendering. The empty graph produces silence.
    pub fn new(config: PreparedConfig) -> Self {
        Self {
            config,
            next_frame: Frame(0),
        }
    }

    pub fn next_frame(&self) -> Frame {
        self.next_frame
    }

    pub fn config(&self) -> PreparedConfig {
        self.config
    }

    pub fn process(&mut self, mut block: AudioBlock<'_, '_>) -> Result<(), Error> {
        if block.frames == 0 {
            return Ok(());
        }
        let limits = self.config.limits();
        block.clear_bounded(limits.audio_outputs, block.frames.min(limits.max_frames));
        if block.frames > limits.max_frames
            || block.inputs.len() != limits.audio_inputs
            || block.outputs.len() != limits.audio_outputs
            || block.inputs.iter().any(|input| input.len() < block.frames)
            || block
                .outputs
                .iter()
                .any(|output| output.len() < block.frames)
        {
            return Err(Error::InvalidShape);
        }
        if block.frame != self.next_frame {
            return Err(Error::InvalidFrame);
        }
        self.next_frame = block.frame.checked_add(block.frames)?;
        Ok(())
    }
}
