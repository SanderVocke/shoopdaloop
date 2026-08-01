use crate::{
    composite_loop_schedule::CompositeLoopSchedule,
    cxx_qt_shoop::qobj_backend_wrapper::BackendWrapper,
    cxx_qt_shoop::qobj_composite_loop_backend_bridge::ffi::*,
    cxx_qt_shoop::qobj_loop_backend_bridge::ffi::qobject_to_loop_backend_ptr,
    loop_helpers::transition_backend_loops,
    loop_mode_helpers::{is_recording_mode, is_running_mode},
    references_qobject::ReferencesQObject,
};
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace, warn as raw_warn,
};
use cxx_qt::CxxQtType;
use cxx_qt::QObject;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    qobject::{self, AsQObject, FromQObject},
    qvariant_helpers::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr},
};
use shoop_engine::{
    AudioRingbufferAdoption, CompositeEntry, CompositePlanDescriptor, CompositeSection,
    CompositeTimeline as EngineCompositeTimeline, LoopIdentity, LoopMode, LoopTargetKind,
    LoopTargetMetadata,
};
use std::{
    cmp::{max, min},
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    pin::Pin,
};
shoop_log_unit!("Frontend.CompositeLoop");

#[allow(unused_macros)]
macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*))
    };
}

#[allow(unused_macros)]
macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*))
    };
}

#[allow(unused_macros)]
macro_rules! warn {
    ($self:ident, $($arg:tt)*) => {
        raw_warn!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*))
    };
}

#[allow(unused_macros)]
macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*))
    };
}

type Transition = (*mut QObject, LoopMode);
type Transitions = Vec<Transition>;
type TransitionsPerIteration = BTreeMap<i32, Transitions>;
type PreparedAdoptions = BTreeMap<LoopIdentity, (LoopIdentity, AudioRingbufferAdoption)>;

unsafe fn engine_identity(obj: *mut QObject) -> Option<LoopIdentity> {
    if obj.is_null() {
        return None;
    }
    let basic = qobject_to_loop_backend_ptr(obj);
    if !basic.is_null() {
        return (&*basic)
            .rust()
            .backend_loop
            .as_ref()
            .and_then(|backend| backend.identity_if_ready());
    }
    let composite = qobject_to_composite_loop_backend_ptr(obj);
    if !composite.is_null() {
        return (&*composite)
            .rust()
            .engine_loop
            .as_ref()
            .and_then(|backend| backend.identity_if_ready());
    }
    None
}

unsafe fn engine_target(obj: *mut QObject) -> Option<(LoopIdentity, u64)> {
    if obj.is_null() {
        return None;
    }
    let basic = qobject_to_loop_backend_ptr(obj);
    if !basic.is_null() {
        let backend = (&*basic).rust().backend_loop.as_ref()?;
        let identity = backend.identity_if_ready()?;
        let length = backend.get_state().ok()?.length as u64;
        return Some((identity, length));
    }
    let composite = qobject_to_composite_loop_backend_ptr(obj);
    if !composite.is_null() {
        let backend = (&*composite).rust().engine_loop.as_ref()?;
        let identity = backend.identity_if_ready()?;
        let length = backend.get_state().ok()?.length;
        return Some((identity, length));
    }
    None
}

impl CompositeLoopBackend {
    pub fn initialize_impl(self: Pin<&mut Self>) {}

    fn install_engine_schedule(mut self: Pin<&mut Self>) -> Result<(), anyhow::Error> {
        if !self.engine_schedule_dirty || self.engine_schedule_install.is_some() {
            return Ok(());
        }
        if self.engine_schedule_installing {
            return Err(anyhow::anyhow!("recursive composite schedule dependency"));
        }
        self.as_mut().rust_mut().engine_schedule_installing = true;
        let result = self.as_mut().install_engine_schedule_impl();
        self.as_mut().rust_mut().engine_schedule_installing = false;
        result
    }

