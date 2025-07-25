use crate::composite_loop_schedule::CompositeLoopIterationEvents;
use crate::composite_loop_schedule::CompositeLoopSchedule;
use crate::cxx_qt_shoop::qobj_composite_loop_backend_bridge::make_raw_composite_loop_backend;
use crate::cxx_qt_shoop::qobj_composite_loop_gui_bridge::ffi::*;
use crate::engine_update_thread;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::connect::connect_or_report;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::invokable::invoke;
use cxx_qt_lib_shoop::qobject::QObject;
use cxx_qt_lib_shoop::qobject::*;
use cxx_qt_lib_shoop::qsharedpointer_qobject::*;
use cxx_qt_lib_shoop::qvariant_qobject::qobject_ptr_to_qvariant;
use cxx_qt_lib_shoop::qvariant_qsharedpointer_qobject::qsharedpointer_qobject_to_qvariant;
use std::pin::Pin;
shoop_log_unit!("Frontend.CompositeLoop");

#[allow(unused_macros)]
macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*));
    };
}

#[allow(unused_macros)]
macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*));
    };
}

#[allow(unused_macros)]
macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*));
    };
}

fn replace_by_backend_objects(
    schedule: &CompositeLoopSchedule,
) -> Result<CompositeLoopSchedule, anyhow::Error> {
    let mut rval = CompositeLoopSchedule::default();

    for (key, events) in schedule.data.iter() {
        let mut new_events = CompositeLoopIterationEvents::default();
        for (object, mode) in events.loops_start.iter() {
            unsafe {
                match invoke::<QObject, *mut QObject, ()>(
                    &mut **object,
                    "get_backend_loop_wrapper()".to_string(),
                    connection_types::DIRECT_CONNECTION,
                    &(),
                ) {
                    Ok(backend_loop) => {
                        if backend_loop.is_null() {
                            return Err(anyhow::anyhow!("Backend loop in schedule is null"));
                        }
                        new_events.loops_start.insert(backend_loop, *mode);
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("Unable to get backend loop: {e}"));
                    }
                }
            }
        }
        rval.data.insert(*key, new_events);
    }

    Ok(rval)
}

use crate::cxx_qt_shoop::qobj_composite_loop_backend_bridge::ffi::composite_loop_backend_qobject_from_ptr;

