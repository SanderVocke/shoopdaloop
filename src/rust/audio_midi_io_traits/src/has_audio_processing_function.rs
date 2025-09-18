use anyhow::Result;

pub trait HasAudioProcessingFunction {
    fn process(&mut self, nframes: usize) -> Result<()>;
}
