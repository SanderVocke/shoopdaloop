use crate::cxx_qt_shoop::rust::qobj_port_backend_bridge::ffi::{
    make_raw_port_backend, port_backend_qobject_from_ptr,
};
use crate::cxx_qt_shoop::rust::qobj_port_gui_bridge::ffi::*;
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
shoop_log_unit!("Frontend.Port");

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

impl PortGui {
    pub fn initialize_impl(mut self: Pin<&mut PortGui>) {
        debug!(self, "Initializing");

        unsafe {
            let backend_port = make_raw_port_backend();
            let backend_port_qobj = port_backend_qobject_from_ptr(backend_port);
            qobject_move_to_thread(
                backend_port_qobj,
                engine_update_thread::get_engine_update_thread().thread,
            )
            .unwrap();

            let self_ref = self.as_ref().get_ref();

            {
                let backend_ref = &*backend_port_qobj;
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
                        "backend_connect_external_port(QString)",
                        backend_ref,
                        "connect_external_port(QString)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_disconnect_external_port(QString)",
                        backend_ref,
                        "disconnect_external_port(QString)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_try_make_connections(QList<QString>)",
                        backend_ref,
                        "try_make_connections(QList<QString>)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_dummy_queue_audio_data(QList<double>)",
                        backend_ref,
                        "dummy_queue_audio_data(QList<double>)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_dummy_request_data(::std::int32_t)",
                        backend_ref,
                        "dummy_request_data(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_dummy_queue_midi_msgs(QList<QVariant>)",
                        backend_ref,
                        "dummy_queue_midi_msgs(QList<QVariant>)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_dummy_clear_queues()",
                        backend_ref,
                        "dummy_clear_queues()",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_backend(QObject*)",
                        backend_ref,
                        "set_backend(QObject*)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_name_hint(QString)",
                        backend_ref,
                        "set_name_hint(QString)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_input_connectability(::std::int32_t)",
                        backend_ref,
                        "set_input_connectability(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_output_connectability(::std::int32_t)",
                        backend_ref,
                        "set_output_connectability(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_is_internal(bool)",
                        backend_ref,
                        "set_is_internal(bool)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_internal_port_connections(QList<QVariant>)",
                        backend_ref,
                        "set_internal_port_connections(QList<QVariant>)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_min_n_ringbuffer_samples(::std::int32_t)",
                        backend_ref,
                        "set_min_n_ringbuffer_samples(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_audio_gain(double)",
                        backend_ref,
                        "push_audio_gain(double)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_muted(bool)",
                        backend_ref,
                        "push_muted(bool)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_passthrough_muted(bool)",
                        backend_ref,
                        "push_passthrough_muted(bool)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_is_midi(bool)",
                        backend_ref,
                        "set_is_midi(bool)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_fx_chain(QObject*)",
                        backend_ref,
                        "set_fx_chain(QObject*)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_fx_chain_port_idx(::std::int32_t)",
                        backend_ref,
                        "set_fx_chain_port_idx(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                }
                {
                    // Connections: backend object -> GUI
                    connect_or_report(
                        backend_ref,
                        "state_changed(bool,QString,bool,bool,double,double,double,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t)",
                        self_ref,
                        "backend_state_changed(bool,QString,bool,bool,double,double,double,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t)",
                        connection_types::QUEUED_CONNECTION
                    );
                }

                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.backend_port_wrapper =
                    QSharedPointer_QObject::from_ptr_delete_later(backend_port_qobj).unwrap();
            }
        }
    }

