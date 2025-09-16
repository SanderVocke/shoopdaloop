use crate::dummy_port::DummyPort;
use audio_midi_io_common::audio_port::AudioPort;
use audio_midi_io_traits::{audio_port::AudioPortImpl, port::PortImpl, types::PortDataType};
use anyhow::Result;
pub struct DummyAudioPort {
    dummy : DummyPort,
    audio : AudioPort,
}

impl PortImpl for DummyAudioPort {
    fn close(self: &mut Self) {
        todo!()
    }

    fn name(self: &Self) -> Result<String> {
        self.dummy.name()
    }

    fn data_type(self: &Self) -> Result<PortDataType> {
        Ok(PortDataType::Audio)
    }

    fn has_internal_read_access(self: &Self) -> Result<bool> {
        todo!()
    }

    fn has_internal_write_access(self: &Self) -> Result<bool> {
        todo!()
    }

    fn has_implicit_input_source(self: &Self) -> Result<bool> {
        todo!()
    }

    fn has_implicit_output_sink(self: &Self) -> Result<bool> {
        todo!()
    }

    fn get_external_connections(self: &Self) -> Result<Vec<audio_midi_io_traits::types::ExternalConnectionStatus>> {
        todo!()
    }

    fn connect_external(self: &mut Self, external_port: String) -> Result<()> {
        todo!()
    }

    fn disconnect_external(self: &mut Self, external_port: String) -> Result<()> {
        todo!()
    }

    fn input_connectability(self: &Self) -> Result<u32> {
        todo!()
    }

    fn output_connectability(self: &Self) -> Result<u32> {
        todo!()
    }

    fn driver_handle(self: &Self) -> Result<std::rc::Weak<std::cell::RefCell<dyn audio_midi_io_traits::driver::DriverImpl>>> {
        todo!()
    }

    fn as_audio<'a>(self: &'a Self) -> Option<&'a dyn audio_midi_io_traits::audio_port::AudioPortImpl> {
        Some(&self.audio as &dyn AudioPortImpl)
    }

    fn as_audio_mut<'a>(self: &'a mut Self) -> Option<&'a mut dyn audio_midi_io_traits::audio_port::AudioPortImpl> {
        Some(&mut self.audio as &mut dyn AudioPortImpl)
    }

    fn as_midi<'a>(self: &'a Self) -> Option<&'a dyn audio_midi_io_traits::midi_port::MidiPortImpl> {
        None
    }

    fn as_midi_mut<'a>(self: &'a mut Self) -> Option<&'a mut dyn audio_midi_io_traits::midi_port::MidiPortImpl> {
        None
    }
}