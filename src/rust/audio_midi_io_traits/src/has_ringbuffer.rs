use anyhow::Result;

pub trait HasRingbuffer {
    fn set_n_ringbuffer_samples(self: &mut Self, n_samples: u32) -> Result<()>;
    fn get_n_ringbuffer_samples(self: &Self) -> Result<u32>;
}
