use crate::dummy_port::DummyPort;
use anyhow::Result;
use audio_midi_io_common::audio_port::AudioPort;
use audio_midi_io_traits::{has_audio_fader::HasAudioFader, port::PortImpl, types::PortDataType};
pub struct DummyAudioPort {
    dummy: DummyPort,
    audio: AudioPort,
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

    fn get_external_connections(
        self: &Self,
    ) -> Result<Vec<audio_midi_io_traits::types::ExternalConnectionStatus>> {
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

    fn driver_handle(
        self: &Self,
    ) -> Result<std::rc::Weak<std::cell::RefCell<dyn audio_midi_io_traits::host::HostImpl>>> {
        todo!()
    }

    fn audio_fader<'a>(
        self: &'a Self,
    ) -> Option<&'a dyn audio_midi_io_traits::has_audio_fader::HasAudioFader> {
        Some(&self.audio as &dyn HasAudioFader)
    }

    fn audio_fader_mut<'a>(
        self: &'a mut Self,
    ) -> Option<&'a mut dyn audio_midi_io_traits::has_audio_fader::HasAudioFader> {
        Some(&mut self.audio as &mut dyn HasAudioFader)
    }

    fn midi_indicators<'a>(
        self: &'a Self,
    ) -> Option<&'a dyn audio_midi_io_traits::has_midi_indicators::HasMidiIndicators> {
        None
    }

    fn midi_indicators_mut<'a>(
        self: &'a mut Self,
    ) -> Option<&'a mut dyn audio_midi_io_traits::has_midi_indicators::HasMidiIndicators> {
        None
    }

    fn mutable<'a>(self: &'a Self) -> Option<&'a dyn audio_midi_io_traits::mutable::Mutable> {
        todo!()
    }

    fn mutable_mut<'a>(
        self: &'a mut Self,
    ) -> Option<&'a mut dyn audio_midi_io_traits::mutable::Mutable> {
        todo!()
    }

    fn has_ringbuffer<'a>(
        self: &'a Self,
    ) -> Option<&'a dyn audio_midi_io_traits::has_ringbuffer::HasRingbuffer> {
        todo!()
    }

    fn has_ringbuffer_mut<'a>(
        self: &'a mut Self,
    ) -> Option<&'a mut dyn audio_midi_io_traits::has_ringbuffer::HasRingbuffer> {
        todo!()
    }
}
