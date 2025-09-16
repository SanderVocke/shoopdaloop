use anyhow::Result;

pub trait AudioPortImpl {
    fn set_gain(self: &mut Self, gain: f32) -> Result<()>;
    fn get_gain(self: &Self) -> Result<f32>;
    fn reset_input_peak(self: &mut Self) -> Result<()>;
    fn get_input_peak(self: &Self) -> Result<f32>;
    fn reset_output_peak(self: &mut Self) -> Result<()>;
    fn get_output_peak(self: &Self) -> Result<f32>;
    fn set_muted(self: &mut Self, muted: bool) -> Result<()>;
    fn get_muted(self: &Self) -> Result<bool>;
    fn set_n_ringbuffer_samples(self: &mut Self, n_samples: u32) -> Result<()>;
    fn get_n_ringbuffer_samples(self: &Self) -> Result<u32>;
}
