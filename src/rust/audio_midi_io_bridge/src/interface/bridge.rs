#[cxx::bridge(namespace = "cxx_audio_midi_io")]
pub mod ffi {
    pub enum PortDataType {
        Audio,
        Midi,
    }

    pub enum PortDirection {
        Input,
        Output,
    }

    pub struct ExternalConnectionStatus {
        external_port: String,
        connected: bool,
    }

    pub struct ExternalPortDescriptor {
        name: String,
        direction: PortDirection,
        data_type: PortDataType,
    }

    extern "Rust" {
        pub type DriverHandle;

        fn start(self: &mut DriverHandle) -> Result<()>;
        fn close(self: &mut DriverHandle) -> Result<()>;
        fn open_audio_port(self: &mut DriverHandle) -> Result<Box<PortHandle>>;
        fn open_midi_port(self: &mut DriverHandle) -> Result<Box<PortHandle>>;
        fn open_decoupled_midi_port(
            self: &mut DriverHandle,
        ) -> Result<Box<DecoupledMidiPortHandle>>;
        fn get_xruns(self: &DriverHandle) -> Result<u32>;
        fn get_sample_rate(self: &DriverHandle) -> Result<u32>;
        fn get_buffer_size(self: &DriverHandle) -> Result<u32>;
        fn get_dsp_load(self: &DriverHandle) -> Result<f32>;
        fn get_client_name(self: &DriverHandle) -> Result<String>;
        fn get_active(self: &DriverHandle) -> Result<bool>;
        fn get_last_processed(self: &DriverHandle) -> Result<u32>;
        fn wait_process(self: &DriverHandle) -> Result<()>;
        fn find_external_ports(self: &DriverHandle) -> Result<Vec<ExternalPortDescriptor>>;
    }

    extern "Rust" {
        pub type DecoupledMidiPortHandle;

        fn unregister(self: &mut DecoupledMidiPortHandle) -> Result<()>;
    }

    extern "Rust" {
        pub type PortHandle;

        fn close(self: &mut PortHandle) -> Result<()>;
        fn name(self: &PortHandle) -> Result<String>;
        fn data_type(self: &PortHandle) -> Result<PortDataType>;
        unsafe fn maybe_driver_handle(self: &PortHandle) -> Result<Box<DriverHandle>>;
        fn has_internal_read_access(self: &PortHandle) -> Result<bool>;
        fn has_internal_write_access(self: &PortHandle) -> Result<bool>;
        fn has_implicit_input_source(self: &PortHandle) -> Result<bool>;
        fn has_implicit_output_sink(self: &PortHandle) -> Result<bool>;
        fn get_external_connections(self: &PortHandle) -> Result<Vec<ExternalConnectionStatus>>;
        fn connect_external(self: &mut PortHandle, external_port: String) -> Result<()>;
        fn disconnect_external(self: &mut PortHandle, external_port: String) -> Result<()>;
        fn input_connectability(self: &PortHandle) -> Result<u32>;
        fn output_connectability(self: &PortHandle) -> Result<u32>;
        fn get_muted(self: &PortHandle) -> Result<bool>;
        fn set_muted(self: &mut PortHandle, muted: bool) -> Result<()>;
        fn set_n_ringbuffer_samples(self: &mut PortHandle, n_samples: u32) -> Result<()>;
        fn get_n_ringbuffer_samples(self: &PortHandle) -> Result<u32>;
    }

    extern "Rust" {
        pub type AudioPortHandle;

        fn set_gain(self: &mut AudioPortHandle, gain: f32) -> Result<()>;
        fn get_gain(self: &AudioPortHandle) -> Result<f32>;
        fn reset_input_peak(self: &mut AudioPortHandle) -> Result<()>;
        fn get_input_peak(self: &AudioPortHandle) -> Result<f32>;
        fn reset_output_peak(self: &mut AudioPortHandle) -> Result<()>;
        fn get_output_peak(self: &AudioPortHandle) -> Result<f32>;
    }

