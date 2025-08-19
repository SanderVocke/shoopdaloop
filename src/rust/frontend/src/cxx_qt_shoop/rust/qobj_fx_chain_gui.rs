use crate::{
    cxx_qt_shoop::{
        qobj_fx_chain_backend_bridge::ffi::{
            fx_chain_backend_qobject_from_ptr, make_raw_fx_chain_backend,
        },
        qobj_fx_chain_gui_bridge::ffi::*,
    },
    engine_update_thread,
};
use std::pin::Pin;

use common::logging::macros::{debug as raw_debug, error as raw_error, shoop_log_unit};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types, invokable,
    qobject::{ffi::qobject_move_to_thread, AsQObject},
    qsharedpointer_qobject::QSharedPointer_QObject,
    qvariant_helpers::qsharedpointer_qobject_to_qvariant,
};
shoop_log_unit!("Frontend.FXChain");

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

impl FXChainGui {
    pub fn initialize_impl(mut self: Pin<&mut Self>) {
        debug!(self, "Initializing");

        unsafe {
            let backend_fx_chain = make_raw_fx_chain_backend();
            let backend_fx_chain_qobj = fx_chain_backend_qobject_from_ptr(backend_fx_chain);
            qobject_move_to_thread(
                backend_fx_chain_qobj,
                engine_update_thread::get_engine_update_thread().thread,
            )
            .unwrap();

            let self_ref = self.as_ref().get_ref();

            {
                let backend_ref = &*backend_fx_chain_qobj;
                let backend_thread_wrapper =
                    &*engine_update_thread::get_engine_update_thread().ref_qobject_ptr();

                {
                    // Connections: update thread -> backend object
                    connect_or_report(
                        backend_thread_wrapper,
                        "update()",
                        backend_ref,
                        "update()",
                        connection_types::DIRECT_CONNECTION,
                    );
                }
                {
                    // Connections: GUI -> backend object
                    connect_or_report(
                        self_ref,
                        "backend_set_backend(QObject*)",
                        backend_ref,
                        "set_backend(QObject*)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_title(QString)",
                        backend_ref,
                        "set_title(QString)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_chain_type(::std::int32_t)",
                        backend_ref,
                        "set_chain_type(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_active(bool)",
                        backend_ref,
                        "push_active(bool)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_ui_visible(bool)",
                        backend_ref,
                        "push_ui_visible(bool)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_restore_state(QString)",
                        backend_ref,
                        "restore_state(QString)",
                        connection_types::QUEUED_CONNECTION,
                    );
                }
                {
                    // Connections: backend object -> GUI
                    connect_or_report(
                        backend_ref,
                        "state_changed(bool,bool,bool,bool)",
                        self_ref,
                        "backend_state_changed(bool,bool,bool,bool)",
                        connection_types::QUEUED_CONNECTION,
                    )
                }

                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.backend_chain_wrapper =
                    QSharedPointer_QObject::from_ptr_delete_later(backend_fx_chain_qobj).unwrap();
            }
        }
    }

    pub fn display_name(self: &Self) -> String {
        if self.title.len() > 0 {
            self.title.to_string()
        } else {
            "unknown".to_string()
        }
    }

    pub unsafe fn backend_state_changed(
        mut self: Pin<&mut FXChainGui>,
        initialized: bool,
        ready: bool,
        active: bool,
        visible: bool,
    ) {
        if initialized != self.initialized {
            debug!(self, "initialized -> {initialized}");
            self.as_mut().rust_mut().initialized = initialized;
            unsafe {
                self.as_mut().initialized_changed(initialized);
            }
        }
        if ready != self.ready {
            debug!(self, "ready -> {ready}");
            self.as_mut().rust_mut().ready = ready;
            unsafe {
                self.as_mut().ready_changed(ready);
            }
        }
        if active != self.active {
            debug!(self, "active -> {active}");
            self.as_mut().rust_mut().active = active;
            unsafe {
                self.as_mut().active_changed(active);
            }
        }
        if visible != self.ui_visible {
            debug!(self, "ui visible -> {visible}");
            self.as_mut().rust_mut().ui_visible = visible;
            unsafe {
                self.as_mut().ui_visible_changed(visible);
            }
        }
    }

    pub fn set_backend(mut self: Pin<&mut Self>, backend: *mut QObject) {
        unsafe {
            self.as_mut().backend_set_backend(backend);
        }
        if backend != self.backend {
            self.as_mut().rust_mut().backend = backend;
            unsafe {
                self.as_mut().backend_changed(backend);
            }
        }
    }

    pub fn set_title(mut self: Pin<&mut Self>, title: QString) {
        unsafe {
            self.as_mut().backend_set_title(title.clone());
        }
        if title != self.title {
            self.as_mut().rust_mut().title = title.clone();
            unsafe {
                self.as_mut().title_changed(title);
            }
        }
    }

    pub fn set_chain_type(mut self: Pin<&mut Self>, chain_type: i32) {
        unsafe {
            self.as_mut().backend_set_chain_type(chain_type);
        }
        if chain_type != self.chain_type {
            self.as_mut().rust_mut().chain_type = chain_type;
            unsafe {
                self.as_mut().chain_type_changed(chain_type);
            }
        }
    }

    pub fn get_state_str(self: Pin<&mut Self>) -> QString {
        match || -> Result<QString, anyhow::Error> {
            let backend_wrapper = self.backend_chain_wrapper.data()?;
            unsafe {
                Ok(invokable::invoke(
                    &mut *backend_wrapper,
                    "get_state_str()",
                    invokable::BLOCKING_QUEUED_CONNECTION,
                    &(),
                )?)
            }
        }() {
            Ok(data) => data,
            Err(e) => {
                error!(self, "Could not get state string: {e}");
                QString::from("")
            }
        }
    }

    pub fn restore_state(self: Pin<&mut Self>, state_str: QString) {
        unsafe {
            self.backend_restore_state(state_str);
        }
    }

    pub unsafe fn get_backend_fx_chain(self: Pin<&mut FXChainGui>) -> QVariant {
        match || -> Result<QVariant, anyhow::Error> {
            if self.backend_chain_wrapper.is_null() {
                return Ok(QVariant::default());
            } else {
                return Ok(qsharedpointer_qobject_to_qvariant(
                    self.backend_chain_wrapper
                        .as_ref()
                        .ok_or(anyhow::anyhow!("Backend wrapper not set"))?,
                )?);
            }
        }() {
            Ok(obj) => {
                return obj;
            }
            Err(e) => {
                error!(self, "Could not get backend object: {e}");
                return QVariant::default();
            }
        }
    }

    pub fn push_active(mut self: Pin<&mut Self>, active: bool) {
        unsafe {
            self.as_mut().backend_push_active(active);
        }
    }

    pub fn push_ui_visible(mut self: Pin<&mut Self>, visible: bool) {
        unsafe {
            self.as_mut().backend_push_ui_visible(visible);
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_fx_chain_gui(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
