use crate::{
    composite_loop_schedule::CompositeLoopSchedule,
    cxx_qt_shoop::qobj_composite_loop_backend_bridge::ffi::*,
    loop_helpers::transition_backend_loops,
    loop_mode_helpers::{is_recording_mode, is_running_mode},
};
use backend_bindings::LoopMode;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace, warn as raw_warn,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    invokable::invoke,
    qobject::{
        ffi::{qobject_property_int, qobject_property_qobject, qobject_property_string},
        qobject_has_property, AsQObject,
    },
    qvariant_qobject::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr},
};
use std::{
    cmp::{max, min},
    collections::{BTreeMap, HashMap, HashSet},
    pin::Pin,
};
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
macro_rules! warn {
    ($self:ident, $($arg:tt)*) => {
        raw_warn!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*));
    };
}

#[allow(unused_macros)]
macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*));
    };
}

fn get_loop_iid(l: &*mut QObject) -> String {
    unsafe {
        match qobject_property_string(&**l, "instance_identifier".to_string()) {
            Ok(iid) => {
                return iid.to_string();
            }
            Err(e) => {
                raw_error!("Could not get instance identifier: {e}");
                return "error-unknown".to_string();
            }
        }
    }
}

type Transition = (*mut QObject, LoopMode);
type Transitions = Vec<Transition>;
type TransitionsPerIteration = BTreeMap<i32, Transitions>;

impl CompositeLoopBackend {
    pub fn initialize_impl(self: Pin<&mut Self>) {}

    pub fn list_transitions(
        mut self: Pin<&mut Self>,
        mode: LoopMode,
        start_cycle: i32,
        end_cycle: i32,
    ) -> TransitionsPerIteration {
        let mut transitions: TransitionsPerIteration = BTreeMap::new();

        // Step through the transitions up to the given iteration.
        let until = self.schedule.data.last_key_value().unwrap().0 + 1;
        for i in start_cycle..until {
            let mut iteration_transitions: Transitions = Transitions::default();
            self.as_mut().do_triggers_with_callback(
                i,
                mode,
                |obj: *mut QObject, obj_mode: LoopMode| {
                    iteration_transitions.push((obj, obj_mode));
                },
            );
            transitions.insert(i, iteration_transitions);
            if i >= end_cycle {
                break;
            }
        }

        transitions
    }

    pub fn transition_multiple(
        self: Pin<&mut CompositeLoopBackend>,
        loops: QList_QVariant,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        if let Err(e) = transition_backend_loops(
            loops
                .iter()
                .map(|variant| qvariant_to_qobject_ptr(variant).unwrap()),
            LoopMode::try_from(to_mode).unwrap(),
            if maybe_cycles_delay < 0 {
                None
            } else {
                Some(maybe_cycles_delay)
            },
            if maybe_to_sync_at_cycle < 0 {
                None
            } else {
                Some(maybe_to_sync_at_cycle)
            },
        ) {
            error!(self, "Failed to transition backend loops: {e}");
        }
    }

    pub fn transition_with_immediate_sync(
        mut self: Pin<&mut Self>,
        to_mode: i32,
        sync_at_cycle: i32,
    ) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let all_transitions =
                self.as_mut()
                    .list_transitions(LoopMode::try_from(to_mode)?, 0, sync_at_cycle);

            debug!(
                self,
                "immediate sync transition - stepping through virtual transition list"
            );
            trace!(self, "virtual transition list: {all_transitions:?}");

            // Find the last transition for each loop up until this point
            type LastTransitionPerLoop = HashMap<*mut QObject, (LoopMode, i32)>;
            let mut last_transition_per_loop: LastTransitionPerLoop = HashMap::new();
            for (iteration, transitions) in all_transitions.iter() {
                for (loop_obj, mode) in transitions.iter() {
                    last_transition_per_loop.insert(*loop_obj, (*mode, *iteration));
                }
            }

