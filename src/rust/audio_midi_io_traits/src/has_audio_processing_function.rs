use anyhow::Result;

pub trait HasAudioProcessingFunction {
    fn process(nframes: u32) -> Result<()>;
}
