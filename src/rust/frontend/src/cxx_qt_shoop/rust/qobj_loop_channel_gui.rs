use crate::cxx_qt_shoop::rust::qobj_loop_channel_gui_bridge::ffi::*;
use crate::engine_update_thread;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QMap};
use cxx_qt_lib_shoop::connect::connect_or_report;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::qobject::{AsQObject, FromQObject};
use cxx_qt_lib_shoop::qsharedpointer_qobject::QSharedPointer_QObject;
use cxx_qt_lib_shoop::qvariant_helpers::{
    qsharedpointer_qobject_to_qvariant, qvariant_to_qobject_ptr,
};
use cxx_qt_lib_shoop::{invokable, qobject::ffi::qobject_move_to_thread};
use std::pin::Pin;
shoop_log_unit!("Frontend.LoopChannel");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

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

impl LoopChannelGui {
    pub fn initialize_impl(mut self: Pin<&mut LoopChannelGui>) {
        debug!(self, "Initializing");

        unsafe {
            let backend_channel = make_raw_loop_channel_backend();
            let backend_channel_qobj = loop_channel_backend_qobject_from_ptr(backend_channel);
            qobject_move_to_thread(
                backend_channel_qobj,
                engine_update_thread::get_engine_update_thread().thread,
            )
            .unwrap();

            let self_ref = self.as_ref().get_ref();

            {
                let backend_ref = &*backend_channel_qobj;
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
                        "backend_set_ports_to_connect(QList<QVariant>)",
                        backend_ref,
                        "set_ports_to_connect(QList<QVariant>)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_mode(::std::int32_t)",
                        backend_ref,
                        "push_mode(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_start_offset(::std::int32_t)",
                        backend_ref,
                        "push_start_offset(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_n_preplay_samples(::std::int32_t)",
                        backend_ref,
                        "push_n_preplay_samples(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_audio_gain(double)",
                        backend_ref,
                        "push_audio_gain(double)",
                        connection_types::QUEUED_CONNECTION,
                    );
                }
                {
                    // Connections: backend object -> GUI
                    connect_or_report(
                        backend_ref,
                        "state_changed(bool,::std::int32_t,::std::int32_t,::std::int32_t,QVariant,::std::int32_t,bool,double,double,::std::int32_t,::std::int32_t)",
                        self_ref,
                        "backend_state_changed(bool,::std::int32_t,::std::int32_t,::std::int32_t,QVariant,::std::int32_t,bool,double,double,::std::int32_t,::std::int32_t)",
                        connection_types::QUEUED_CONNECTION
                    );
                }

                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.backend_channel_wrapper =
                    QSharedPointer_QObject::from_ptr_delete_later(backend_port_qobj).unwrap();
            }
        }
    }
}