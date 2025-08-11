use backend_bindings::{FXChain as BackendFXChain, FXChainType};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    qobject::{qobject_property_bool, FromQObject},
};

pub use crate::cxx_qt_shoop::qobj_fx_chain_backend_bridge::ffi::FXChainBackend;
use crate::cxx_qt_shoop::{
    qobj_backend_wrapper::BackendWrapper, qobj_fx_chain_backend_bridge::ffi::*,
};
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace, warn as raw_warn,
};
use std::pin::Pin;
shoop_log_unit!("Frontend.FXChain");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! warn {
    ($self:ident, $($arg:tt)*) => {
        raw_warn!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

impl FXChainBackend {
    pub fn initialize_impl(self: Pin<&mut Self>) {}

    pub fn update(mut self: Pin<&mut FXChainBackend>) {
        if self.backend_chain_wrapper.is_none() {
            return;
        }

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let chain = self.backend_chain_wrapper.as_ref().unwrap();
            let prev_state = self.prev_state.clone();
            let new_state = chain
                .get_state()
                .ok_or(anyhow::anyhow!("Could not get FX chain state"))?;

            {
                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.prev_state = new_state.clone();
            }

            // Self "state_changed" signal
            unsafe {
                let initialized = self.initialized;
                self.as_mut().state_changed(
                    initialized,
                    new_state.ready != 0,
                    new_state.active != 0,
                    new_state.visible != 0,
                );
            }

            // Update individual field signals
            unsafe {
                if new_state.ready != prev_state.ready {
                    self.as_mut().ready_changed(new_state.ready != 0);
                }
                if new_state.active != prev_state.active {
                    self.as_mut().active_changed(new_state.active != 0);
                }
                if new_state.visible != prev_state.visible {
                    self.as_mut().ui_visible_changed(new_state.visible != 0);
                }
            }

            Ok(())
        }() {
            error!(self, "Could not update: {e}");
        }
    }

    pub fn maybe_initialize_backend(mut self: Pin<&mut FXChainBackend>) -> bool {
        if self.initialized {
            return true;
        }

        let mut non_ready_vars: Vec<String> = Vec::new();
        unsafe {
            if self.backend.is_null() {
                non_ready_vars.push("backend".to_string());
            }
            if !self.backend.is_null()
                && !qobject_property_bool(self.backend.as_ref().unwrap(), "ready").unwrap_or(false)
            {
                non_ready_vars.push("backend_ready".to_string());
            }
            if self.chain_type.is_none() {
                non_ready_vars.push("chain_type".to_string());
            }
            if self.title.is_none() {
                non_ready_vars.push("title".to_string());
            }
        }

        let initialize_condition: bool = !self.initialized && non_ready_vars.is_empty();

        let result = if initialize_condition {
            if let Err(e) = || -> Result<(), anyhow::Error> {
                unsafe {
                    let backend =
                        BackendWrapper::from_qobject_ref_ptr(self.backend as *const QObject)?;
                    let chain = backend
                        .session
                        .as_ref()
                        .ok_or(anyhow::anyhow!("No session in backend"))?
                        .create_fx_chain(
                            self.chain_type.unwrap() as u32,
                            self.title.as_ref().unwrap().as_str(),
                        )?;

                    // To push any state that was already set on us before initializing
                    let state = &self.prev_state;
                    debug!(self, "Push deferred state: {state:?}");
                    chain.set_active(state.active != 0);
                    chain.set_visible(state.visible != 0);

                    let mut rust_mut = self.as_mut().rust_mut();
                    rust_mut.backend_chain_wrapper = Some(chain);

                    Ok(())
                }
            }() {
                error!(self, "Failed to initialize backend FX chain: {e}");
                false
            } else {
                true
            }
        } else {
            trace!(
                self,
                "not initializing backend yet. Non-ready variables: {non_ready_vars:?}"
            );
            false
        };

        self.set_initialized(result);
        result
    }

    pub fn display_name(self: &Self) -> String {
        if self.title.is_some() && self.title.as_ref().unwrap().len() > 0 {
            self.title.as_ref().unwrap().to_string()
        } else {
            "unknown".to_string()
        }
    }

    pub fn set_backend(mut self: Pin<&mut Self>, backend: *mut QObject) {
        self.as_mut().maybe_initialize_backend();
        if self.backend_chain_wrapper.is_some() {
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
                if !backend.is_null() {
                    let self_qobject = fx_chain_backend_qobject_from_ptr(
                        self.as_mut().get_unchecked_mut() as *mut Self,
                    );
                    trace!(self, "Connect back-end ready signal");
                    connect_or_report(
                        &mut *backend,
                        "readyChanged()",
                        &mut *self_qobject,
                        "maybe_initialize_backend()",
                        connection_types::QUEUED_CONNECTION,
                    );
                }
                self.as_mut().backend_changed(backend);
            }
        }
    }

    pub fn set_title(mut self: Pin<&mut Self>, title: QString) {
        self.as_mut().maybe_initialize_backend();
        if self.backend_chain_wrapper.is_some() {
            error!(
                self,
                "Can't change title after backend has been initialized"
            );
            return;
        }
        let title_string = title.to_string();
        if !self.title.as_ref().is_some_and(|v| *v == title_string) {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.title = Some(title_string);
            unsafe {
                self.as_mut().title_changed(title);
            }
        }
    }

    pub fn set_chain_type(mut self: Pin<&mut Self>, chain_type: i32) {
        self.as_mut().maybe_initialize_backend();
        if self.backend_chain_wrapper.is_some() {
            error!(
                self,
                "Can't change chain type after backend has been initialized"
            );
            return;
        }
        let chain_type_enum = FXChainType::try_from(chain_type).unwrap();
        if !self
            .chain_type
            .as_ref()
            .is_some_and(|v| *v == chain_type_enum)
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.chain_type = Some(chain_type_enum);
            unsafe {
                self.as_mut().chain_type_changed(chain_type);
            }
        }
    }

    pub fn get_state_str(self: Pin<&mut Self>) -> QString {
        if let Some(backend_chain) = self.backend_chain_wrapper.as_ref() {
            let state_str = backend_chain.get_state_str();
            if let Some(state_str) = state_str {
                QString::from(state_str.as_str())
            } else {
                warn!(self, "Got null state string from backend FX chain");
                QString::from("")
            }
        } else {
            error!(self, "Cannot get state string of uninitialized FX chain");
            QString::from("")
        }
    }

    pub fn restore_state(self: Pin<&mut Self>, state_str: QString) {
        if let Some(backend_chain) = self.backend_chain_wrapper.as_ref() {
            backend_chain.restore_state(state_str.to_string().as_str());
        } else {
            error!(
                self,
                "Cannot restore from state string for uninitialized FX chain"
            );
        }
    }

    pub fn get_midi_input_port(
        self: Pin<&mut Self>,
        idx: u32,
    ) -> Option<backend_bindings::MidiPort> {
        if let Some(backend_chain) = self.backend_chain_wrapper.as_ref() {
            backend_chain.get_midi_input_port(idx)
        } else {
            error!(
                self,
                "Cannot get MIDI input port for uninitialized FX chain"
            );
            None
        }
    }

    pub fn get_audio_input_port(
        self: Pin<&mut Self>,
        idx: u32,
    ) -> Option<backend_bindings::AudioPort> {
        if let Some(backend_chain) = self.backend_chain_wrapper.as_ref() {
            backend_chain.get_audio_input_port(idx)
        } else {
            error!(
                self,
                "Cannot get audio input port for uninitialized FX chain"
            );
            None
        }
    }

    pub fn get_audio_output_port(
        self: Pin<&mut Self>,
        idx: u32,
    ) -> Option<backend_bindings::AudioPort> {
        if let Some(backend_chain) = self.backend_chain_wrapper.as_ref() {
            backend_chain.get_audio_output_port(idx)
        } else {
            error!(
                self,
                "Cannot get audio output port for uninitialized FX chain"
            );
            None
        }
    }

    pub fn push_ui_visible(mut self: Pin<&mut FXChainBackend>, ui_visible: bool) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chain) = self.backend_chain_wrapper.as_ref() {
            chain.set_visible(ui_visible);
        } else {
            debug!(self, "ui visible (deferred) -> {ui_visible}");
            self.as_mut().rust_mut().prev_state.visible = if ui_visible { 1 } else { 0 };
        }
    }

    pub fn push_active(mut self: Pin<&mut FXChainBackend>, active: bool) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chain) = self.backend_chain_wrapper.as_ref() {
            chain.set_active(active);
        } else {
            debug!(self, "active (deferred) -> {active}");
            self.as_mut().rust_mut().prev_state.active = if active { 1 } else { 0 };
        }
    }

    pub fn get_ui_visible(self: Pin<&mut FXChainBackend>) -> bool {
        self.prev_state.visible != 0
    }

    pub fn get_ready(self: Pin<&mut FXChainBackend>) -> bool {
        self.prev_state.ready != 0
    }

    pub fn get_active(self: Pin<&mut FXChainBackend>) -> bool {
        self.prev_state.visible != 0
    }

    pub fn get_chain_type(self: Pin<&mut FXChainBackend>) -> i32 {
        self.chain_type.unwrap_or(FXChainType::CarlaRack) as i32
    }

    pub fn get_title(self: Pin<&mut FXChainBackend>) -> QString {
        QString::from(self.title.as_ref().unwrap_or(&"FX Chain".to_string()))
    }

    pub fn set_initialized(mut self: Pin<&mut FXChainBackend>, initialized: bool) {
        let mut rust_mut = self.as_mut().rust_mut();
        if rust_mut.initialized != initialized {
            rust_mut.initialized = initialized;
            unsafe {
                self.as_mut().initialized_changed(initialized);
            }
        }
    }
}
