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

    let get_backend_obj = |object: *mut QObject| -> Result<*mut QObject, anyhow::Error> {
        unsafe {
            match invoke::<QObject, *mut QObject, ()>(
                &mut *object,
                "get_backend_loop_wrapper()".to_string(),
                connection_types::DIRECT_CONNECTION,
                &(),
            ) {
                Ok(backend_loop) => {
                    if backend_loop.is_null() {
                        return Err(anyhow::anyhow!("Backend loop in schedule is null"));
                    }
                    return Ok(backend_loop);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Unable to get backend loop: {e}"));
                }
            }
        }
    };

    let mut rval = CompositeLoopSchedule::default();

    for (key, events) in schedule.data.iter() {
        let mut new_events = CompositeLoopIterationEvents::default();
        for (object, mode) in events.loops_start.iter() {
            let backend_loop = get_backend_obj(*object)?;
            new_events.loops_start.insert(backend_loop, *mode);
        }
        for object in events.loops_end.iter() {
            let backend_loop = get_backend_obj(*object)?;
            new_events.loops_end.insert(backend_loop);
        }
        for object in events.loops_ignored.iter() {
            let backend_loop = get_backend_obj(*object)?;
            new_events.loops_ignored.insert(backend_loop);
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
                        "nCyclesChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_n_cycles_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "syncLengthChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_sync_length_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "iterationChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_iteration_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "modeChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_mode_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "syncPositionChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_sync_position_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "nextModeChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_next_mode_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "nextTransitionDelayChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_next_transition_delay_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "runningLoopsChanged(QList<QVariant>)".to_string(),
                        self_ref,
                        "on_backend_running_loops_changed(QList<QVariant>)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "lengthChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_length_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "positionChanged(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_position_changed(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "initializedChanged(bool)".to_string(),
                        self_ref,
                        "on_backend_initialized_changed(bool)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        backend_ref,
                        "cycled(::std::int32_t)".to_string(),
                        self_ref,
                        "on_backend_cycled(::std::int32_t)".to_string(),
                        connection_types::QUEUED_CONNECTION,
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

    pub fn transition(
        self: Pin<&mut Self>,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        self.backend_transition(to_mode, maybe_cycles_delay, maybe_to_sync_at_cycle);
    }

    pub fn adopt_ringbuffers(
        self: Pin<&mut Self>,
        maybe_reverse_start_cycle: QVariant,
        maybe_cycles_length: QVariant,
        maybe_go_to_cycle: QVariant,
        go_to_mode: i32,
    ) {
        self.backend_adopt_ringbuffers(maybe_reverse_start_cycle, maybe_cycles_length, maybe_go_to_cycle, go_to_mode);
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

    pub fn set_instance_identifier(mut self: Pin<&mut Self>, instance_identifier: QString) {
        let dbg = instance_identifier.to_string();
        debug!(self, "queue set instance identifier -> {dbg}");
        if instance_identifier != self.instance_identifier {
            self.as_mut().rust_mut().instance_identifier = instance_identifier.clone();
            unsafe { self.as_mut().instance_identifier_changed(instance_identifier.clone()); }
        }
        self.backend_set_instance_identifier(instance_identifier);
    }

    pub fn get_backend_loop_wrapper(self: Pin<&mut Self>) -> *mut QObject {
        self.backend_loop_wrapper
            .data()
            .unwrap_or(std::ptr::null_mut())
    }

    pub fn on_backend_n_cycles_changed(mut self: Pin<&mut CompositeLoopGui>, n_cycles: i32) {
        trace!(self, "backend n cycles -> {n_cycles}");
        let mut rust_mut = self.as_mut().rust_mut();
        if n_cycles != rust_mut.n_cycles {
            rust_mut.n_cycles = n_cycles;
            unsafe {
                self.n_cycles_changed();
            }
        }
    }

    pub fn on_backend_sync_length_changed(mut self: Pin<&mut CompositeLoopGui>, sync_length: i32) {
        trace!(self, "backend sync length -> {sync_length}");
        let mut rust_mut = self.as_mut().rust_mut();
        if sync_length != rust_mut.sync_length {
            rust_mut.sync_length = sync_length;
            unsafe {
                self.sync_length_changed();
            }
        }
    }

    pub fn on_backend_iteration_changed(mut self: Pin<&mut CompositeLoopGui>, iteration: i32) {
        trace!(self, "backend iteration -> {iteration}");
        let mut rust_mut = self.as_mut().rust_mut();
        if iteration != rust_mut.iteration {
            rust_mut.iteration = iteration;
            unsafe {
                self.iteration_changed(iteration);
            }
        }
    }

    pub fn on_backend_mode_changed(mut self: Pin<&mut CompositeLoopGui>, mode: i32) {
        trace!(self, "backend mode -> {mode}");
        let mut rust_mut = self.as_mut().rust_mut();
        if mode != rust_mut.mode {
            rust_mut.mode = mode;
            unsafe {
                self.mode_changed();
            }
        }
    }

    pub fn on_backend_sync_position_changed(
        mut self: Pin<&mut CompositeLoopGui>,
        sync_position: i32,
    ) {
        trace!(self, "backend sync position -> {sync_position}");
        let mut rust_mut = self.as_mut().rust_mut();
        if sync_position != rust_mut.sync_position {
            rust_mut.sync_position = sync_position;
            unsafe {
                self.sync_position_changed();
            }
        }
    }

    pub fn on_backend_next_mode_changed(mut self: Pin<&mut CompositeLoopGui>, next_mode: i32) {
        trace!(self, "backend next mode -> {next_mode}");
        let mut rust_mut = self.as_mut().rust_mut();
        if next_mode != rust_mut.next_mode {
            rust_mut.next_mode = next_mode;
            unsafe {
                self.next_mode_changed();
            }
        }
    }

    pub fn on_backend_next_transition_delay_changed(
        mut self: Pin<&mut CompositeLoopGui>,
        next_transition_delay: i32,
    ) {
        trace!(self, "backend next transition delay -> {next_transition_delay}");
        let mut rust_mut = self.as_mut().rust_mut();
        if next_transition_delay != rust_mut.next_transition_delay {
            rust_mut.next_transition_delay = next_transition_delay;
            unsafe {
                self.next_transition_delay_changed();
            }
        }
    }

    pub fn on_backend_running_loops_changed(
        mut self: Pin<&mut CompositeLoopGui>,
        running_loops: QList_QVariant,
    ) {
        trace!(self, "backend running loops changed");
        let mut rust_mut = self.as_mut().rust_mut();
        if running_loops != rust_mut.running_loops {
            rust_mut.running_loops = running_loops.clone();
            unsafe {
                self.running_loops_changed();
            }
        }
    }

    pub fn on_backend_length_changed(mut self: Pin<&mut CompositeLoopGui>, length: i32) {
        trace!(self, "backend length -> {length}");
        let mut rust_mut = self.as_mut().rust_mut();
        if length != rust_mut.length {
            rust_mut.length = length;
            unsafe{ self.length_changed(); }
        }
    }

    pub fn on_backend_position_changed(mut self: Pin<&mut CompositeLoopGui>, position: i32) {
        trace!(self, "backend position -> {position}");
        let mut rust_mut = self.as_mut().rust_mut();
        if position != rust_mut.position {
            rust_mut.position = position;
            unsafe{ self.position_changed(); }
        }
    }

    pub fn on_backend_cycled(self: Pin<&mut CompositeLoopGui>, cycle_nr: i32) {
        trace!(self, "backend cycled -> {cycle_nr}");
        self.cycled(cycle_nr);
    }

    pub fn on_backend_initialized_changed(mut self: Pin<&mut Self>, initialized: bool) {
        debug!(self, "backend initialized -> {initialized}");
        let mut rust_mut = self.as_mut().rust_mut();
        if initialized != rust_mut.initialized {
            rust_mut.initialized = initialized;
            unsafe{ self.initialized_changed(initialized); }
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let obj = make_unique_composite_loop_gui();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_composite_loop_gui(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}