    pub unsafe fn backend_state_changed(
        mut self: Pin<&mut PortGui>,
        initialized: bool,
        name: QString,
        muted: bool,
        passthrough_muted: bool,
        audio_gain: f64,
        audio_input_peak: f64,
        audio_output_peak: f64,
        midi_n_input_events: i32,
        midi_n_output_events: i32,
        midi_n_input_notes_active: i32,
        midi_n_output_notes_active: i32,
        n_ringbuffer_samples: i32,
    ) {
        if initialized != self.initialized {
            debug!(self, "initialized -> {initialized}");
            self.as_mut().rust_mut().initialized = initialized;
            unsafe {
                self.as_mut().initialized_changed(initialized);
            }
        }
        if name != self.name {
            debug!(self, "name -> {name:?}");
            self.as_mut().rust_mut().name = name.clone();
            unsafe {
                self.as_mut().name_changed(name);
            }
        }
        if muted != self.muted {
            debug!(self, "muted -> {muted}");
            self.as_mut().rust_mut().muted = muted;
            unsafe {
                self.as_mut().muted_changed(muted);
            }
        }
        if passthrough_muted != self.passthrough_muted {
            debug!(self, "muted -> {passthrough_muted}");
            self.as_mut().rust_mut().passthrough_muted = passthrough_muted;
            unsafe {
                self.as_mut().passthrough_muted_changed(passthrough_muted);
            }
        }
        if audio_gain != self.audio_gain {
            trace!(self, "gain -> {audio_gain}");
            self.as_mut().rust_mut().audio_gain = audio_gain;
            unsafe {
                self.as_mut().audio_gain_changed(audio_gain);
            }
        }
        if audio_input_peak != self.audio_input_peak {
            trace!(self, "input peak -> {audio_input_peak}");
            self.as_mut().rust_mut().audio_input_peak = audio_input_peak;
            unsafe {
                self.as_mut().audio_input_peak_changed(audio_input_peak);
            }
        }
        if audio_output_peak != self.audio_output_peak {
            trace!(self, "output peak -> {audio_output_peak}");
            self.as_mut().rust_mut().audio_output_peak = audio_output_peak;
            unsafe {
                self.as_mut().audio_output_peak_changed(audio_output_peak);
            }
        }
        if midi_n_input_events != self.midi_n_input_events {
            trace!(self, "n input events -> {midi_n_input_events}");
            self.as_mut().rust_mut().midi_n_input_events = midi_n_input_events;
            unsafe {
                self.as_mut()
                    .midi_n_input_events_changed(midi_n_input_events);
            }
        }
        if midi_n_output_events != self.midi_n_output_events {
            trace!(self, "n output events -> {midi_n_output_events}");
            self.as_mut().rust_mut().midi_n_output_events = midi_n_output_events;
            unsafe {
                self.as_mut()
                    .midi_n_output_events_changed(midi_n_output_events);
            }
        }
        if midi_n_input_notes_active != self.midi_n_input_notes_active {
            trace!(self, "n input notes active -> {midi_n_input_notes_active}");
            self.as_mut().rust_mut().midi_n_input_notes_active = midi_n_input_notes_active;
            unsafe {
                self.as_mut()
                    .midi_n_input_notes_active_changed(midi_n_input_notes_active);
            }
        }
        if midi_n_output_notes_active != self.midi_n_output_notes_active {
            trace!(
                self,
                "n output notes active -> {midi_n_output_notes_active}"
            );
            self.as_mut().rust_mut().midi_n_output_notes_active = midi_n_output_notes_active;
            unsafe {
                self.as_mut()
                    .midi_n_output_notes_active_changed(midi_n_output_notes_active);
            }
        }
        if n_ringbuffer_samples != self.n_ringbuffer_samples {
            trace!(self, "n ringbuffer samples -> {n_ringbuffer_samples}");
            self.as_mut().rust_mut().n_ringbuffer_samples = n_ringbuffer_samples;
            unsafe {
                self.as_mut()
                    .n_ringbuffer_samples_changed(n_ringbuffer_samples);
            }
        }
    }

    pub fn display_name(self: &PortGui) -> String {
        if self.name.len() > 0 {
            self.name.to_string()
        } else if self.name_hint.len() > 0 {
            self.name_hint.to_string()
        } else {
            "unknown".to_string()
        }
    }
    pub fn connect_external_port(self: Pin<&mut PortGui>, name: QString) {
        unsafe {
            self.backend_connect_external_port(name);
        }
    }

    pub fn disconnect_external_port(self: Pin<&mut PortGui>, name: QString) {
        unsafe {
            self.backend_disconnect_external_port(name);
        }
    }

    pub fn get_connections_state(self: Pin<&mut PortGui>) -> QMap_QString_QVariant {
        match || -> Result<QMap_QString_QVariant, anyhow::Error> {
            let backend_wrapper = if self.backend_port_wrapper.is_null() {
                return Err(anyhow::anyhow!("backend not initialized"));
            } else {
                self.backend_port_wrapper.data()?
            };
            unsafe {
                Ok(invokable::invoke(
                    &mut *backend_wrapper,
                    "get_connections_state()",
                    invokable::BLOCKING_QUEUED_CONNECTION,
                    &(),
                )?)
            }
        }() {
            Ok(data) => data,
            Err(e) => {
                error!(self, "Could not dequeue audio data: {e}");
                QMap::default()
            }
        }
    }

