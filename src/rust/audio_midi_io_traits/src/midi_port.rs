use anyhow::Result;

pub trait MidiPortImpl {
    fn reset_n_input_events(self: &mut Self) -> Result<()>;
    fn get_n_input_events(self: &Self) -> Result<u32>;
    fn reset_n_output_events(self: &mut Self) -> Result<()>;
    fn get_n_output_events(self: &Self) -> Result<u32>;
    fn get_n_input_notes_active(self: &Self) -> Result<u32>;
    fn get_n_output_notes_active(self: &Self) -> Result<u32>;
}