    extern "Rust" {
        pub type MidiPortHandle;

        fn reset_n_input_events(self: &mut MidiPortHandle) -> Result<()>;
        fn get_n_input_events(self: &MidiPortHandle) -> Result<u32>;
        fn reset_n_output_events(self: &mut MidiPortHandle) -> Result<()>;
        fn get_n_output_events(self: &MidiPortHandle) -> Result<u32>;

        fn get_n_input_notes_active(self: &MidiPortHandle) -> Result<u32>;
        fn get_n_output_notes_active(self: &MidiPortHandle) -> Result<u32>;
    }
}

use audio_midi_io_traits::types::ExternalConnectionStatus as RustExternalConnectionStatus;
use audio_midi_io_traits::types::ExternalPortDescriptor as RustExternalPortDescriptor;
use audio_midi_io_traits::types::PortDataType as RustPortDataType;
use audio_midi_io_traits::{driver::DriverImpl, port::PortImpl};
use common::logging::macros::*;
use std::{cell::RefCell, rc::Rc};
shoop_log_unit!("Frontend.AudioMidiIOBridge");

struct DriverHandle {
    driver: Rc<RefCell<dyn DriverImpl>>,
}

struct PortHandle {
    port: Rc<RefCell<dyn PortImpl>>,
}

struct MidiPortHandle {
    port: Rc<RefCell<dyn PortImpl>>,
}

struct AudioPortHandle {
    port: Rc<RefCell<dyn PortImpl>>,
}

struct DecoupledMidiPortHandle {}

impl ffi::PortDataType {
    fn from_rust(rust_port_data_type: RustPortDataType) -> ffi::PortDataType {
        match rust_port_data_type {
            RustPortDataType::Audio => ffi::PortDataType::Audio,
            RustPortDataType::Midi => ffi::PortDataType::Midi,
        }
    }
}

impl ffi::ExternalConnectionStatus {
    fn from_rust(rust_status: RustExternalConnectionStatus) -> ffi::ExternalConnectionStatus {
        ffi::ExternalConnectionStatus {
            external_port: rust_status.external_port,
            connected: rust_status.connected,
        }
    }
}

impl ffi::ExternalPortDescriptor {
    fn from_rust(rust_port: RustExternalPortDescriptor) -> ffi::ExternalPortDescriptor {
        ffi::ExternalPortDescriptor {
            name: rust_port.name,
            direction: match rust_port.direction {
                audio_midi_io_traits::types::PortDirection::Input => ffi::PortDirection::Input,
                audio_midi_io_traits::types::PortDirection::Output => ffi::PortDirection::Output,
            },
            data_type: ffi::PortDataType::from_rust(rust_port.data_type),
        }
    }
}

