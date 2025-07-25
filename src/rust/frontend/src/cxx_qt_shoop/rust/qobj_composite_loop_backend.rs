use crate::{
    composite_loop_schedule::CompositeLoopSchedule,
    cxx_qt_shoop::qobj_composite_loop_backend_bridge::ffi::*,
    loop_mode_helpers::{is_recording_mode, is_running_mode},
};
use backend_bindings::LoopMode;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::{
    connect::connect_or_report, connection_types, invokable::invoke, qobject::{ffi::qobject_property_int, AsQObject}, qvariant_qobject::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr}
};
use core::sync;
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

fn get_loop_iid(l: &*mut QObject) -> String {
    todo!();
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
                self.as_mut().next_mode_changed(next_mode);
            }
        }
    }

    pub fn set_next_transition_delay(mut self: Pin<&mut Self>, next_transition_delay: i32) {
        if next_transition_delay != self.next_transition_delay {
            debug!(self, "next transition delay -> {next_transition_delay}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.next_transition_delay = next_transition_delay;
            unsafe {
                self.as_mut().next_transition_delay_changed(next_transition_delay);
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
                self.as_mut().sync_position_changed(v);
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
                self.as_mut().sync_length_changed(v);
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
                self.as_mut().length_changed(length);
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
                self.as_mut().position_changed(v);
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
                unsafe { self.as_mut().n_cycles_changed(highest_iteration) };
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
            self.play_after_record_changed(play_after_record);
        }
    }

    pub unsafe fn set_sync_mode_active(mut self: Pin<&mut Self>, sync_mode_active: bool) {
        debug!(self, "sync mode active -> {sync_mode_active}");
        if sync_mode_active != self.sync_mode_active {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.sync_mode_active = sync_mode_active;
            self.sync_mode_active_changed(sync_mode_active);
        }
    }

    pub unsafe fn set_kind(mut self: Pin<&mut Self>, kind: QString) {
        let dbg = kind.to_string();
        debug!(self, "kind -> {dbg}");
        if kind != self.kind {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.kind = kind.clone();
            self.kind_changed(kind);
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

    pub fn do_trigger<AlternativeTriggerCallback>(
        mut self: Pin<&mut CompositeLoopBackend>,
        loop_obj: *mut QObject,
        mode: LoopMode,
        callback: Option<AlternativeTriggerCallback>,
    ) where
        AlternativeTriggerCallback: Fn(*mut QObject, LoopMode),
    {
        if let Some(callback) = callback.as_ref() {
            callback(loop_obj, mode);
        } else {
            let self_qobj = unsafe { self.as_mut().pin_mut_qobject_ptr() };
            let loop_iid = get_loop_iid(&loop_obj);
            if loop_obj == self_qobj {
                // Instead of queueing, apply the transition immediately
                trace!(self, "Transition self to {mode:?}");
                self.as_mut().transition(mode as i32, -1, -1);
            } else {
                // When triggering another loop, delay it by one event loop cycle.
                // this ensures that with nested composites, the composites always
                // trigger themselves first, then get triggered by others.
                // That is good because them being controlled by other loops should
                // take precedence.
                trace!(self, "Queue transition {loop_iid} to {mode:?}");
                todo!();
            }
        }
    }

    pub fn do_triggers<AlternativeTriggerCallback>(
        mut self: Pin<&mut Self>,
        iteration: i32,
        mode: LoopMode,
        trigger_callback: Option<AlternativeTriggerCallback>,
        nested: bool,
    ) where
        AlternativeTriggerCallback: Fn(*mut QObject, LoopMode) + Copy,
    {
        if let Err(err) = || -> Result<(), anyhow::Error> {
            let schedule = self.schedule.clone();
            let kind = self.kind.to_string();

            debug!(
                self,
                "{kind} composite loop - do triggers ({iteration}, {mode:?})"
            );

            if schedule.data.contains_key(&iteration) {
                // Some triggers need to be executed for this iteration
                // First clone some of our state data to operate on.
                let events = &schedule.data[&iteration];
                let loops_start = &events.loops_start;
                let loops_end = &events.loops_end;
                let mut running_loops: HashSet<*mut QObject> = self
                    .as_mut()
                    .running_loops
                    .iter()
                    .map(|variant| {
                        qvariant_to_qobject_ptr(variant)
                            .ok_or(anyhow::anyhow!("Could not convert QVariant to object"))
                    })
                    .collect::<Result<HashSet<_>, _>>()?;
                let mut running_loops_changed = false;

                // Handle any loop that needs to end this iteration.
                for loop_end in loops_end.iter() {
                    let loop_iid = get_loop_iid(loop_end);
                    debug!(self, "loop end: {loop_iid}");
                    self.as_mut()
                        .do_trigger(*loop_end, LoopMode::Stopped, trigger_callback);
                    running_loops_changed = running_loops.remove(loop_end);
                }

                if is_running_mode(mode) {
                    // Our new mode will be a running mode. Apply it to
                    // loops that start this iteration.
                    for (loop_start, maybe_explicit_mode) in loops_start.iter() {
                        let loop_iid = get_loop_iid(loop_start);
                        if let Some(explicit_mode) = maybe_explicit_mode {
                            // Explicit mode, just apply it as scheduled
                            debug!(
                                self,
                                "generate loop start (explicit mode {explicit_mode:?}): {loop_iid}"
                            );
                            self.as_mut()
                                .do_trigger(*loop_start, *explicit_mode, trigger_callback);
                            running_loops_changed = running_loops.insert(*loop_start);
                        } else {
                            // Implicit mode, set it based on our own
                            let implicit_mode = mode;
                            if is_recording_mode(mode) {
                                // Recording is a special case. During one composite loop iteration,
                                // the same loop may be played more than once.
                                // That means that if we are set to "record" the composite loop, it
                                // would be recorded more than once, which makes no sense.
                                // In this case, we want the first time the sub-loop is played to be
                                // its recording, then the loop should be ignored for the rest of the
                                // recording iteration of the composite loop.
                                // The recorded sub-loop will start playing back on the next composite
                                // loop iteration.

                                // Check whether we have already recorded.
                                for i in 0..iteration {
                                    if schedule.data.contains_key(&i) {
                                        let earlier_loop_starts = &schedule.data[&i].loops_start;
                                        if earlier_loop_starts.contains_key(loop_start) {
                                            // We have already recorded this loop.
                                            debug!(
                                                self,
                                                "Not re-recording {loop_iid}, stopping instead"
                                            );
                                            self.as_mut().do_trigger(
                                                *loop_start,
                                                LoopMode::Stopped,
                                                trigger_callback,
                                            );
                                            running_loops_changed =
                                                running_loops.remove(loop_start);
                                            break;
                                        }
                                    }
                                }
                            } else {
                                // Explicit mode, just apply it as scheduled
                                let loop_iid = get_loop_iid(loop_start);
                                debug!(self, "generate loop start (implicit mode {implicit_mode:?}): {loop_iid}");
                                self.as_mut().do_trigger(
                                    *loop_start,
                                    implicit_mode,
                                    trigger_callback,
                                );
                                running_loops_changed = running_loops.insert(*loop_start);
                            }
                        }
                    }
                }

                if running_loops_changed {
                    let mut new_running_loops: QList_QVariant = QList_QVariant::default();
                    for l in running_loops.iter() {
                        let loop_variant = qobject_ptr_to_qvariant(*l);
                        new_running_loops.append(loop_variant);
                    }
                    let mut rust_mut = self.as_mut().rust_mut();
                    rust_mut.running_loops = new_running_loops.clone();
                    unsafe {
                        self.as_mut().running_loops_changed(new_running_loops);
                    }
                }
            }

            // Now, check if we are ending our composite loop this iteration.
            if iteration >= self.n_cycles && !nested {
                let self_qobj = unsafe { self.as_mut().pin_mut_qobject_ptr() };
                let next_mode = LoopMode::try_from(self.next_mode)?;
                let self_mode = LoopMode::try_from(self.mode)?;
                debug!(self, "Extra trigger for cycle end");
                if self.kind.to_string() == "script"
                    && !(self.next_transition_delay >= 0 && is_running_mode(next_mode))
                {
                    debug!(self, "Ending script");
                    self.as_mut().do_trigger(self_qobj, LoopMode::Stopped, trigger_callback);
                }
                else if is_recording_mode(self_mode) {
                    debug!(self, "Ending recording");
                    // At end of recording cycle, transition to playing or stopped
                    let new_mode = if self.play_after_record { LoopMode::Playing } else { LoopMode::Stopped };
                    self.as_mut().do_trigger(self_qobj, new_mode, trigger_callback);
                } else {
                    // Just cycle around
                    let self_mode = LoopMode::try_from(self.mode)?;
                    debug!(self, "cycling");
                    self.as_mut().do_triggers(0, self_mode, trigger_callback, true);
                }
            }

            Ok(())
        }() {
            error!(self, "Could not perform triggers: {err}");
        }
    }
}
