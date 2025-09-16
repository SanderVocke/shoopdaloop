use anyhow::Result;

pub trait HasAudioFader {
    fn set_gain(self: &mut Self, gain: f32) -> Result<()>;
    fn get_gain(self: &Self) -> Result<f32>;
    fn reset_input_peak(self: &mut Self) -> Result<()>;
    fn get_input_peak(self: &Self) -> Result<f32>;
    fn reset_output_peak(self: &mut Self) -> Result<()>;
    fn get_output_peak(self: &Self) -> Result<f32>;
}