impl PortHandle {
    fn close(self: &mut PortHandle) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| String::from(format!("{e}")))?
            .close();
        Ok(())
    }

    fn name(self: &PortHandle) -> Result<String, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .name()
            .map_err(|e| e.to_string())?)
    }

    fn data_type(self: &PortHandle) -> Result<ffi::PortDataType, String> {
        Ok(ffi::PortDataType::from_rust(
            self.port
                .try_borrow()
                .map_err(|e| String::from(format!("{e}")))?
                .data_type()
                .map_err(|e| e.to_string())?,
        ))
    }

    unsafe fn maybe_driver_handle(self: &PortHandle) -> Result<Box<DriverHandle>, String> {
        let handle = self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .driver_handle()
            .map_err(|e| e.to_string())?;

        match handle.upgrade() {
            Some(handle) => Ok(Box::new(DriverHandle { driver: handle })),
            None => Err(String::from("Driver handle no longer exists")),
        }
    }

    fn has_internal_read_access(self: &PortHandle) -> Result<bool, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .has_internal_read_access()
            .map_err(|e| e.to_string())?)
    }

    fn has_internal_write_access(self: &PortHandle) -> Result<bool, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .has_internal_write_access()
            .map_err(|e| e.to_string())?)
    }

    fn has_implicit_input_source(self: &PortHandle) -> Result<bool, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .has_implicit_input_source()
            .map_err(|e| e.to_string())?)
    }

    fn has_implicit_output_sink(self: &PortHandle) -> Result<bool, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .has_implicit_output_sink()
            .map_err(|e| e.to_string())?)
    }

    fn get_external_connections(
        self: &PortHandle,
    ) -> Result<Vec<ffi::ExternalConnectionStatus>, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .get_external_connections()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|status| ffi::ExternalConnectionStatus::from_rust(status))
            .collect())
    }

    fn connect_external(self: &mut PortHandle, external_port: String) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| String::from(format!("{e}")))?
            .connect_external(external_port)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn disconnect_external(self: &mut PortHandle, external_port: String) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| String::from(format!("{e}")))?
            .disconnect_external(external_port)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn input_connectability(self: &PortHandle) -> Result<u32, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .input_connectability()
            .map_err(|e| e.to_string())?)
    }

    fn output_connectability(self: &PortHandle) -> Result<u32, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| String::from(format!("{e}")))?
            .output_connectability()
            .map_err(|e| e.to_string())?)
    }

    fn get_muted(self: &PortHandle) -> Result<bool, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .mutable()
            .ok_or(String::from("No mute function"))?
            .get_muted()
            .map_err(|e| format!("{e}"))?)
    }

    fn set_muted(self: &mut PortHandle, muted: bool) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .mutable_mut()
            .ok_or(String::from("No mute function"))?
            .set_muted(muted)
            .map_err(|e| format!("{e}"))?;
        Ok(())
    }

    fn set_n_ringbuffer_samples(self: &mut PortHandle, n_samples: u32) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .has_ringbuffer_mut()
            .ok_or(String::from("No ringbuffer on this port"))?
            .set_n_ringbuffer_samples(n_samples)
            .map_err(|e| format!("{e}"))?;
        Ok(())
    }

    fn get_n_ringbuffer_samples(self: &PortHandle) -> Result<u32, String> {
        Ok(self
            .port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .has_ringbuffer()
            .ok_or(String::from("No ringbuffer on this port"))?
            .get_n_ringbuffer_samples()
            .map_err(|e| format!("{e}"))?)
    }
}

impl DriverHandle {
    fn start(self: &mut DriverHandle) -> Result<(), String> {
        self.driver
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .start()
            .map_err(|e| format!("{e}").into())
    }
    fn close(self: &mut DriverHandle) -> Result<(), String> {
        self.driver
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .close()
            .map_err(|e| format!("{e}"))?;
        Ok(())
    }
    fn open_audio_port(self: &mut DriverHandle) -> Result<Box<PortHandle>, String> {
        let port = self
            .driver
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .open_audio_port()
            .map_err(|e| format!("{e}"))?;
        Ok(Box::new(PortHandle { port }))
    }
    fn open_midi_port(self: &mut DriverHandle) -> Result<Box<PortHandle>, String> {
        let port = self
            .driver
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .open_midi_port()
            .map_err(|e| format!("{e}"))?;
        Ok(Box::new(PortHandle { port }))
    }
    fn open_decoupled_midi_port(
        self: &mut DriverHandle,
    ) -> Result<Box<DecoupledMidiPortHandle>, String> {
        todo!()
    }
    fn get_xruns(self: &DriverHandle) -> Result<u32, String> {
        Ok(self
            .driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .get_xruns()
            .map_err(|e| format!("{e}"))?)
    }
    fn get_sample_rate(self: &DriverHandle) -> Result<u32, String> {
        Ok(self
            .driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .get_sample_rate()
            .map_err(|e| format!("{e}"))?)
    }
    fn get_buffer_size(self: &DriverHandle) -> Result<u32, String> {
        Ok(self
            .driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .get_buffer_size()
            .map_err(|e| format!("{e}"))?)
    }
    fn get_dsp_load(self: &DriverHandle) -> Result<f32, String> {
        Ok(self
            .driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .get_dsp_load()
            .map_err(|e| format!("{e}"))?)
    }
    fn get_client_name(self: &DriverHandle) -> Result<String, String> {
        Ok(self
            .driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .get_client_name()
            .map_err(|e| format!("{e}"))?)
    }
    fn get_active(self: &DriverHandle) -> Result<bool, String> {
        Ok(self
            .driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .get_active()
            .map_err(|e| format!("{e}"))?)
    }
    fn get_last_processed(self: &DriverHandle) -> Result<u32, String> {
        Ok(self
            .driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .get_last_processed()
            .map_err(|e| format!("{e}"))?)
    }

