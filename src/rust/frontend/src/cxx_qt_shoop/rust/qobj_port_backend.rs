use crate::{
    any_backend_port::AnyBackendPort,
    cxx_qt_shoop::{qobj_backend_wrapper::BackendWrapper, rust::qobj_port_backend_bridge::ffi::*},
};
use backend_bindings::{PortConnectability, PortDataType, PortDirection};
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::qobject::{qobject_property_bool, FromQObject};
use std::pin::Pin;
shoop_log_unit!("Frontend.Port");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*));
    };
}

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*));
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*));
    };
}

impl PortBackend {
    pub fn update(self: Pin<&mut PortBackend>) {
        todo!();
    }

    pub fn display_name(self: &PortBackend) -> String {
        if self.name.len() > 0 {
            self.name.to_string()
        } else if self.name_hint.len() > 0 {
            self.name_hint.to_string()
        } else {
            "unknown".to_string()
        }
    }

    pub fn set_fx_chain(self: Pin<&mut PortBackend>, fx_chain: *mut QObject) {
        todo!();
    }

    pub fn set_fx_chain_port_idx(self: Pin<&mut PortBackend>, fx_chain_port_idx: i32) {
        todo!();
    }

    pub fn maybe_initialize_backend_internal(self: Pin<&mut PortBackend>) -> bool {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let idx = self.fx_chain_port_idx;
            let n_ringbuffer = self.n_ringbuffer_samples;
            let output_connectability = PortConnectability::from_ffi(self.output_connectability as u32);
            if !output_connectability.internal {

            }
            todo!();
            Ok(())
        }() {
            error!(
                self,
                "Failed to initialize internal-facing backend port: {e}"
            );
            return false;
        }
        return true;
    }

    pub fn maybe_initialize_backend_external(mut self: Pin<&mut PortBackend>) -> bool {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let input_connectability =
                PortConnectability::from_ffi(self.input_connectability as u32);
            let direction = if input_connectability.internal {
                PortDirection::Input
            } else {
                PortDirection::Output
            };
            let port_type = if self.is_midi {
                PortDataType::Midi
            } else {
                PortDataType::Audio
            };
            unsafe {
                let name_hint = self.name_hint.to_string();
                let n_ringbuffer_samples = self.n_ringbuffer_samples;
                let mut rust_mut = self.as_mut().rust_mut();
                let backend =
                    BackendWrapper::from_qobject_ref_ptr(rust_mut.backend as *const QObject)?;
                rust_mut.maybe_backend_port = Some(AnyBackendPort::new_driver_port(
                    port_type,
                    backend
                        .session
                        .as_ref()
                        .ok_or(anyhow::anyhow!("No session in backend"))?,
                    backend
                        .driver
                        .as_ref()
                        .ok_or(anyhow::anyhow!("No driver in backend"))?,
                    name_hint.as_str(),
                    &direction,
                    n_ringbuffer_samples as u32,
                )?);
            }
            debug!(self, "Initialized as external-facing port ({port_type:?})");

            self.as_mut().update();
            Ok(())
        }() {
            error!(
                self,
                "Failed to initialize external-facing backend port: {e}"
            );
            return false;
        }
        return true;
    }

    pub fn maybe_initialize_backend(mut self: Pin<&mut PortBackend>) -> bool {
        if self.initialized {
            return true;
        }

        let initialize_condition: bool = unsafe {
            !self.initialized
                && !self.backend.is_null()
                && qobject_property_bool(self.backend.as_ref().unwrap(), "ready".to_string())
                    .unwrap_or(false)
                && self.maybe_backend_port.is_none()
        };

        let result = if initialize_condition {
            if self.is_internal {
                self.as_mut().maybe_initialize_backend_internal()
            } else {
                self.as_mut().maybe_initialize_backend_external()
            }
        } else {
            false
        };

        self.as_mut().set_initialized(result);
        result
    }

    pub fn set_initialized(mut self: Pin<&mut Self>, value: bool) {
        let mut rust_mut = self.as_mut().rust_mut();
        if rust_mut.initialized != value {
            rust_mut.initialized = value;
            unsafe {
                self.as_mut().initialized_changed(value);
            }
        }
    }

    pub fn get_initialized(&self) -> bool {
        self.initialized
    }

    pub fn get_name(&self) -> &QString {
        todo!();
    }

    pub fn get_muted(&self) -> bool {
        todo!();
    }

    pub fn get_passthrough_muted(&self) -> bool {
        todo!();
    }

    pub fn get_audio_input_peak(&self) -> f64 {
        todo!();
    }

    pub fn get_audio_output_peak(&self) -> f64 {
        todo!();
    }

    pub fn get_midi_n_input_events(&self) -> i32 {
        todo!();
    }

    pub fn get_midi_n_output_events(&self) -> i32 {
        todo!();
    }

    pub fn get_midi_n_input_notes_active(&self) -> i32 {
        todo!();
    }

    pub fn get_midi_n_output_notes_active(&self) -> i32 {
        todo!();
    }

    pub fn initialize_impl(self: Pin<&mut PortBackend>) {
        todo!();
    }

    pub fn connect_external_port(self: Pin<&mut PortBackend>, name: QString) {
        todo!();
    }

    pub fn disconnect_external_port(self: Pin<&mut PortBackend>, name: QString) {
        todo!();
    }

    pub fn get_connections_state(self: Pin<&mut PortBackend>) -> QMap_QString_QVariant {
        todo!();
    }

    pub fn get_connected_external_ports(self: Pin<&mut PortBackend>) -> QList_QString {
        todo!();
    }

    pub fn try_make_connections(self: Pin<&mut PortBackend>, connections: QList_QString) {
        todo!();
    }

    pub unsafe fn set_backend(mut self: Pin<&mut PortBackend>, backend: *mut QObject) {
        todo!();
    }

    pub fn set_name_hint(mut self: Pin<&mut PortBackend>, name_hint: QString) {
        todo!();
    }

    pub fn set_input_connectability(mut self: Pin<&mut PortBackend>, input_connectability: i32) {
        todo!();
    }

    pub fn set_output_connectability(mut self: Pin<&mut PortBackend>, output_connectability: i32) {
        todo!();
    }

    pub fn set_is_internal(mut self: Pin<&mut PortBackend>, is_internal: bool) {
        todo!();
    }

    pub fn set_is_midi(self: Pin<&mut PortBackend>, is_midi: bool) {
        todo!();
    }

    pub fn set_internal_port_connections(
        mut self: Pin<&mut PortBackend>,
        internal_port_connections: QList_QVariant,
    ) {
        todo!();
    }

    pub fn set_n_ringbuffer_samples(mut self: Pin<&mut PortBackend>, n_ringbuffer_samples: i32) {
        todo!();
    }

    pub fn set_audio_gain(mut self: Pin<&mut PortBackend>, audio_gain: f64) {
        todo!();
    }

    pub fn dummy_queue_audio_data(self: Pin<&mut PortBackend>, audio_data: QList_f64) {
        todo!();
    }

    pub fn dummy_dequeue_audio_data(self: Pin<&mut PortBackend>) -> QList_f64 {
        todo!();
    }

    pub fn dummy_request_data(self: Pin<&mut PortBackend>, n: i32) {
        todo!();
    }

    pub fn dummy_queue_midi_msgs(self: Pin<&mut PortBackend>, midi_msgs: QList_QVariant) {
        todo!();
    }

    pub fn dummy_dequeue_midi_msgs(self: Pin<&mut PortBackend>) -> QList_QVariant {
        todo!();
    }

    pub fn dummy_clear_queues(self: Pin<&mut PortBackend>) {
        todo!();
    }
}