            // Get all currently active loops into their correct mode and cycle
            for (loop_obj, (mode, iteration)) in last_transition_per_loop.iter() {
                let n_cycles_ago = sync_at_cycle - iteration;
                let mut n_cycles = 1;

                unsafe {
                    if qobject_has_property(&**loop_obj, "sync_source".to_string())? {
                        let sync_source =
                            qobject_property_qobject(&**loop_obj, "sync_source".to_string())?;
                        let loop_length = qobject_property_int(&**loop_obj, "length".to_string())?;
                        let sync_source_length =
                            qobject_property_int(&*sync_source, "length".to_string())?;
                        n_cycles = (loop_length as f64 / sync_source_length as f64).ceil() as i32;
                    }

                    let sync_to_immediate_cycle: Option<i32> = match mode {
                        LoopMode::Stopped => None,
                        LoopMode::Recording => Some(n_cycles_ago), // Keep going indefinitely
                        _ => Some(n_cycles_ago % max(n_cycles, 1)), // Loop around
                    };

                    let iid = get_loop_iid(loop_obj);
                    trace!(self, "loop {iid} -> {mode:?}, goto cycle {sync_to_immediate_cycle:?} (triggered {n_cycles_ago} cycles ago, loop length {n_cycles} cycles)");
                    let sync_to_immediate_cycle = sync_to_immediate_cycle.unwrap_or(-1);
                    invoke(
                        &mut **loop_obj,
                        "transition(::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
                        connection_types::DIRECT_CONNECTION,
                        &(*mode as isize as i32, -1 as i32, sync_to_immediate_cycle),
                    )?;
                }
            }

            // Apply our own mode change
            self.as_mut().set_mode(to_mode);
            self.as_mut().set_iteration(sync_at_cycle);
            self.as_mut().update_length();
            self.as_mut().update_position();
            let position = self.position;
            trace!(self, "Immediate sync done. mode -> {to_mode}, iteration -> {sync_at_cycle}, position -> {position}");

            // Trigger(s) for next loop cycle
            let iteration = self.iteration;
            self.as_mut()
                .do_triggers(iteration + 1, LoopMode::try_from(to_mode)?);

            Ok(())
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
                trace!(self, "Starting to run - resetting iteration to -1");
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

            if next_transition_delay == 0 {
                if is_running_mode(to_mode) {
                    self.as_mut().do_triggers(0, to_mode);
                } else {
                    self.as_mut().cancel_all();
                }
            }