    pub fn get_connected_external_ports(self: Pin<&mut PortGui>) -> QList_QString {
        match || -> Result<QList_QString, anyhow::Error> {
            let backend_wrapper = self.backend_port_wrapper.data()?;
            unsafe {
                Ok(invokable::invoke(
                    &mut *backend_wrapper,
                    "get_connected_external_ports()",
                    invokable::BLOCKING_QUEUED_CONNECTION,
                    &(),
                )?)
            }
        }() {
            Ok(data) => data,
            Err(e) => {
                error!(self, "Could not dequeue audio data: {e}");
                QList::default()
            }
        }
    }

    pub fn try_make_connections(self: Pin<&mut PortGui>, connections: QList_QString) {
        unsafe {
            self.backend_try_make_connections(connections);
        }
    }

    pub unsafe fn set_backend(mut self: Pin<&mut PortGui>, backend: *mut QObject) {
        self.as_mut().backend_set_backend(backend);
        if backend != self.backend {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.backend = backend;
            unsafe {
                self.as_mut().backend_changed(backend);
            }
        }
    }

    pub fn set_name_hint(mut self: Pin<&mut PortGui>, name_hint: QString) {
        unsafe {
            self.as_mut().backend_set_name_hint(name_hint.clone());
        }
        if name_hint != self.name_hint {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.name_hint = name_hint.clone();
            unsafe {
                self.as_mut().name_hint_changed(name_hint);
            }
        }
    }

    pub fn set_input_connectability(mut self: Pin<&mut PortGui>, input_connectability: i32) {
        unsafe {
            self.as_mut()
                .backend_set_input_connectability(input_connectability);
        }
        if input_connectability != self.input_connectability {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.input_connectability = input_connectability;
            unsafe {
                self.as_mut()
                    .input_connectability_changed(input_connectability);
            }
        }
    }

    pub fn set_output_connectability(mut self: Pin<&mut PortGui>, output_connectability: i32) {
        unsafe {
            self.as_mut()
                .backend_set_output_connectability(output_connectability);
        }
        if output_connectability != self.output_connectability {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.output_connectability = output_connectability;
            unsafe {
                self.as_mut()
                    .output_connectability_changed(output_connectability);
            }
        }
    }

    pub fn set_is_internal(mut self: Pin<&mut PortGui>, is_internal: bool) {
        unsafe {
            self.as_mut().backend_set_is_internal(is_internal);
        }
        if is_internal != self.is_internal {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.is_internal = is_internal;
            unsafe {
                self.as_mut().is_internal_changed(is_internal);
            }
        }
    }

    pub unsafe fn set_is_midi(mut self: Pin<&mut PortGui>, is_midi: bool) {
        unsafe {
            self.as_mut().backend_set_is_midi(is_midi);
        }
        if is_midi != self.is_midi {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.is_midi = is_midi;
            unsafe {
                self.as_mut().is_midi_changed(is_midi);
            }
        }
    }