    fn find_external_ports(
        self: &DriverHandle,
    ) -> Result<Vec<ffi::ExternalPortDescriptor>, String> {
        Ok(self
            .driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .find_external_ports()
            .map_err(|e| format!("{e}"))?
            .into_iter()
            .map(|d| ffi::ExternalPortDescriptor::from_rust(d))
            .collect())
    }

    fn wait_process(self: &DriverHandle) -> Result<(), String> {
        self.driver
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .wait_process()
            .map_err(|e| format!("{e}"))?;
        Ok(())
    }
}

impl DecoupledMidiPortHandle {
    fn unregister(self: &mut DecoupledMidiPortHandle) -> Result<(), String> {
        todo!()
    }
}

impl AudioPortHandle {
    fn set_gain(self: &mut AudioPortHandle, gain: f32) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .audio_fader_mut()
            .ok_or(format!("Port is not an audio port"))?
            .set_gain(gain)
            .map_err(|e| format!("{e}"))?;
        Ok(())
    }

    fn get_gain(self: &AudioPortHandle) -> Result<f32, String> {
        self.port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .audio_fader()
            .ok_or(format!("Port is not an audio port"))?
            .get_gain()
            .map_err(|e| format!("{e}"))
    }

    fn reset_input_peak(self: &mut AudioPortHandle) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .audio_fader_mut()
            .ok_or(format!("Port is not an audio port"))?
            .reset_input_peak()
            .map_err(|e| format!("{e}"))
    }

    fn get_input_peak(self: &AudioPortHandle) -> Result<f32, String> {
        self.port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .audio_fader()
            .ok_or(format!("Port is not an audio port"))?
            .get_input_peak()
            .map_err(|e| format!("{e}"))
    }

    fn reset_output_peak(self: &mut AudioPortHandle) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .audio_fader_mut()
            .ok_or(format!("Port is not an audio port"))?
            .reset_output_peak()
            .map_err(|e| format!("{e}"))
    }

    fn get_output_peak(self: &AudioPortHandle) -> Result<f32, String> {
        self.port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .audio_fader()
            .ok_or(format!("Port is not an audio port"))?
            .get_output_peak()
            .map_err(|e| format!("{e}"))
    }
}

impl MidiPortHandle {
    fn reset_n_input_events(self: &mut MidiPortHandle) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .midi_indicators_mut()
            .ok_or(format!("Port is not a midi port"))?
            .reset_n_input_events()
            .map_err(|e| format!("{e}"))
    }

    fn get_n_input_events(self: &MidiPortHandle) -> Result<u32, String> {
        self.port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .midi_indicators()
            .ok_or(format!("Port is not a midi port"))?
            .get_n_input_events()
            .map_err(|e| format!("{e}"))
    }

    fn reset_n_output_events(self: &mut MidiPortHandle) -> Result<(), String> {
        self.port
            .try_borrow_mut()
            .map_err(|e| format!("{e}"))?
            .midi_indicators_mut()
            .ok_or(format!("Port is not a midi port"))?
            .reset_n_output_events()
            .map_err(|e| format!("{e}"))
    }

    fn get_n_output_events(self: &MidiPortHandle) -> Result<u32, String> {
        self.port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .midi_indicators()
            .ok_or(format!("Port is not a midi port"))?
            .get_n_output_events()
            .map_err(|e| format!("{e}"))
    }

    fn get_n_input_notes_active(self: &MidiPortHandle) -> Result<u32, String> {
        self.port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .midi_indicators()
            .ok_or(format!("Port is not a midi port"))?
            .get_n_input_notes_active()
            .map_err(|e| format!("{e}"))
    }
    fn get_n_output_notes_active(self: &MidiPortHandle) -> Result<u32, String> {
        self.port
            .try_borrow()
            .map_err(|e| format!("{e}"))?
            .midi_indicators()
            .ok_or(format!("Port is not a midi port"))?
            .get_n_output_notes_active()
            .map_err(|e| format!("{e}"))
    }
}
