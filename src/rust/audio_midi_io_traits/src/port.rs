use anyhow::Result;
use std::{cell::RefCell, rc::Weak};

use crate::{
    has_audio_fader::HasAudioFader,
    has_midi_indicators::HasMidiIndicators,
    has_ringbuffer::HasRingbuffer,
    host::HostImpl,
    mutable::Mutable,
    types::{ExternalConnectionStatus, PortDataType},
};

pub trait PortImpl {
    fn close(self: &mut Self);
    fn name(self: &Self) -> Result<String>;
    fn data_type(self: &Self) -> Result<PortDataType>;
    fn has_internal_read_access(self: &Self) -> Result<bool>;
    fn has_internal_write_access(self: &Self) -> Result<bool>;
    fn has_implicit_input_source(self: &Self) -> Result<bool>;
    fn has_implicit_output_sink(self: &Self) -> Result<bool>;
    fn get_external_connections(self: &Self) -> Result<Vec<ExternalConnectionStatus>>;
    fn connect_external(self: &mut Self, external_port: String) -> Result<()>;
    fn disconnect_external(self: &mut Self, external_port: String) -> Result<()>;
    fn input_connectability(self: &Self) -> Result<u32>;
    fn output_connectability(self: &Self) -> Result<u32>;
    fn driver_handle(self: &Self) -> Result<Weak<RefCell<dyn HostImpl>>>;
    fn audio_fader<'a>(self: &'a Self) -> Option<&'a dyn HasAudioFader>;
    fn audio_fader_mut<'a>(self: &'a mut Self) -> Option<&'a mut dyn HasAudioFader>;
    fn midi_indicators<'a>(self: &'a Self) -> Option<&'a dyn HasMidiIndicators>;
    fn midi_indicators_mut<'a>(self: &'a mut Self) -> Option<&'a mut dyn HasMidiIndicators>;
    fn mutable<'a>(self: &'a Self) -> Option<&'a dyn Mutable>;
    fn mutable_mut<'a>(self: &'a mut Self) -> Option<&'a mut dyn Mutable>;
    fn has_ringbuffer<'a>(self: &'a Self) -> Option<&'a dyn HasRingbuffer>;
    fn has_ringbuffer_mut<'a>(self: &'a mut Self) -> Option<&'a mut dyn HasRingbuffer>;
}