    pub unsafe fn set_fx_chain(mut self: Pin<&mut PortGui>, fx_chain: *mut QObject) {
        unsafe {
            if fx_chain.is_null() {
                trace!(self, "set backend fx chain -> {fx_chain:?}");
                self.as_mut().backend_set_fx_chain(std::ptr::null_mut());
            } else {
                let backend_chain: *mut QObject = invokable::invoke(
                    &mut *fx_chain,
                    "get_backend_fx_chain()",
                    invokable::DIRECT_CONNECTION,
                    &(),
                )
                .unwrap();
                trace!(self, "set backend fx chain -> {backend_chain:?}");
                self.as_mut().backend_set_fx_chain(backend_chain);
            }
        }
        if fx_chain != self.maybe_fx_chain {
            debug!(self, "fx chain -> {fx_chain:?}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.maybe_fx_chain = fx_chain;
            unsafe {
                self.as_mut().fx_chain_changed(fx_chain);
            }
        }
    }

    pub unsafe fn set_fx_chain_port_idx(mut self: Pin<&mut PortGui>, fx_chain_port_idx: i32) {
        unsafe {
            trace!(self, "set backend fx chain port idx -> {fx_chain_port_idx}");
            self.as_mut()
                .backend_set_fx_chain_port_idx(fx_chain_port_idx);
        }
        if fx_chain_port_idx != self.fx_chain_port_idx {
            trace!(self, "fx chain port idx -> {fx_chain_port_idx}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.fx_chain_port_idx = fx_chain_port_idx;
            unsafe {
                self.as_mut().fx_chain_port_idx_changed(fx_chain_port_idx);
            }
        }
    }

    pub fn set_internal_port_connections(
        mut self: Pin<&mut PortGui>,
        internal_port_connections: QList_QVariant,
    ) {
        unsafe {
            // Store a list of QVariants which hold QSharedPointer instances so that the backend
            // loops don't go out of scope waiting for our operation to complete
            let mut backend_port_handles: QList_QVariant = QList::default();
            match internal_port_connections
                .iter()
                .try_for_each(|v| -> Result<(), anyhow::Error> {
                    let other = qvariant_to_qobject_ptr(v)?;
                    let other = PortGui::from_qobject_mut_ptr(other)?;
                    let other_backend = other
                        .backend_port_wrapper
                        .as_ref()
                        .ok_or(anyhow::anyhow!("Other backend wrapper not set"))?;
                    let other_backend_copy = qsharedpointer_qobject_to_qvariant(other_backend)?;
                    backend_port_handles.append(other_backend_copy);
                    Ok(())
                }) {
                Ok(()) => {
                    self.as_mut()
                        .backend_set_internal_port_connections(backend_port_handles);
                }
                Err(e) => {
                    error!(self, "Failed to get other loop backend handle: {e}");
                }
            }
        }
        if internal_port_connections != self.internal_port_connections {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.internal_port_connections = internal_port_connections.clone();
            unsafe {
                self.as_mut()
                    .internal_port_connections_changed(internal_port_connections);
            }
        }
    }

    pub fn push_audio_gain(self: Pin<&mut PortGui>, audio_gain: f64) {
        unsafe {
            debug!(self, "push gain -> {audio_gain}");
            self.backend_set_audio_gain(audio_gain as f64);
        }
    }

    pub fn push_muted(self: Pin<&mut PortGui>, muted: bool) {
        unsafe {
            debug!(self, "push muted -> {muted}");
            self.backend_set_muted(muted);
        }
    }

    pub fn push_passthrough_muted(self: Pin<&mut PortGui>, muted: bool) {
        unsafe {
            debug!(self, "push passthrough muted -> {muted}");
            self.backend_set_passthrough_muted(muted);
        }
    }

    pub fn set_min_n_ringbuffer_samples(mut self: Pin<&mut PortGui>, n_ringbuffer_samples: i32) {
        unsafe {
            self.as_mut()
                .backend_set_min_n_ringbuffer_samples(n_ringbuffer_samples);
        }
        if n_ringbuffer_samples != self.min_n_ringbuffer_samples {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.min_n_ringbuffer_samples = n_ringbuffer_samples;
            unsafe {
                self.as_mut()
                    .min_n_ringbuffer_samples_changed(n_ringbuffer_samples);
            }
        }
    }

    pub fn dummy_queue_audio_data(self: Pin<&mut PortGui>, audio_data: QList_f64) {
        unsafe {
            self.backend_dummy_queue_audio_data(audio_data);
        }
    }

    pub fn dummy_dequeue_audio_data(self: Pin<&mut PortGui>, n: i32) -> QList_f64 {
        match || -> Result<QList_f64, anyhow::Error> {
            let backend_wrapper = self.backend_port_wrapper.data()?;
            unsafe {
                Ok(invokable::invoke(
                    &mut *backend_wrapper,
                    "dummy_dequeue_audio_data(::std::int32_t)",
                    invokable::BLOCKING_QUEUED_CONNECTION,
                    &(n),
                )?)
            }
        }() {
            Ok(data) => data,
            Err(e) => {
                error!(self, "Could not dequeue audio data: {e}");
                QList::default()
            }
        }
    }

    pub fn dummy_request_data(self: Pin<&mut PortGui>, n: i32) {
        unsafe {
            self.backend_dummy_request_data(n);
        }
    }

    pub fn dummy_queue_midi_msgs(self: Pin<&mut PortGui>, midi_msgs: QList_QVariant) {
        unsafe {
            self.backend_dummy_queue_midi_msgs(midi_msgs);
        }
    }

    pub fn dummy_dequeue_midi_msgs(self: Pin<&mut PortGui>) -> QList_QVariant {
        match || -> Result<QList_QVariant, anyhow::Error> {
            let backend_wrapper = self.backend_port_wrapper.data()?;
            unsafe {
                Ok(invokable::invoke::<_, QList_QVariant, ()>(
                    &mut *backend_wrapper,
                    "dummy_dequeue_midi_msgs()",
                    invokable::BLOCKING_QUEUED_CONNECTION,
                    &(),
                )?)
            }
        }() {
            Ok(data) => data,
            Err(e) => {
                error!(self, "Could not dequeue midi messages: {e}");
                QList::default()
            }
        }
    }

    pub fn dummy_clear_queues(self: Pin<&mut PortGui>) {
        unsafe {
            self.backend_dummy_clear_queues();
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_port_gui(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
