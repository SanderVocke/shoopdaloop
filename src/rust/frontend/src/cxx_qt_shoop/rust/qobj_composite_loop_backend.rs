use crate::{
    composite_loop_schedule::CompositeLoopSchedule,
    cxx_qt_shoop::qobj_composite_loop_backend_bridge::ffi::*,
};
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::{connect::connect_or_report, connection_types, qobject::AsQObject};
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

impl CompositeLoopBackend {
    pub fn initialize_impl(mut self: Pin<&mut Self>) {}

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

    pub unsafe fn set_sync_source(mut self: Pin<&mut Self>, sync_source: *mut QObject) {
        debug!(self, "set sync source -> {sync_source:?}");
        if sync_source != self.sync_source {
            if !self.sync_source.is_null() {
                error!(self, "cannot change sync source");
                return;
            }
            let self_qobj = self.as_mut().pin_mut_qobject_ptr();
            let self_mut = self.as_mut();
            let mut rust_mut = self_mut.rust_mut();

            rust_mut.sync_source = sync_source;
            connect_or_report(
                &*sync_source,
                "positionChanged(::std::int32_t,::std::int32_t)".to_string(),
                &*self_qobj,
                "update_sync_position()".to_string(),
                connection_types::DIRECT_CONNECTION,
            );
            connect_or_report(
                &*sync_source,
                "lengthChanged(::std::int32_t,::std::int32_t)".to_string(),
                &*self_qobj,
                "update_sync_length()".to_string(),
                connection_types::DIRECT_CONNECTION,
            );
            connect_or_report(
                &*sync_source,
                "cycled(int)".to_string(),
                &*self_qobj,
                "handle_sync_loop_trigger(int)".to_string(),
                connection_types::DIRECT_CONNECTION,
            );
            self.sync_source_changed(sync_source);
        }
    }

    pub fn update_sync_position(self: Pin<&mut CompositeLoopBackend>) {}

    pub fn update_sync_length(self: Pin<&mut CompositeLoopBackend>) {}

    pub fn handle_sync_loop_trigger(self: Pin<&mut CompositeLoopBackend>, cycle_nr: i32) {}

    pub unsafe fn set_backend(mut self: Pin<&mut Self>, backend: *mut QObject) {
        debug!(self, "set backend -> {backend:?}");
        if !backend.is_null() {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.initialized = true;
            self.initialized_changed(true);
        }
    }

    pub fn update_n_cycles(mut self: Pin<&mut Self>) {
        let highest_iteration = *self.schedule.data.keys().max().unwrap_or(&0);
        if self.n_cycles != highest_iteration {
            debug!(self, "n_cycles -> {highest_iteration}");
            let self_mut = self.as_mut();
            let mut rust_mut = self_mut.rust_mut();
            rust_mut.n_cycles = highest_iteration;
            unsafe { self.n_cycles_changed() };
        }
    }

    pub unsafe fn set_schedule(mut self: Pin<&mut Self>, schedule: QMap_QString_QVariant) {
        match CompositeLoopSchedule::from_qvariantmap(&schedule) {
            Ok(converted_schedule) => {
                if converted_schedule != self.schedule {
                    debug!(self, "schedule updated");
                    let self_mut = self.as_mut();
                    let mut rust_mut = self_mut.rust_mut();
                    rust_mut.schedule = converted_schedule;
                    self.as_mut().schedule_changed(schedule);
                    self.as_mut().update_n_cycles();
                }
            }
            Err(e) => {
                error!(self, "Could not convert incoming schedule: {e}");
            }
        }
    }

    pub unsafe fn set_play_after_record(mut self: Pin<&mut Self>, play_after_record: bool) {
        debug!(self, "play after record -> {play_after_record}");
        if play_after_record != self.play_after_record {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.play_after_record = play_after_record;
            self.play_after_record_changed();
        }
    }

    pub unsafe fn set_sync_mode_active(mut self: Pin<&mut Self>, sync_mode_active: bool) {
        debug!(self, "sync mode active -> {sync_mode_active}");
        if sync_mode_active != self.sync_mode_active {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.sync_mode_active = sync_mode_active;
            self.sync_mode_active_changed();
        }
    }

    pub unsafe fn set_kind(mut self: Pin<&mut Self>, kind: QString) {
        let dbg = kind.to_string();
        debug!(self, "kind -> {dbg}");
        if kind != self.kind {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.kind = kind;
            self.kind_changed();
        }
    }

    pub fn set_instance_identifier(mut self: Pin<&mut Self>, instance_identifier: QString) {
        let dbg = instance_identifier.to_string();
        debug!(self, "instance identifier -> {dbg}");
        if instance_identifier != self.instance_identifier {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.instance_identifier = instance_identifier.clone();
            unsafe {
                self.instance_identifier_changed(instance_identifier);
            };
        }
    }

    pub fn update(self: Pin<&mut Self>) {}

    pub fn get_schedule(self: &CompositeLoopBackend) -> QMap_QString_QVariant {
        self.schedule.to_qvariantmap()
    }

    pub fn clear(self: Pin<&mut CompositeLoopBackend>) {}
}