    fn install_engine_schedule_impl(mut self: Pin<&mut Self>) -> Result<(), anyhow::Error> {
        let session = self
            .backend_session
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("engine session is not initialized"))?
            .clone();
        let composite = self
            .engine_loop
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("engine composite is not initialized"))?
            .clone();
        let composite_identity = composite
            .identity_if_ready()
            .ok_or_else(|| anyhow::anyhow!("engine composite creation is still pending"))?;
        let (sync_source, sync_length) = unsafe {
            engine_target(self.sync_source)
                .ok_or_else(|| anyhow::anyhow!("composite sync source is not engine-backed"))?
        };
        let mut metadata = BTreeMap::new();
        metadata.insert(
            composite_identity,
            LoopTargetMetadata {
                identity: composite_identity,
                length_samples: self.length.max(0) as u64,
            },
        );
        metadata.insert(
            sync_source,
            LoopTargetMetadata {
                identity: sync_source,
                length_samples: sync_length,
            },
        );
        let mut entries = Vec::new();
        for (&start, events) in &self.schedule.data {
            for (target, explicit_mode) in &events.loops_start {
                let target_obj = target.obj.as_qobject_ref() as *mut QObject;
                unsafe {
                    let dependency = qobject_to_composite_loop_backend_ptr(target_obj);
                    let this = self.as_ref().get_ref() as *const Self as *mut Self;
                    if !dependency.is_null() && dependency != this {
                        Pin::new_unchecked(&mut *dependency).install_engine_schedule()?;
                    }
                }
                let Some((identity, length_samples)) = (unsafe { engine_target(target_obj) })
                else {
                    return Err(anyhow::anyhow!(
                        "composite schedule target is not engine-backed"
                    ));
                };
                metadata.insert(
                    identity,
                    LoopTargetMetadata {
                        identity,
                        length_samples,
                    },
                );
                let end = self
                    .schedule
                    .data
                    .range(start.saturating_add(1)..)
                    .find_map(|(&iteration, events)| {
                        events.loops_end.contains(target).then_some(iteration)
                    })
                    .unwrap_or(self.n_cycles.max(start.saturating_add(1)));
                entries.push(CompositeEntry {
                    target: identity,
                    delay: i64::from(start.max(0)),
                    n_cycles: Some(i64::from((end - start).max(1))),
                    mode: *explicit_mode,
                });
            }
        }
        let descriptor = CompositePlanDescriptor {
            source: composite_identity,
            sync_length,
            timelines: vec![EngineCompositeTimeline {
                sections: vec![CompositeSection { entries }],
            }],
        };
        let primitive_sync_sources = session
            .primitive_sync_sources_if_ready()
            .ok_or_else(|| anyhow::anyhow!("primitive loop topology is still pending"))?;
        let install = session.configure_composite_loop_queued(
            &composite,
            descriptor,
            sync_source,
            metadata.into_values().collect(),
            &primitive_sync_sources,
            self.play_after_record,
        )?;
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.engine_schedule_install = Some(install);
            rust_mut.engine_schedule_dirty = false;
        }
        Ok(())
    }

    pub fn list_transitions(
        self: Pin<&mut Self>,
        mode: LoopMode,
        start_cycle: i32,
        end_cycle: i32,
    ) -> TransitionsPerIteration {
        let mut transitions = TransitionsPerIteration::new();
        let mut previously_started = HashSet::new();

        for (&iteration, events) in self.schedule.data.range(start_cycle..=end_cycle) {
            let mut iteration_transitions = Transitions::new();
            iteration_transitions.extend(events.loops_end.iter().filter_map(|target| {
                let object = target.obj.as_qobject_ref() as *mut QObject;
                (!object.is_null()).then_some((object, LoopMode::Stopped))
            }));
            iteration_transitions.extend(events.loops_start.iter().filter_map(
                |(target, explicit_mode)| {
                    let object = target.obj.as_qobject_ref() as *mut QObject;
                    if object.is_null() {
                        return None;
                    }
                    let target_mode = explicit_mode.unwrap_or_else(|| {
                        if is_recording_mode(mode) && previously_started.contains(&object) {
                            LoopMode::Stopped
                        } else {
                            mode
                        }
                    });
                    previously_started.insert(object);
                    Some((object, target_mode))
                },
            ));
            transitions.insert(iteration, iteration_transitions);
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
        let loops_iter = loops
            .iter()
            .map(|variant| qvariant_to_qobject_ptr(variant))
            .collect::<Result<Vec<*mut QObject>, _>>();

        if let Err(e) = loops_iter {
            error!(self, "Failed to extract loop pointers for transition: {e}");
            return;
        }
        let loop_ptrs = match loops_iter {
            Ok(ptrs) => ptrs,
            Err(e) => {
                error!(self, "Failed to extract loop pointers for transition: {e}");
                return;
            }
        };

        let mode = match LoopMode::try_from(to_mode) {
            Ok(m) => m,
            Err(e) => {
                error!(self, "Invalid loop mode {to_mode}: {e}");
                return;
            }
        };

        if let Err(e) = transition_backend_loops(
            loop_ptrs.into_iter(),
            mode,
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
        mut self: Pin<&mut Self>,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        debug!(self, "transition -> {to_mode}: wait {maybe_cycles_delay:?}, align @ {maybe_to_sync_at_cycle:?}");
        let Some(engine_loop) = self.engine_loop.as_ref().cloned() else {
            error!(self, "engine composite is not initialized");
            return;
        };
        if let Err(error) = self.as_mut().install_engine_schedule() {
            error!(self, "engine composite configuration failed: {error}");
            return;
        }
        let result = LoopMode::try_from(to_mode)
            .map_err(anyhow::Error::from)
            .and_then(|mode| {
                if maybe_to_sync_at_cycle >= 0 {
                    engine_loop
                        .transition_immediate(mode, i64::from(maybe_to_sync_at_cycle))
                        .map(|_| ())
                } else {
                    engine_loop
                        .transition(mode, maybe_cycles_delay.max(0) as u32)
                        .map(|_| ())
                }
            });
        if let Err(error) = result {
            error!(self, "engine composite transition failed: {error}");
        }
    }

    fn collect_ringbuffer_adoptions(
        mut self: Pin<&mut Self>,
        adoptions: &mut PreparedAdoptions,
        visited: &mut BTreeSet<LoopIdentity>,
    ) -> Result<(), anyhow::Error> {
        let source = self
            .engine_loop
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("engine composite is not initialized"))?
            .identity_if_ready()
            .ok_or_else(|| anyhow::anyhow!("engine composite creation is still pending"))?;
        if !visited.insert(source) || self.sync_source.is_null() || self.sync_length <= 0 {
            return Ok(());
        }

        let n_cycles = self.n_cycles;
        let transitions = self
            .as_mut()
            .list_transitions(LoopMode::Recording, 0, n_cycles);
        let mut starts = HashMap::<*mut QObject, i32>::new();
        let mut ends = HashMap::<*mut QObject, i32>::new();
        for (&iteration, transitions) in &transitions {
            for &(object, mode) in transitions {
                if mode == LoopMode::Recording {
                    starts
                        .entry(object)
                        .and_modify(|start| *start = min(*start, iteration))
                        .or_insert(iteration);
                } else if starts.get(&object).is_some_and(|start| iteration > *start) {
                    ends.entry(object)
                        .and_modify(|end| *end = min(*end, iteration))
                        .or_insert(iteration);
                }
            }
        }

        let mut nested = Vec::new();
        for (object, start) in starts {
            let Some(identity) = (unsafe { engine_identity(object) }) else {
                continue;
            };
            let end = ends.get(&object).copied().unwrap_or(self.n_cycles);
            let mut reverse_start = self.n_cycles - start;
            if !self.sync_mode_active {
                reverse_start = max(reverse_start - 1, 0);
            }
            if identity.kind == LoopTargetKind::Basic {
                let request = AudioRingbufferAdoption {
                    loop_idx: identity.slot as usize,
                    reverse_start_cycle: Some(reverse_start),
                    cycles_length: Some(max(end - start, 1)),
                    go_to_cycle: Some(0),
                    go_to_mode: LoopMode::Unknown,
                };
                adoptions
                    .entry(identity)
                    .and_modify(|existing| {
                        if source < existing.0 {
                            *existing = (source, request);
                        }
                    })
                    .or_insert((source, request));
            } else {
                nested.push((identity, object));
            }
        }
        nested.sort_unstable_by_key(|(identity, _)| *identity);
        for (_, object) in nested {
            let dependency = unsafe { qobject_to_composite_loop_backend_ptr(object) };
            if dependency.is_null() {
                return Err(anyhow::anyhow!("nested composite target is unavailable"));
            }
            unsafe {
                Pin::new_unchecked(&mut *dependency)
                    .collect_ringbuffer_adoptions(adoptions, visited)?;
            }
        }
        self.as_mut().rust_mut().engine_schedule_dirty = true;
        Ok(())
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
            let maybe_go_to_cycle: Option<i32> = maybe_go_to_cycle.value::<i32>();
            let go_to_mode = LoopMode::try_from(go_to_mode)?;
            let mut prepared = PreparedAdoptions::new();
            self.as_mut()
                .collect_ringbuffer_adoptions(&mut prepared, &mut BTreeSet::new())?;
            if !prepared.is_empty() {
                let requests = prepared.into_values().map(|(_, request)| request).collect();
                self.backend_session
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("engine session is not initialized"))?
                    .adopt_audio_ringbuffers(requests)?;
                self.as_mut().install_engine_schedule()?;
            }
            if go_to_mode != LoopMode::Unknown {
                self.as_mut().transition(
                    go_to_mode as isize as i32,
                    -1,
                    maybe_go_to_cycle.unwrap_or(-1),
                );
            }
            Ok(())
        }() {
            error!(self, "Could not adopt ringbuffers: {e}");
        }
    }

    fn all_loops(self: &Self) -> HashSet<*mut QObject> {
        let mut result: HashSet<*mut QObject> = HashSet::new();
        for (_, events) in self.schedule.data.iter() {
            for (l, _mode) in events.loops_start.iter() {
                let l = l.obj.as_qobject_ref() as *mut QObject;
                if !l.is_null() {
                    result.insert(l);
                }
            }
            for l in events.loops_end.iter().chain(events.loops_ignored.iter()) {
                let l = l.obj.as_qobject_ref() as *mut QObject;
                if !l.is_null() {
                    result.insert(l);
                }
            }
        }
        result
    }

    pub unsafe fn set_sync_source(mut self: Pin<&mut Self>, sync_source: *mut QObject) {
        debug!(self, "set sync source -> {sync_source:?}");
        if sync_source != self.sync_source {
            let self_qobj = self.as_mut().pin_mut_qobject_ptr();
            let self_mut = self.as_mut();
            let mut rust_mut = self_mut.rust_mut();

            rust_mut.sync_source = sync_source;
            rust_mut.engine_schedule_dirty = true;

            if !rust_mut.sync_source.is_null() {
                connect_or_report(
                    &*sync_source,
                    "positionChanged(::std::int32_t,::std::int32_t)",
                    &*self_qobj,
                    "update_sync_position()",
                    connection_types::DIRECT_CONNECTION,
                );
                connect_or_report(
                    &*sync_source,
                    "lengthChanged(::std::int32_t,::std::int32_t)",
                    &*self_qobj,
                    "update_sync_length()",
                    connection_types::DIRECT_CONNECTION,
                );
                self.as_mut().update_sync_position();
                self.as_mut().update_sync_length();
            }
            self.sync_source_changed(sync_source);
        }
    }

    pub fn update_sync_position(mut self: Pin<&mut CompositeLoopBackend>) {
        trace!(self, "update sync position");
        let mut v = 0;
        unsafe {
            if !self.sync_source.is_null() {
                match qobject::qobject_property_int(&*self.sync_source, "position") {
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
                match qobject::qobject_property_int(&*self.sync_source, "length") {
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

    pub unsafe fn set_backend(mut self: Pin<&mut Self>, backend: *mut QObject) {
        debug!(self, "set backend -> {backend:?}");
        let backend_changed = self.backend != backend;
        let current_session = if backend.is_null() {
            None
        } else {
            BackendWrapper::from_qobject_mut_ptr(backend)
                .ok()
                .and_then(|wrapper| wrapper.session.clone())
        };
        let current_session_id = current_session.as_ref().map(|session| session.session_id());
        let installed_session_id = self
            .backend_session
            .as_ref()
            .map(|session| session.session_id());
        let session_changed = current_session_id != installed_session_id;

        if backend_changed || session_changed {
            let was_initialized = self.initialized;
            {
                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.backend = backend;
                rust_mut.backend_session = None;
                rust_mut.engine_loop = None;
                rust_mut.engine_schedule_install = None;
                rust_mut.engine_schedule_dirty = true;
                rust_mut.engine_schedule_installed = false;
                rust_mut.initialized = false;
            }
            if was_initialized {
                self.as_mut().initialized_changed(false);
            }
            if backend_changed {
                self.as_mut().backend_changed(backend);
            }
        }
        if backend.is_null() || self.engine_loop.is_some() {
            return;
        }
        let result = || -> Result<(), anyhow::Error> {
            let session =
                current_session.ok_or_else(|| anyhow::anyhow!("Backend session is null"))?;
            let engine_loop = session.create_composite_loop()?;
            {
                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.backend_session = Some(session);
                rust_mut.engine_loop = Some(engine_loop);
                rust_mut.initialized = true;
            }
            self.as_mut().initialized_changed(true);
            Ok(())
        }();
        if let Err(error) = result {
            error!(self, "could not initialize engine composite: {error}");
        }
    }

    pub fn update_length(mut self: Pin<&mut Self>) {
        trace!(
            self,
            "update length: sync length {}, n cycles {}",
            self.sync_length,
            self.n_cycles
        );
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
        if self.engine_loop.is_some() {
            return;
        }
        trace!(self, "update position");
        let mut v = max(0, self.iteration) * self.sync_length;
        if is_running_mode(LoopMode::try_from(self.mode).unwrap_or(LoopMode::Unknown)) {
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
                    if converted_schedule.data.is_empty() && self.engine_schedule_installed {
                        if let Some(engine_loop) = self.engine_loop.as_ref().cloned() {
                            if engine_loop
                                .get_state()
                                .is_ok_and(|state| state.mode != LoopMode::Stopped)
                            {
                                if let Err(error) =
                                    engine_loop.transition_immediate(LoopMode::Stopped, 0)
                                {
                                    error!(self, "engine composite clear failed: {error}");
                                }
                            }
                        }
                    }
                    let self_mut = self.as_mut();
                    let mut rust_mut = self_mut.rust_mut();
                    rust_mut.schedule = converted_schedule;
                    rust_mut.engine_schedule_dirty = true;
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
        if !self.engine_schedule_dirty {
            if let Some(engine_loop) = self.engine_loop.as_ref() {
                if let Err(error) = engine_loop.set_play_after_record(play_after_record) {
                    error!(self, "engine record option failed: {error}");
                }
            }
        }
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
            rust_mut.engine_schedule_dirty = true;
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

    pub fn update(mut self: Pin<&mut Self>) {
        unsafe {
            let backend = self.backend;
            self.as_mut().set_backend(backend);
        }
        if self.engine_loop.is_none() {
            return;
        }
        let install_result = self
            .engine_schedule_install
            .as_ref()
            .and_then(|install| install.take_result());
        if let Some(result) = install_result {
            self.as_mut().rust_mut().engine_schedule_install = None;
            match result {
                Ok(_) => {
                    self.as_mut().rust_mut().engine_schedule_installed = true;
                    if let Some(engine_loop) = self.engine_loop.as_ref() {
                        if let Err(error) =
                            engine_loop.set_play_after_record(self.play_after_record)
                        {
                            error!(self, "could not queue composite record option: {error}");
                        }
                    }
                }
                Err(error) => {
                    error!(self, "engine composite configuration failed: {error}");
                    self.as_mut().rust_mut().engine_schedule_dirty = true;
                }
            }
        }
        if self.engine_schedule_dirty && self.as_mut().install_engine_schedule().is_err() {
            return;
        }
        let Some(state) = self.engine_loop.as_ref().and_then(|engine_loop| {
            engine_loop
                .poll_state()
                .or_else(|| engine_loop.get_state().ok())
        }) else {
            return;
        };
        self.as_mut().set_mode(state.mode as i32);
        self.as_mut()
            .set_next_mode(state.maybe_next_mode.map(|mode| mode as i32).unwrap_or(-1));
        self.as_mut().set_next_transition_delay(
            state
                .maybe_next_mode_delay
                .map(|delay| delay as i32)
                .unwrap_or(-1),
        );
        self.as_mut().set_iteration(state.iteration as i32);
        let previous_cycle = self.cycle_nr;
        let cycle = state.cycle_count.min(i32::MAX as u64) as i32;
        self.as_mut().set_cycle_nr(cycle);
        if cycle > previous_cycle {
            self.as_mut().cycled(cycle);
        }
        let length = state.length.min(i32::MAX as u64) as i32;
        let position = state.position.min(i32::MAX as u64) as i32;
        let play_after_record_changed = self.play_after_record != state.play_after_record;
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.length = length;
            rust_mut.position = position;
            rust_mut.play_after_record = state.play_after_record;
        }
        unsafe {
            self.as_mut().length_changed(length);
            self.as_mut().position_changed(position);
            if play_after_record_changed {
                self.as_mut()
                    .play_after_record_changed(state.play_after_record);
            }
        }

        let active: BTreeSet<_> = state
            .active_children
            .iter()
            .map(|child| child.identity)
            .collect();
        let mut running = QList_QVariant::default();
        for object in self.as_mut().all_loops() {
            if object.is_null() {
                continue;
            }
            let Some(identity) = (unsafe { engine_identity(object) }) else {
                continue;
            };
            if active.contains(&identity) {
                if let Ok(variant) = qobject_ptr_to_qvariant(&object) {
                    running.append(variant);
                }
            }
        }
        self.as_mut().rust_mut().running_loops = running.clone();
        unsafe {
            self.as_mut().running_loops_changed(running);
        }
    }

    pub fn get_schedule(self: &CompositeLoopBackend) -> QMap_QString_QVariant {
        self.schedule.to_qvariantmap()
    }

    pub fn clear(mut self: Pin<&mut CompositeLoopBackend>) {
        if self.engine_schedule_installed {
            if let Some(engine_loop) = self.engine_loop.as_ref().cloned() {
                if engine_loop
                    .get_state()
                    .is_ok_and(|state| state.mode != LoopMode::Stopped)
                {
                    if let Err(error) = engine_loop.transition_immediate(LoopMode::Stopped, 0) {
                        error!(self, "engine composite clear failed: {error}");
                    }
                }
            }
        }
        let empty_running_loops = QList_QVariant::default();
        let empty_schedule = CompositeLoopSchedule::default();
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.running_loops = empty_running_loops.clone();
            rust_mut.schedule = empty_schedule;
            rust_mut.iteration = -1;
            rust_mut.mode = LoopMode::Stopped as isize as i32;
            rust_mut.next_mode = -1;
            rust_mut.next_transition_delay = -1;
            rust_mut.n_cycles = 0;
            rust_mut.length = 0;
            rust_mut.sync_position = 0;
            rust_mut.sync_length = 0;
            rust_mut.position = 0;
            rust_mut.cycle_nr = 0;
            rust_mut.engine_schedule_dirty = true;
        }
        unsafe {
            self.as_mut().running_loops_changed(empty_running_loops);
            self.as_mut().iteration_changed(-1);
            self.as_mut()
                .mode_changed(LoopMode::Stopped as isize as i32);
            self.as_mut().next_mode_changed(-1);
            self.as_mut().next_transition_delay_changed(-1);
            self.as_mut().n_cycles_changed(0);
            self.as_mut().length_changed(0);
            self.as_mut().sync_position_changed(0);
            self.as_mut().sync_length_changed(0);
            self.as_mut().position_changed(0);
            self.as_mut().cycle_nr_changed(0);
        }
    }

    pub fn deinit(mut self: Pin<&mut CompositeLoopBackend>) {
        if self.engine_schedule_installed {
            let removal = self
                .backend_session
                .as_ref()
                .zip(self.engine_loop.as_ref())
                .map(|(session, engine_loop)| {
                    let primitive_sync_sources = session.primitive_sync_sources();
                    session.remove_composite_loop(engine_loop, &primitive_sync_sources)
                });
            if let Some(Err(error)) = removal {
                error!(self, "engine composite removal failed: {error}");
                return;
            }
        }
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut.engine_loop = None;
        rust_mut.backend_session = None;
        rust_mut.engine_schedule_installed = false;
        rust_mut.engine_schedule_installing = false;
        rust_mut.engine_schedule_dirty = true;
        rust_mut.sync_source = std::ptr::null_mut();
        rust_mut.backend = std::ptr::null_mut();
        rust_mut.initialized = false;
    }

    pub fn metatype_name() -> String {
        unsafe {
            composite_loop_backend_metatype_name(std::ptr::null_mut())
                .unwrap_or_else(|_| "unknown".to_string())
        }
    }
}
