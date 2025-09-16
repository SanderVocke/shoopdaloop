use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

use crate::port::PortImpl;
use crate::types::ExternalPortDescriptor;

pub trait HostImpl {
    fn start(&mut self) -> Result<()>;
    fn close(&mut self) -> Result<()>;
    fn open_audio_port(&mut self) -> Result<Rc<RefCell<dyn PortImpl>>>;
    fn open_midi_port(&mut self) -> Result<Rc<RefCell<dyn PortImpl>>>;
    fn open_decoupled_midi_port(&mut self) -> Result<Rc<RefCell<dyn PortImpl>>>;
    fn get_xruns(&self) -> Result<u32>;
    fn get_sample_rate(&self) -> Result<u32>;
    fn get_buffer_size(&self) -> Result<u32>;
    fn get_dsp_load(&self) -> Result<f32>;
    fn get_client_name(&self) -> Result<String>;
    fn get_active(&self) -> Result<bool>;
    fn get_last_processed(&self) -> Result<u32>;
    fn wait_process(&self) -> Result<()>;
    fn find_external_ports(&self) -> Result<Vec<ExternalPortDescriptor>>;
}