impl CompositeLoopGui {
    pub fn initialize_impl(mut self: Pin<&mut Self>) {
        debug!(self, "initializing");

        unsafe {
            let backend_loop = make_raw_composite_loop_backend();
            let backend_loop_qobj = composite_loop_backend_qobject_from_ptr(backend_loop);
            qobject_move_to_thread(
                backend_loop_qobj,
                engine_update_thread::get_engine_update_thread().thread,
            )
            .unwrap();

            {
                let backend_loop_pin = std::pin::Pin::new_unchecked(&mut *backend_loop);
                let self_qobject = self.as_mut().pin_mut_qobject_ptr();
                backend_loop_pin.set_frontend_loop(self_qobject);
            }

            let self_ref = self.as_ref().get_ref();

            {
                let backend_ref = &*backend_loop_qobj;
                let backend_thread_wrapper =
                    &*engine_update_thread::get_engine_update_thread().ref_qobject_ptr();

                {
                    // Connections : update thread -> backend object
                    connect_or_report(
                        backend_thread_wrapper,
                        "update()".to_string(),
                        backend_ref,
                        "update()".to_string(),
                        connection_types::DIRECT_CONNECTION,
                    );
                }
                {
                    // Connections: backend object -> GUI
                    connect_or_report(
                        backend_ref,
                        "stateChanged(::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,QList<QVariant>)".to_string(),
                        self_ref,
                        "on_backend_state_changed(::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,QList<QVariant>)".to_string(),
                        connection_types::QUEUED_CONNECTION
                    );
                }
                {
                    // Connections: GUI -> backend object
                    connect_or_report(
                        self_ref,
                        "backend_clear()".to_string(),
                        backend_ref,
                        "clear()".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_kind(QString)".to_string(),
                        backend_ref,
                        "set_kind(QString)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_schedule(QMap<QString,QVariant>)".to_string(),
                        backend_ref,
                        "set_schedule(QMap<QString,QVariant>)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_play_after_record(bool)".to_string(),
                        backend_ref,
                        "set_play_after_record(bool)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_sync_mode_active(bool)".to_string(),
                        backend_ref,
                        "set_sync_mode_active(bool)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_transition(::std::int32_t,::std::int32_t,::std::int32_t)"
                            .to_string(),
                        backend_ref,
                        "transition(::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_adopt_ringbuffers(QVariant,QVariant,QVariant,::std::int32_t)"
                            .to_string(),
                        backend_ref,
                        "adopt_ringbuffers(QVariant,QVariant,QVariant,::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backendChanged(QObject*)".to_string(),
                        backend_ref,
                        "set_backend(QObject*)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "instanceIdentifierChanged(QString)".to_string(),
                        backend_ref,
                        "set_instance_identifier(QString)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_sync_source(QObject*)".to_string(),
                        backend_ref,
                        "set_sync_source(QObject*)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                }
            }

            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.backend_loop_wrapper =
                QSharedPointer_QObject::from_ptr_delete_later(backend_loop_qobj).unwrap();
        }
    }

    pub fn get_schedule(self: &CompositeLoopGui) -> QMap_QString_QVariant {
        self.schedule.to_qvariantmap()
    }

    pub fn on_backend_state_changed(
        self: Pin<&mut Self>,
        mode: i32,
        length: i32,
        position: i32,
        next_mode: i32,
        next_transition_delay: i32,
        cycle_nr: i32,
        sync_position: i32,
        sync_length: i32,
        iteration: i32,
        running_loops: QList_QVariant,
    ) {
    }

    pub fn transition(
        self: Pin<&mut Self>,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
    }

    pub fn adopt_ringbuffers(
        self: Pin<&mut Self>,
        maybe_reverse_start_cycle: QVariant,
        maybe_cycles_length: QVariant,
        maybe_go_to_cycle: QVariant,
        go_to_mode: i32,
    ) {
    }

    pub fn update_backend_sync_source(mut self: Pin<&mut Self>) {
        debug!(self, "Updating backend sync source");
        let sync_source_in: *mut QObject = *self.sync_source();

        unsafe {
            match invoke::<QObject, *mut QObject, ()>(
                &mut *sync_source_in,
                "get_backend_loop_wrapper()".to_string(),
                connection_types::DIRECT_CONNECTION,
                &(),
            ) {
                Ok(backend_loop) => {
                    if backend_loop.is_null() {
                        error!(self, "Backend loop in sync source is null");
                    }
                    self.as_mut().backend_set_sync_source(backend_loop);
                }
                Err(e) => {
                    error!(self, "Unable to get backend loop: {e}");
                }
            }
        }
    }

    pub unsafe fn set_sync_source(mut self: Pin<&mut Self>, sync_source: *mut QObject) {
        debug!(self, "sync source -> {:?}", sync_source);
        let changed = self.as_mut().rust_mut().sync_source != sync_source;
        self.as_mut().rust_mut().sync_source = sync_source;

        if changed {
            self.as_mut().sync_source_changed(sync_source);
        }

        self.update_backend_sync_source();
    }

    pub unsafe fn set_backend(mut self: Pin<&mut Self>, backend: *mut QObject) {
        debug!(self, "backend -> {:?}", backend);
        let changed = self.as_mut().rust_mut().backend != backend;
        self.as_mut().rust_mut().backend = backend;

        if changed {
            self.as_mut().backend_changed(backend);
        }
    }

    pub unsafe fn set_schedule(mut self: Pin<&mut Self>, schedule: QMap_QString_QVariant) {
        let self_mut = self.as_mut();
        let mut rust_mut = self_mut.rust_mut();
        match CompositeLoopSchedule::from_qvariantmap(&schedule) {
            Ok(schedule) => {
                let changed = rust_mut.schedule != schedule;
                if changed {
                    match replace_by_backend_objects(&schedule) {
                        Ok(backend_schedule) => {
                            let backend_schedule = backend_schedule.to_qvariantmap();
                            rust_mut.schedule = schedule;
                            self.backend_set_schedule(backend_schedule);
                        }
                        Err(e) => {
                            error!(self, "Could not get backend schedule: {e:?}");
                        }
                    }
                }
            }
            Err(err) => {
                error!(self, "set_schedule failed with {:?}", err);
            }
        }
    }

    pub unsafe fn set_play_after_record(self: Pin<&mut Self>, play_after_record: bool) {
        debug!(self, "queue set play after record -> {play_after_record}");
        self.backend_set_play_after_record(play_after_record);
    }

    pub unsafe fn set_sync_mode_active(self: Pin<&mut Self>, sync_mode_active: bool) {
        debug!(self, "queue set sync mode active -> {sync_mode_active}");
        self.backend_set_sync_mode_active(sync_mode_active);
    }

    pub unsafe fn set_kind(self: Pin<&mut Self>, kind: QString) {
        let dbg = kind.to_string();
        debug!(self, "queue set kind -> {dbg}");
        self.backend_set_kind(kind);
    }

    pub fn set_instance_identifier(self: Pin<&mut Self>, instance_identifier: QString) {
        let dbg = instance_identifier.to_string();
        debug!(self, "queue set instance identifier -> {dbg}");
        self.backend_set_instance_identifier(instance_identifier);
    }

    pub fn get_backend_loop_wrapper(self: Pin<&mut Self>) -> *mut QObject {
        self.backend_loop_wrapper
            .data()
            .unwrap_or(std::ptr::null_mut())
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let obj = make_unique_composite_loop_gui();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_composite_loop_gui(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}