            Ok(())
        }() {
            error!(self, "Could not perform transition: {e}");
        }
    }

    pub fn cancel_all(self: Pin<&mut Self>) {
        trace!(self, "cancel all");
        if let Err(e) = transition_backend_loops(
            self.running_loops
                .iter()
                .map(|l| qvariant_to_qobject_ptr(l).unwrap()),
            LoopMode::Stopped,
            Some(0),
            None,
        ) {
            error!(self, "Failed to transition backend loops: {e}");
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
                self.as_mut()
                    .next_transition_delay_changed(next_transition_delay);
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
        mut self: Pin<&mut Self>,
        _maybe_reverse_start_cycle: QVariant,
        _maybe_cycles_length: QVariant,
        maybe_go_to_cycle: QVariant,
        go_to_mode: i32,
    ) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            if self.sync_source.is_null() || self.sync_length <= 0 {
                warn!(self, "ignoring grab - undefined / empty sync loop");
                return Ok(());
            }
            let maybe_go_to_cycle_opt: Option<i32> = maybe_go_to_cycle.value::<i32>();
            let go_to_mode = LoopMode::try_from(go_to_mode)?;

            trace!(self, "adopt ringbuffers and go to cycle {maybe_go_to_cycle_opt:?}, go to mode {go_to_mode:?}");

            // Proceed through the schedule up to the point we want to go by
            // calling our trigger function with a callback to just register
            // the made transitions.
            let n_cycles = self.n_cycles;
            let transitions = self.as_mut().list_transitions(LoopMode::Recording, 0, n_cycles);

            trace!(self, "virtual transition list: {transitions:?}");

            // Find the first recording range for each loop.
            type IterationPerLoop = HashMap<*mut QObject, i32>;
            let mut loop_recording_starts = IterationPerLoop::default();
            let mut loop_recording_ends = IterationPerLoop::default();
            for (iteration, transitions) in transitions.iter() {
                for (loop_obj, mode) in transitions.iter() {
                    if *mode == LoopMode::Recording {
                        // Store only the first recording bounds.
                        if !loop_recording_starts.contains_key(loop_obj) {
                            loop_recording_starts.insert(*loop_obj, *iteration);
                        } else {
                            let v = loop_recording_starts.get_mut(loop_obj).unwrap();
                            *v = min(*v, *iteration);
                        }
                    }
                }
            }
            for (iteration, transitions) in transitions.iter() {
                for (loop_obj, mode) in transitions.iter() {
                    if *mode != LoopMode::Recording &&
                        loop_recording_starts.contains_key(loop_obj) &&
                        iteration > loop_recording_starts.get(loop_obj).unwrap()
                    {
                        if !loop_recording_ends.contains_key(loop_obj) {
                            loop_recording_ends.insert(*loop_obj, *iteration);
                        } else {
                            let v = loop_recording_ends.get_mut(loop_obj).unwrap();
                            *v = min(*v, *iteration);
                        }
                    }
                }
            }

            // Determine the grabs to make on our sub-loops.
            #[derive(Debug)]
            struct ToGrab {
                loop_obj: *mut QObject,
                reverse_start: i32,
                n_cycles: i32,
            }
            let mut to_grab: Vec<ToGrab> = Vec::new();
            for (loop_obj, start_it) in loop_recording_starts.iter() {
                let end_it = *loop_recording_ends
                    .get(loop_obj)
                    .unwrap_or(&self.n_cycles.clone());
                let n_cycles = max(end_it - start_it, 1);
                let mut reverse_start = self.n_cycles - start_it;
                if !self.sync_mode_active {
                    // With sync mode inactive, we want to end up inside the
                    // last cycle with our grab.
                    reverse_start = max(reverse_start - 1, 0);
                }
                let g = ToGrab {
                    loop_obj: *loop_obj,
                    reverse_start,
                    n_cycles,
                };
                let iid = get_loop_iid(loop_obj);
                trace!(self, "will grab {iid}: {g:?}");
                to_grab.push(g);
            }

            for g in to_grab.iter() {
                unsafe {
                    // Note we don't allow the loop to directly go to the go_to_mode.
                    // We will instead do that transition after all grabs are done.
                    invoke(
                        &mut *g.loop_obj,
                        "adopt_ringbuffers(QVariant,QVariant,QVariant,::std::int32_t)".to_string(),
                        connection_types::DIRECT_CONNECTION,
                        &(
                            QVariant::from(&g.reverse_start),
                            QVariant::from(&g.n_cycles),
                            QVariant::from(&0),
                            LoopMode::Unknown as isize as i32,
                        )
                    )?;
                }
            }

            if go_to_mode != LoopMode::Unknown {
                self.as_mut().transition(go_to_mode as isize as i32, -1, maybe_go_to_cycle_opt.unwrap_or(-1));
            }

            Ok(())
        }() {
            error!(self, "Could not adopt ringbuffers: {e}");
        }
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
                "cycled(::std::int32_t)".to_string(),
                &*self_qobj,
                "handle_sync_loop_trigger(::std::int32_t)".to_string(),
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
                self.as_mut().update_position();
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
                self.as_mut().update_length();
            }
        }
    }

    pub fn set_cycle_nr(mut self: Pin<&mut Self>, cycle_nr: i32) {
        if cycle_nr != self.cycle_nr {
            debug!(self, "cycle nr -> {cycle_nr}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.cycle_nr = cycle_nr;
            unsafe {
                self.as_mut().cycle_nr_changed(cycle_nr);
            }
        }
    }

    pub fn handle_sync_loop_trigger(mut self: Pin<&mut CompositeLoopBackend>, cycle_nr: i32) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            trace!(self, "handle sync trigger");

            if Some(cycle_nr) == self.last_handled_sync_cycle {
                trace!(self, "already handled sync cycle {cycle_nr}");
                return Ok(());
            }

            unsafe {
                // Before we start, give all of the loops in our schedule a chance
                // to handle the sync cycle first. This ensures a deterministic ordering
                // of events.
                for loop_obj in self.as_mut().all_loops().iter() {
                    invoke::<QObject, (), i32>(
                        &mut **loop_obj,
                        "dependent_will_handle_sync_loop_cycle(::std::int32_t)".to_string(),
                        connection_types::DIRECT_CONNECTION,
                        &(cycle_nr),
                    )?;
                }

                if self.next_transition_delay == 0 {
                    let next_mode = self.next_mode;
                    self.as_mut()
                        .handle_transition(LoopMode::try_from(next_mode)?);
                } else if self.next_transition_delay > 0 {
                    let next_mode = LoopMode::try_from(self.next_mode)?;
                    let next_transition_delay = self.next_transition_delay - 1;
                    self.as_mut()
                        .set_next_transition_delay(next_transition_delay);
                    if next_transition_delay == 0 {
                        self.as_mut().do_triggers(0, next_mode);
                    }
                }

                if is_running_mode(LoopMode::try_from(self.mode)?) {
                    let mut cycled = false;
                    let mut new_iteration = self.iteration + 1;
                    if new_iteration >= self.n_cycles {
                        new_iteration = 0;
                        cycled = true;
                    }
                    self.as_mut().set_iteration(new_iteration);

                    let mode = LoopMode::try_from(self.mode)?;
                    self.as_mut().do_triggers(new_iteration + 1, mode);

                    if cycled {
                        let cycle_nr = self.cycle_nr + 1;
                        self.as_mut().set_cycle_nr(cycle_nr);
                        self.as_mut().cycled(cycle_nr);
                    }
                }
            }

            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.last_handled_sync_cycle = Some(cycle_nr);

            Ok(())
        }() {
            error!(self, "Could not handle sync loop trigger: {e}");
        }
    }

    pub fn set_mode(mut self: Pin<&mut Self>, mode: i32) {
        debug!(self, "mode -> {mode:?}");
        if mode != self.mode {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.mode = mode;
            unsafe {
                self.as_mut().mode_changed(mode);
            }
        }
    }

    pub fn handle_transition(mut self: Pin<&mut Self>, mode: LoopMode) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            debug!(self, "handle transition -> {mode:?}");
            self.as_mut().set_next_transition_delay(-1);
            self.as_mut().set_mode(mode as isize as i32);
            if !is_running_mode(mode) {
                self.as_mut().set_iteration(0);
            }
            Ok(())
        }() {
            error!(self, "Could not handle transition: {e}");
        }
    }

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
                    trace!(self, "schedule: {converted_schedule:?}");
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
        let mut extended: QString = instance_identifier.clone();
        extended.append(&QString::from("-backend"));
        debug!(self, "instance identifier -> {extended:?}");
        if extended != self.instance_identifier {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.instance_identifier = extended.clone();
            unsafe {
                self.instance_identifier_changed(extended);
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
        mut callback: Option<&mut AlternativeTriggerCallback>,
    ) where
        AlternativeTriggerCallback: FnMut(*mut QObject, LoopMode),
    {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            if let Some(callback) = callback.as_mut() {
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
                    unsafe {
                        trace!(self, "Queue transition {loop_iid} to {mode:?}");
                        invoke(
                            &mut *loop_obj,
                            "transition(::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
                            connection_types::QUEUED_CONNECTION,
                            &(mode as isize as i32, 0, -1),
                        )?;
                    }
                }
            }

            Ok(())
        }() {
            error!(self, "Could not perform trigger: {e}");
        }
    }

    pub fn do_triggers(self: Pin<&mut Self>, iteration: i32, mode: LoopMode) {
        type Callback = fn(*mut QObject, LoopMode);
        self.do_triggers_impl::<Callback>(iteration, mode, None, false);
    }

    pub fn do_triggers_with_callback<TriggerCallback>(
        self: Pin<&mut Self>,
        iteration: i32,
        mode: LoopMode,
        mut callback: TriggerCallback,
    ) where
        TriggerCallback: FnMut(*mut QObject, LoopMode),
    {
        self.do_triggers_impl::<TriggerCallback>(iteration, mode, Some(&mut callback), false);
    }

    pub fn do_triggers_impl<AlternativeTriggerCallback>(
        mut self: Pin<&mut Self>,
        iteration: i32,
        mode: LoopMode,
        mut trigger_callback: Option<&mut AlternativeTriggerCallback>,
        nested: bool,
    ) where
        AlternativeTriggerCallback: FnMut(*mut QObject, LoopMode),
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
                    self.as_mut().do_trigger(
                        *loop_end,
                        LoopMode::Stopped,
                        trigger_callback.as_mut(),
                    );
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
                                "loop start (explicit mode {explicit_mode:?}): {loop_iid}"
                            );
                            self.as_mut().do_trigger(
                                *loop_start,
                                *explicit_mode,
                                trigger_callback.as_mut(),
                            );
                            running_loops_changed = running_loops.insert(*loop_start);
                        } else {
                            // Implicit mode, set it based on our own
                            let implicit_mode = mode;

                            let determine_already_recorded = || -> bool {
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
                                            return true;
                                        }
                                    }
                                }
                                return false;
                            };

                            let handled_already_recording =
                                is_recording_mode(mode) && determine_already_recorded();
                            if handled_already_recording {
                                // We have already recorded this loop.
                                debug!(self, "Not re-recording {loop_iid}, stopping instead");
                                self.as_mut().do_trigger(
                                    *loop_start,
                                    LoopMode::Stopped,
                                    trigger_callback.as_mut(),
                                );
                                running_loops_changed = running_loops.remove(loop_start);
                            } else {
                                // Implicit mode, apply it
                                let loop_iid = get_loop_iid(loop_start);
                                debug!(self, "generate loop start (implicit mode {implicit_mode:?}): {loop_iid}");
                                self.as_mut().do_trigger(
                                    *loop_start,
                                    implicit_mode,
                                    trigger_callback.as_mut(),
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
                let next_mode: Option<LoopMode> = LoopMode::try_from(self.next_mode).ok();
                let next_mode_is_running_mode =
                    next_mode.is_some() && is_running_mode(next_mode.unwrap());
                let self_mode = LoopMode::try_from(self.mode)?;
                debug!(self, "Extra trigger for cycle end");
                if self.kind.to_string() == "script"
                    && !(self.next_transition_delay >= 0 && next_mode_is_running_mode)
                {
                    debug!(self, "Ending script");
                    self.as_mut()
                        .do_trigger(self_qobj, LoopMode::Stopped, trigger_callback);
                } else if is_recording_mode(self_mode) {
                    debug!(self, "Ending recording");
                    // At end of recording cycle, transition to playing or stopped
                    let new_mode = if self.play_after_record {
                        LoopMode::Playing
                    } else {
                        LoopMode::Stopped
                    };
                    self.as_mut()
                        .do_trigger(self_qobj, new_mode, trigger_callback);
                } else {
                    // Just cycle around
                    let self_mode = LoopMode::try_from(self.mode)?;
                    debug!(self, "cycling");
                    self.as_mut()
                        .do_triggers_impl(0, self_mode, trigger_callback, true);
                }
            }

            Ok(())
        }() {
            error!(self, "Could not perform triggers: {err:?}");
        }
    }

    pub fn metatype_name() -> String {
        unsafe { composite_loop_backend_metatype_name(std::ptr::null_mut()).unwrap() }
    }

    pub fn dependent_will_handle_sync_loop_cycle(
        self: Pin<&mut CompositeLoopBackend>,
        cycle_nr: i32,
    ) {
        // Another loop which references this loop (composite) can notify this loop that it is
        // about to handle a sync loop cycle in advance, to ensure a deterministic ordering.
        self.handle_sync_loop_trigger(cycle_nr);
    }
}
