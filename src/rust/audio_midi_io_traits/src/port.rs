use anyhow::Result;
use std::{cell::RefCell, rc::Weak};

use crate::{
    audio_port::AudioPortImpl,
    driver::DriverImpl,
    midi_port::MidiPortImpl,
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
    fn driver_handle(self: &Self) -> Result<Weak<RefCell<dyn DriverImpl>>>;
    fn as_audio<'a>(self: &'a Self) -> Option<&'a dyn AudioPortImpl>;
    fn as_audio_mut<'a>(self: &'a mut Self) -> Option<&'a mut dyn AudioPortImpl>;
    fn as_midi<'a>(self: &'a Self) -> Option<&'a dyn MidiPortImpl>;
    fn as_midi_mut<'a>(self: &'a mut Self) -> Option<&'a mut dyn MidiPortImpl>;
}
