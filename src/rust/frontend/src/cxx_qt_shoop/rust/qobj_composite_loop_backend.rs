use crate::{
    composite_loop_schedule::CompositeLoopSchedule,
    cxx_qt_shoop::qobj_composite_loop_backend_bridge::ffi::*, loop_mode_helpers::is_running_mode,
};
use backend_bindings::LoopMode;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    qobject::{ffi::qobject_property_int, AsQObject},
};
use std::{cmp::max, collections::HashSet, hash::Hash, pin::Pin};
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

    pub fn list_transitions(
        self: Pin<&mut Self>,
        mode: LoopMode,
        start_cycle: i32,
        end_cycle: i32,
    ) {
        todo!();
    }

    pub fn transition_with_immediate_sync(self: Pin<&mut Self>, to_mode: i32, sync_at_cycle: i32) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            todo!();
        }() {
            error!(
                self,
                "Could not perform transition with immediate sync: {e}"
            );
        }
    }

    pub fn transition_regular(mut self: Pin<&mut Self>, to_mode: i32, delay: Option<i32>) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let our_mode = LoopMode::try_from(self.mode)?;
            let to_mode = LoopMode::try_from(to_mode)?;

            let iteration = if !is_running_mode(our_mode) && is_running_mode(to_mode) {
                -1
            } else {
                self.iteration
            };
            let next_transition_delay = delay.unwrap_or(0);
            let next_mode = to_mode;

            self.as_mut().set_iteration(iteration);
            self.as_mut().set_next_mode(next_mode as isize as i32);
            self.as_mut()
                .set_next_transition_delay(next_transition_delay);

            Ok(())
        }() {
            error!(self, "Could not perform transition: {e}");
        }
    }

    pub fn set_iteration(mut self: Pin<&mut Self>, iteration: i32) {
        if iteration != self.iteration {
            debug!(self, "iteration -> {iteration}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.iteration = iteration;
            unsafe {
                self.as_mut().iteration_changed(iteration);
            }
        }
    }

    pub fn set_next_mode(mut self: Pin<&mut Self>, next_mode: i32) {
        if next_mode != self.next_mode {
            debug!(self, "next mode -> {next_mode}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.next_mode = next_mode;
            unsafe {
                self.as_mut().next_mode_changed();
            }
        }
    }

    pub fn set_next_transition_delay(mut self: Pin<&mut Self>, next_transition_delay: i32) {
        if next_transition_delay != self.next_transition_delay {
            debug!(self, "next transition delay -> {next_transition_delay}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.next_transition_delay = next_transition_delay;
            unsafe {
                self.as_mut().next_transition_delay_changed();
            }
        }
    }

    pub fn transition(
        self: Pin<&mut Self>,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        debug!(self, "transition -> {to_mode}: wait {maybe_cycles_delay:?}, align @ {maybe_to_sync_at_cycle:?}");
        let maybe_cycles_delay = match maybe_cycles_delay {
            v if v < 0 => None,
            other => Some(other),
        };
        let maybe_to_sync_at_cycle = match maybe_to_sync_at_cycle {
            v if v < 0 => None,
            other => Some(other),
        };
        if maybe_to_sync_at_cycle.is_some() {
            self.transition_with_immediate_sync(to_mode, maybe_to_sync_at_cycle.unwrap());
        } else {
            self.transition_regular(to_mode, maybe_cycles_delay);
        }
    }

    pub fn adopt_ringbuffers(
        self: Pin<&mut Self>,
        maybe_reverse_start_cycle: QVariant,
        maybe_cycles_length: QVariant,
        maybe_go_to_cycle: QVariant,
        go_to_mode: i32,
    ) {
    }

    fn all_loops(self: &Self) -> HashSet<*mut QObject> {
        let mut result: HashSet<*mut QObject> = HashSet::new();
        for (iteration, events) in self.schedule.data.iter() {
            for (l, _mode) in events.loops_start.iter() {
                result.insert(*l);
            }
            for l in events.loops_end.iter().chain(events.loops_ignored.iter()) {
                result.insert(*l);
            }
        }
        result
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
            self.as_mut().update_sync_position();
            self.as_mut().update_sync_length();
            self.sync_source_changed(sync_source);
        }
    }

    pub fn update_sync_position(mut self: Pin<&mut CompositeLoopBackend>) {
        trace!(self, "update sync position");
        let mut v = 0;
        unsafe {
            if !self.sync_source.is_null() {
                match qobject_property_int(&*self.sync_source, "position".to_string()) {
                    Ok(pos) => {
                        v = pos;
                    }
                    Err(e) => {
                        error!(self, "Unable to get sync loop position: {e}");
                    }
                }
            }
        }
        if v != self.sync_position {
            trace!(self, "sync position -> {v}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.sync_position = v;
            unsafe {
                self.as_mut().sync_position_changed();
            }
        }
    }

    pub fn update_sync_length(mut self: Pin<&mut CompositeLoopBackend>) {
        trace!(self, "update sync length");
        let mut v = 0;
        unsafe {
            if !self.sync_source.is_null() {
                match qobject_property_int(&*self.sync_source, "length".to_string()) {
                    Ok(l) => {
                        v = l;
                    }
                    Err(e) => {
                        error!(self, "Unable to get sync loop length: {e}");
                    }
                }
            }
        }
        if v != self.sync_length {
            trace!(self, "sync length -> {v}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.sync_length = v;
            unsafe {
                self.as_mut().sync_length_changed();
            }
        }
    }

    pub fn handle_sync_loop_trigger(self: Pin<&mut CompositeLoopBackend>, cycle_nr: i32) {}

    pub unsafe fn set_backend(mut self: Pin<&mut Self>, backend: *mut QObject) {
        debug!(self, "set backend -> {backend:?}");
        if !backend.is_null() {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.initialized = true;
            self.initialized_changed(true);
        }
    }

    pub fn update_length(mut self: Pin<&mut Self>) {
        trace!(self, "update length");
        let length = self.sync_length * self.n_cycles;
        if length != self.length {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.length = length;
            unsafe {
                self.as_mut().length_changed();
            }
        }
    }

    pub fn update_position(mut self: Pin<&mut CompositeLoopBackend>) {
        trace!(self, "update position");
        let mut v = max(0, self.iteration) * self.sync_length;
        if is_running_mode(LoopMode::try_from(self.mode).unwrap()) {
            v += self.sync_position;
        }
        if v != self.position {
            trace!(self, "position -> {v}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.position = v;
            unsafe {
                self.as_mut().position_changed();
            }
        }
    }

    pub fn update_n_cycles(mut self: Pin<&mut Self>) {
        let highest_iteration = *self.schedule.data.keys().max().unwrap_or(&0);
        if self.n_cycles != highest_iteration {
            debug!(self, "n cycles -> {highest_iteration}");
            {
                let self_mut = self.as_mut();
                let mut rust_mut = self_mut.rust_mut();
                rust_mut.n_cycles = highest_iteration;
                unsafe { self.as_mut().n_cycles_changed() };
            }
            self.as_mut().update_length();
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

    pub fn do_triggers(
        self: Pin<&mut Self>,
        iteration: i32,
        mode: LoopMode,
        trigger_callback: Option<bool>,
        nested: bool,
    ) {
        let schedule = &self.schedule;
        let kind = self.kind.to_string();
        debug!(self, "{kind} composite loop - do triggers ({iteration}, {mode:?})")
    }
}
