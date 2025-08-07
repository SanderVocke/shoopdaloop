use crate::{
    any_backend_port::AnyBackendPort,
    cxx_qt_shoop::qobj_fx_chain_backend::FXChainBackend,
    cxx_qt_shoop::{qobj_backend_wrapper::BackendWrapper, rust::qobj_port_backend_bridge::ffi::*},
};
use backend_bindings::{PortConnectability, PortDataType, PortDirection};
use common::logging::macros::{debug as raw_debug, error as raw_error, shoop_log_unit};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QMap};
use cxx_qt_lib_shoop::qobject::{qobject_property_bool, FromQObject};
use std::{collections::HashMap, pin::Pin};
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
    pub fn update(mut self: Pin<&mut PortBackend>) {
        if self.maybe_backend_port.is_none() {
            return;
        }

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let port = self.maybe_backend_port.as_ref().unwrap();
            let prev_state = self.prev_state.clone();

            {
                let new_state = port.get_state();
                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.prev_state = new_state;
            }

            // Self "state_changed" signal
            todo!();

            Ok(())
        }() {
            error!(self, "Could not update: {e}")
        }
    }

    pub fn display_name(self: &PortBackend) -> String {
        if self.prev_state.name.len() > 0 {
            self.prev_state.name.clone()
        } else if self.name_hint.is_some() && self.name_hint.as_ref().unwrap().len() > 0 {
            self.name_hint.as_ref().unwrap().to_string()
        } else {
            "unknown".to_string()
        }
    }

    pub fn set_fx_chain(mut self: Pin<&mut PortBackend>, fx_chain: *mut QObject) {
        if self.maybe_backend_port.is_some() {
            error!(self, "cannot set FX chain after initialization");
            return;
        }
        if !self.fx_chain.as_ref().is_some_and(|v| *v == fx_chain) {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.fx_chain = Some(fx_chain);
            unsafe {
                self.as_mut().fx_chain_changed(fx_chain);
            }
        }
    }

    pub fn set_fx_chain_port_idx(self: Pin<&mut PortBackend>, fx_chain_port_idx: i32) {
        todo!();
    }

    pub fn maybe_initialize_backend_internal(mut self: Pin<&mut PortBackend>) -> bool {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let idx = self
                .fx_chain_port_idx
                .ok_or(anyhow::anyhow!("no fx chain port index set"))?;
            let output_connectability = self
                .output_connectability
                .as_ref()
                .ok_or(anyhow::anyhow!("output connectability not set"))?;
            let fx_chain: Pin<&mut FXChainBackend> = unsafe {
                FXChainBackend::from_qobject_mut_ptr(
                    self.fx_chain.ok_or(anyhow::anyhow!("fx chain not set"))?,
                )?
            };
            let backend_fx_chain: &Option<backend_bindings::FXChain> =
                fx_chain.get_backend_object();
            let is_input = !output_connectability.internal;
            let is_midi = self
                .port_type
                .ok_or(anyhow::anyhow!("port data type (is_midi) not set"))?
                == PortDataType::Midi;
            let port: AnyBackendPort = if is_input {
                if is_midi {
                    AnyBackendPort::Midi(
                        backend_fx_chain
                            .as_ref()
                            .unwrap()
                            .get_midi_input_port(idx as u32)
                            .ok_or(anyhow::anyhow!("Could not get FX chain MIDI input {idx}"))?,
                    )
                } else {
                    // audio
                    AnyBackendPort::Audio(
                        backend_fx_chain
                            .as_ref()
                            .unwrap()
                            .get_audio_input_port(idx as u32)
                            .ok_or(anyhow::anyhow!("Could not get FX chain audio input {idx}"))?,
                    )
                }
            } else {
                if is_midi {
                    return Err(anyhow::anyhow!(
                        "FX chains do not support internal MIDI out"
                    ));
                } else {
                    // audio
                    AnyBackendPort::Audio(
                        backend_fx_chain
                            .as_ref()
                            .unwrap()
                            .get_audio_output_port(idx as u32)
                            .ok_or(anyhow::anyhow!("Could not get FX chain audio output {idx}"))?,
                    )
                }
            };
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.maybe_backend_port = Some(port);

            debug!(self, "Initialized as internal-facing port");
            self.as_mut().update();
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
            let input_connectability = self
                .input_connectability
                .as_ref()
                .ok_or(anyhow::anyhow!("input_connectability not set"))?;
            let direction = if input_connectability.internal {
                PortDirection::Input
            } else {
                PortDirection::Output
            };
            let port_type = self
                .port_type
                .as_ref()
                .ok_or(anyhow::anyhow!("port data type (is_midi) not set"))?;
            unsafe {
                let name_hint = self
                    .name_hint
                    .as_ref()
                    .ok_or(anyhow::anyhow!("name_hint not set"))?
                    .to_string();
                let n_ringbuffer_samples: i32 = todo!();
                let mut rust_mut = self.as_mut().rust_mut();
                let backend =
                    BackendWrapper::from_qobject_ref_ptr(rust_mut.backend as *const QObject)?;
                rust_mut.maybe_backend_port = Some(AnyBackendPort::new_driver_port(
                    port_type.clone(),
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
                && self.is_internal.is_some()
        };

        let result = if initialize_condition {
            if self.is_internal.unwrap() {
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

    pub fn get_name(&self) -> QString {
        QString::from(self.prev_state.name.clone())
    }

    pub fn get_muted(&self) -> bool {
        self.prev_state.muted != 0
    }

    pub fn get_passthrough_muted(&self) -> bool {
        self.prev_state.passthrough_muted != 0
    }

    pub fn get_audio_input_peak(&self) -> f64 {
        self.prev_state.input_peak as f64
    }

    pub fn get_audio_output_peak(&self) -> f64 {
        self.prev_state.output_peak as f64
    }

    pub fn get_midi_n_input_events(&self) -> i32 {
        self.prev_state.n_input_events as i32
    }

    pub fn get_midi_n_output_events(&self) -> i32 {
        self.prev_state.n_output_events as i32
    }

    pub fn get_midi_n_input_notes_active(&self) -> i32 {
        self.prev_state.n_input_notes_active as i32
    }

    pub fn get_midi_n_output_notes_active(&self) -> i32 {
        self.prev_state.n_output_notes_active as i32
    }

    pub fn initialize_impl(self: Pin<&mut PortBackend>) {
        todo!();
    }

    pub fn connect_external_port(self: Pin<&mut PortBackend>, name: QString) {
        if let Some(port) = self.maybe_backend_port.as_ref() {
            port.connect_external_port(name.to_string().as_str());
        } else {
            error!(self, "Cannot connect external port: uninitialized");
        }
    }

    pub fn disconnect_external_port(self: Pin<&mut PortBackend>, name: QString) {
        if let Some(port) = self.maybe_backend_port.as_ref() {
            port.disconnect_external_port(name.to_string().as_str());
        } else {
            error!(self, "Cannot disconnect external port: uninitialized");
        }
    }

    pub fn get_connections_state_raw(
        self: Pin<&mut PortBackend>,
    ) -> Result<HashMap<String, bool>, anyhow::Error> {
        if let Some(port) = self.maybe_backend_port.as_ref() {
            Ok(port.get_connections_state())
        } else {
            Err(anyhow::anyhow!("uninitialized"))
        }
    }

    pub fn get_connections_state(mut self: Pin<&mut PortBackend>) -> QMap_QString_QVariant {
        match || -> Result<QMap_QString_QVariant, anyhow::Error> {
            let mut result: QMap_QString_QVariant = QMap::default();
            self.as_mut().get_connections_state_raw()?.iter().for_each(
                |(name, connected): (&String, &bool)| {
                    result.insert(QString::from(name), QVariant::from(connected));
                },
            );
            Ok(result)
        }() {
            Ok(connections) => connections,
            Err(err) => {
                error!(self, "Error while getting connections state: {}", err);
                QMap::default()
            }
        }
    }

    pub fn get_connected_external_ports_raw(
        self: Pin<&mut PortBackend>,
    ) -> Result<Vec<String>, anyhow::Error> {
        Ok(self
            .get_connections_state_raw()?
            .iter()
            .filter(|(_, connected)| **connected)
            .map(|(name, _)| name.clone())
            .collect())
    }

    pub fn get_connected_external_ports(mut self: Pin<&mut PortBackend>) -> QList_QString {
        match || -> Result<QList_QString, anyhow::Error> {
            let mut result: QList_QString = QList::default();
            self.as_mut()
                .get_connected_external_ports_raw()?
                .iter()
                .for_each(|name| {
                    result.append(QString::from(name));
                });
            Ok(result)
        }() {
            Ok(connections) => connections,
            Err(err) => {
                error!(self, "Error while getting connections list: {}", err);
                QList::default()
            }
        }
    }

    pub fn get_audio_gain(self: Pin<&mut PortBackend>) -> f32 {
        self.prev_state.gain
    }

    pub fn try_make_connections(
        mut self: Pin<&mut PortBackend>,
        desired_connections: QList_QString,
    ) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let connected: Vec<String> = self.as_mut().get_connected_external_ports_raw()?;
            let desired_connections: Vec<String> = desired_connections
                .iter()
                .map(|name| name.to_string())
                .collect();
            if let Some(port) = self.maybe_backend_port.as_ref() {
                // disconnect unwanted connections
                connected
                    .iter()
                    .filter(|name| !desired_connections.contains(name))
                    .for_each(|name| {
                        port.disconnect_external_port(name.as_str());
                    });

                // connect wanted connections
                desired_connections
                    .iter()
                    .filter(|name| !connected.contains(name))
                    .for_each(|name| {
                        port.connect_external_port(name.as_str());
                    });
            }
            Ok(())
        }() {
            error!(self, "Error while making connections: {e}");
        }
    }

    pub unsafe fn set_backend(mut self: Pin<&mut PortBackend>, backend: *mut QObject) {
        if self.maybe_backend_port.is_some() {
            error!(
                self,
                "Can't change backend after backend has been initialized"
            );
            return;
        }
        if self.backend != backend {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.backend = backend;
            unsafe {
                self.as_mut().backend_changed(backend);
            }
        }
    }

    pub fn set_name_hint(mut self: Pin<&mut PortBackend>, name_hint: QString) {
        if self.maybe_backend_port.is_some() {
            error!(
                self,
                "Can't change name hint after backend has been initialized"
            );
            return;
        }
        if !self.name_hint.as_ref().is_some_and(|v| name_hint == *v) {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.name_hint = Some(name_hint.clone());
            unsafe {
                self.as_mut().name_hint_changed(name_hint);
            }
        }
    }

    pub fn set_input_connectability(mut self: Pin<&mut PortBackend>, input_connectability: i32) {
        if self.maybe_backend_port.is_some() {
            error!(
                self,
                "Can't change input connectability after backend has been initialized"
            );
            return;
        }
        let input_connectability = PortConnectability::from_ffi(input_connectability as u32);
        if !self
            .input_connectability
            .as_ref()
            .is_some_and(|v| input_connectability == *v)
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.input_connectability = Some(input_connectability.clone());
            unsafe {
                self.as_mut()
                    .input_connectability_changed(input_connectability.to_ffi() as i32);
            }
        }
    }

    pub fn set_output_connectability(mut self: Pin<&mut PortBackend>, output_connectability: i32) {
        if self.maybe_backend_port.is_some() {
            error!(
                self,
                "Can't change output connectability after backend has been initialized"
            );
            return;
        }
        let output_connectability = PortConnectability::from_ffi(output_connectability as u32);
        if !self
            .output_connectability
            .as_ref()
            .is_some_and(|v| output_connectability == *v)
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.output_connectability = Some(output_connectability.clone());
            unsafe {
                self.as_mut()
                    .output_connectability_changed(output_connectability.to_ffi() as i32);
            }
        }
    }

    pub fn set_is_internal(mut self: Pin<&mut PortBackend>, is_internal: bool) {
        if self.maybe_backend_port.is_some() {
            error!(
                self,
                "Can't change internal flag after backend has been initialized"
            );
            return;
        }
        if !self.is_internal.as_ref().is_some_and(|v| is_internal == *v) {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.is_internal = Some(is_internal);
            unsafe {
                self.as_mut().is_internal_changed(is_internal);
            }
        }
    }

    pub fn set_is_midi(mut self: Pin<&mut PortBackend>, is_midi: bool) {
        if self.maybe_backend_port.is_some() {
            error!(
                self,
                "Can't change port type (is_midi) after backend has been initialized"
            );
            return;
        }
        let port_type = if is_midi {
            PortDataType::Midi
        } else {
            PortDataType::Audio
        };
        if !self.port_type.as_ref().is_some_and(|v| port_type == *v) {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.port_type = Some(port_type.clone());
            unsafe {
                self.as_mut().is_midi_changed(is_midi);
            }
        }
    }

    pub fn set_internal_port_connections(
        self: Pin<&mut PortBackend>,
        internal_port_connections: QList_QVariant,
    ) {
        todo!();
    }

    pub fn set_min_n_ringbuffer_samples(
        mut self: Pin<&mut PortBackend>,
        n_ringbuffer_samples: i32,
    ) {
        if self.maybe_backend_port.is_some() {
            error!(
                self,
                "Can't change minimum ringbuffer samples after backend has been initialized"
            );
            return;
        }
        if !self
            .min_n_ringbuffer_samples
            .as_ref()
            .is_some_and(|v| n_ringbuffer_samples == *v)
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.min_n_ringbuffer_samples = Some(n_ringbuffer_samples);
            unsafe {
                self.as_mut()
                    .min_n_ringbuffer_samples_changed(n_ringbuffer_samples);
            }
        }
    }

    pub fn get_min_n_ringbuffer_samples(self: Pin<&mut PortBackend>) -> i32 {
        self.min_n_ringbuffer_samples.unwrap_or(0)
    }

    pub fn set_audio_gain(self: Pin<&mut PortBackend>, audio_gain: f64) {
        if let Some(port) = self.maybe_backend_port.as_ref() {
            port.set_gain(audio_gain as f32);
        } else {
            error!(
                self,
                "Can't change audio gain before backend has been initialized"
            );
        }
    }

    pub fn dummy_queue_audio_data(self: Pin<&mut PortBackend>, audio_data: QList_f64) {
        todo!();
    }

    pub fn dummy_dequeue_audio_data(self: Pin<&mut PortBackend>) -> QList_f64 {
        todo!();
    }

    pub fn dummy_request_data(self: Pin<&mut PortBackend>, n: i32) {
        if let Some(port) = self.maybe_backend_port.as_ref() {
            port.dummy_request_data(n as u32);
        } else {
            error!(
                self,
                "Can't request data before backend has been initialized"
            );
        }
    }

    pub fn dummy_queue_midi_msgs(self: Pin<&mut PortBackend>, midi_msgs: QList_QVariant) {
        todo!();
    }

    pub fn dummy_dequeue_midi_msgs(self: Pin<&mut PortBackend>) -> QList_QVariant {
        todo!();
    }

    pub fn dummy_clear_queues(self: Pin<&mut PortBackend>) {
        if let Some(port) = self.maybe_backend_port.as_ref() {
            port.dummy_clear_queues();
        } else {
            error!(
                self,
                "Can't clear queues before backend has been initialized"
            );
        }
    }

    pub fn get_n_ringbuffer_samples(self: Pin<&mut PortBackend>) -> i32 {
        self.prev_state.ringbuffer_n_samples as i32
    }

    pub fn get_is_midi(self: Pin<&mut PortBackend>) -> bool {
        self.port_type.unwrap_or(PortDataType::Audio) == PortDataType::Midi
    }

    pub fn get_maybe_fx_chain(self: Pin<&mut PortBackend>) -> *mut QObject {
        self.fx_chain.unwrap_or(std::ptr::null_mut())
    }

    pub fn get_fx_chain_port_idx(self: Pin<&mut PortBackend>) -> i32 {
        self.fx_chain_port_idx.unwrap_or(0) as i32
    }

    pub fn get_name_hint(self: Pin<&mut PortBackend>) -> QString {
        if let Some(name_hint) = self.name_hint.as_ref() {
            name_hint.clone()
        } else {
            QString::default()
        }
    }

    pub fn get_input_connectability(self: Pin<&mut PortBackend>) -> i32 {
        if let Some(c) = self.input_connectability.as_ref() {
            c.to_ffi() as i32
        } else {
            0
        }
    }

    pub fn get_output_connectability(self: Pin<&mut PortBackend>) -> i32 {
        if let Some(c) = self.output_connectability.as_ref() {
            c.to_ffi() as i32
        } else {
            0
        }
    }

    pub fn get_is_internal(self: Pin<&mut PortBackend>) -> bool {
        self.is_internal.unwrap_or(false)
    }
}
