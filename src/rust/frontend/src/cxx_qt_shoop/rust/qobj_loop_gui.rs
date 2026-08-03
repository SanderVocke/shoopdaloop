use crate::cxx_qt_shoop::qobj_backend_wrapper::BackendWrapper;
use crate::cxx_qt_shoop::qobj_loop_gui_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_loop_gui_bridge::LoopGui;
use crate::loop_helpers::transition_frontend_loops;
use anyhow::anyhow;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt::QObject;
use cxx_qt_lib::{QString, QVariant};
use cxx_qt_lib_shoop::connect::connect_or_report;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::qobject;
use cxx_qt_lib_shoop::qobject::FromQObject;
use cxx_qt_lib_shoop::qpointer::{qpointer_from_qobject, qpointer_to_qobject};
use cxx_qt_lib_shoop::qvariant_helpers::qvariant_to_qobject_ptr;
use shoop_engine::LoopMode;
use std::pin::Pin;
shoop_log_unit!("Frontend.Loop");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*))
    };
}

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*))
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*))
    };
}

fn convert_maybe_mode_i32(value: Option<LoopMode>) -> i32 {
    match value {
        Some(v) => v as i32,
        None => LoopMode::Unknown as i32,
    }
}

impl LoopGui {
    pub fn initialize_impl(self: Pin<&mut LoopGui>) {}

    pub fn queue_set_length(self: Pin<&mut LoopGui>, length: i32) {
        self.set_length(length);
    }

    pub fn set_length(mut self: Pin<&mut LoopGui>, length: i32) {
        if !self.as_mut().maybe_initialize_backend() {
            debug!(self, "set length -> {length} (deferred)");
            let mut rust = self.as_mut().rust_mut();
            rust.prev_state.length = length as u32;
            return;
        } else {
            debug!(self, "set length -> {}", length);
            let mut rust = self.as_mut().rust_mut();
            if let Some(loop_obj) = rust.backend_loop.as_mut() {
                if let Err(e) = loop_obj.set_length(length as u32) {
                    error!(self, "Failed to set length on backend loop: {e}");
                }
            } else {
                error!(
                    self,
                    "Backend loop object doesn't exist when setting length"
                );
            }
        }
    }

    pub fn queue_set_position(self: Pin<&mut LoopGui>, position: i32) {
        self.set_position(position);
    }

    pub fn set_position(mut self: Pin<&mut LoopGui>, position: i32) {
        if !self.as_mut().maybe_initialize_backend() {
            debug!(self, "set position -> {position} (deferred)");
            let mut rust = self.as_mut().rust_mut();
            rust.prev_state.position = position as u32;
            return;
        } else {
            debug!(self, "set position -> {}", position);
            let mut rust = self.as_mut().rust_mut();
            if let Some(loop_obj) = rust.backend_loop.as_mut() {
                if let Err(e) = loop_obj.set_position(position as u32) {
                    error!(self, "Failed to set position on backend loop: {e}");
                }
            } else {
                error!(
                    self,
                    "Backend loop object doesn't exist when setting position"
                );
            }
        }
    }

    pub fn set_backend(mut self: Pin<&mut LoopGui>, backend: *mut QObject) {
        debug!(self, "set backend -> {:?}", backend);
        let backend_changed = self.backend != backend;
        let was_initialized = self.get_initialized();
        {
            let mut rust_mut = self.as_mut().rust_mut();
            if backend_changed {
                rust_mut.backend_loop = None;
                rust_mut.sync_source_applied_session_id = None;
            }
            rust_mut.backend = backend;
        }
        if backend_changed && was_initialized {
            self.as_mut().initialized_changed(false);
        }

        if backend_changed && !backend.is_null() {
            unsafe {
                connect_or_report(
                    &*backend,
                    "readyChanged()",
                    self.as_ref().get_ref(),
                    "maybe_initialize_backend()",
                    connection_types::QUEUED_CONNECTION,
                );
                connect_or_report(
                    &*backend,
                    "updated_on_gui_thread()",
                    self.as_ref().get_ref(),
                    "update()",
                    connection_types::DIRECT_CONNECTION,
                );
            }
        }
        self.as_mut().maybe_initialize_backend();

        unsafe {
            self.as_mut().backend_changed(backend);
        }
    }

    pub fn set_instance_identifier(mut self: Pin<&mut LoopGui>, instance_identifier: QString) {
        let mut extended: QString = instance_identifier.clone();
        extended.append(&QString::from("-backend"));
        debug!(self, "set instance identifier -> {:?}", &extended);
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut.instance_identifier = extended.clone();
        self.as_mut().instance_identifier_changed(extended);
    }

    pub fn maybe_initialize_backend(mut self: Pin<&mut LoopGui>) -> bool {
        match || -> Result<bool, anyhow::Error> {
            let initialize_condition: bool;

            if self.as_ref().get_initialized() {
                let current_session_id = unsafe {
                    let backend = BackendWrapper::from_qobject_mut_ptr(self.as_ref().backend)?;
                    backend.session.as_ref().map(|session| session.session_id())
                };
                let loop_session_id = self
                    .as_ref()
                    .rust()
                    .backend_loop
                    .as_ref()
                    .map(|loop_| loop_.session_id());
                if current_session_id == loop_session_id {
                    return Ok(true);
                }
                {
                    let mut rust_mut = self.as_mut().rust_mut();
                    rust_mut.backend_loop = None;
                    rust_mut.sync_source_applied_session_id = None;
                }
                self.as_mut().initialized_changed(false);
                // Signal handlers may rebuild dependent QML objects synchronously. Retry
                // initialization on the next frontend tick rather than continuing with self.
                return Ok(false);
            }

            unsafe {
                initialize_condition = !self.get_initialized()
                    && self.as_ref().backend != std::ptr::null_mut()
                    && match self.backend.as_ref() {
                        Some(backend) => {
                            qobject::qobject_property_bool(backend, "ready").unwrap_or(false)
                        }
                        None => false,
                    }
                    && self.as_ref().backend_loop.is_none();
            }

            if initialize_condition {
                debug!(self, "Initializing");
                unsafe {
                    let backend = BackendWrapper::from_qobject_mut_ptr(self.as_ref().backend)?;
                    let backend_session = backend
                        .session
                        .as_ref()
                        .ok_or_else(|| anyhow!("Backend session is null"))?;
                    let backend_loop = backend_session
                        .create_loop()
                        .map_err(|e| anyhow!("Failed to create backend loop: {}", e))?;
                    {
                        let mut rust_mut = self.as_mut().rust_mut();
                        rust_mut.backend_loop = Some(backend_loop);
                    }

                    {
                        let sync_source = self.as_ref().guarded_sync_source();
                        let length = self.as_ref().prev_state.length;
                        let position = self.as_ref().prev_state.position;
                        let session_id = self
                            .as_ref()
                            .backend_loop
                            .as_ref()
                            .map(|loop_| loop_.session_id())
                            .unwrap_or_default();
                        if self
                            .as_ref()
                            .sync_source_is_ready_for_session(sync_source, session_id)
                        {
                            self.as_mut().set_backend_sync_source(sync_source);
                        }
                        self.as_mut().set_length(length as i32);
                        self.as_mut().set_position(position as i32);
                    }

                    {
                        // Force getting of the initial state
                        self.as_mut().update();

                        let initialized = self.get_initialized();
                        self.as_mut().initialized_changed(initialized);
                        return Ok(initialized);
                    }
                }
            } else {
                debug!(self, "Not initializing as not all conditions are met");
                return Ok(false);
            }
        }() {
            Ok(result) => return result,
            Err(e) => {
                debug!(self, "Error initializing backend: {e}");
                return false;
            }
        }
    }

    pub fn update(mut self: Pin<&mut LoopGui>) {
        let span = tracing::debug_span!(
            "frontend.loop.update",
            mode = tracing::field::Empty,
            position = tracing::field::Empty,
            length = tracing::field::Empty
        );
        let _entered = span.enter();
        if !self.as_mut().maybe_initialize_backend() {
            return;
        }
        let session_id = self
            .as_ref()
            .backend_loop
            .as_ref()
            .map(|loop_| loop_.session_id())
            .unwrap_or_default();
        let sync_source = self.as_ref().guarded_sync_source();
        if self.sync_source_applied_session_id != Some(session_id)
            && self
                .as_ref()
                .sync_source_is_ready_for_session(sync_source, session_id)
        {
            unsafe {
                self.as_mut().set_backend_sync_source(sync_source);
            }
        }

        let result = || -> Result<(), anyhow::Error> {
            self.as_mut().starting_update();
            let mut rust = self.as_mut().rust_mut();
            let backend_loop = rust
                .backend_loop
                .as_mut()
                .ok_or(anyhow!("backend loop object doesn't exist"))?;
            // Published state rather than a round trip to the audio thread. Pending loops
            // keep their previous/default frontend state until the first mirror is ready.
            let Some(new_state) = backend_loop.poll_state() else {
                return Ok(());
            };
            span.record("mode", new_state.mode as u32);
            span.record("position", new_state.position);
            span.record("length", new_state.length);
            if common::tracing_helpers::is_tracing_enabled() {
                tracy_client::plot!("engine.loop.mode", new_state.mode as u32 as f64);
                tracy_client::plot!("engine.loop.position", new_state.position as f64);
                tracy_client::plot!(
                    "engine.loop.next_mode",
                    new_state
                        .maybe_next_mode
                        .map(|mode| mode as u32 as f64)
                        .unwrap_or(-1.0)
                );
                tracy_client::plot!(
                    "engine.loop.transition_delay",
                    new_state
                        .maybe_next_mode_delay
                        .map(f64::from)
                        .unwrap_or(-1.0)
                );
            }

            let prev_state;
            let prev_cycle_nr: i32;
            let new_cycle_nr: i32;
            {
                let mut rust = self.as_mut().rust_mut();

                prev_state = rust.prev_state.clone();
                prev_cycle_nr = rust.prev_cycle_nr;

                new_cycle_nr = new_state.cycle_count.min(i32::MAX as u64) as i32;

                rust.prev_state = new_state.clone();
                rust.prev_cycle_nr = new_cycle_nr;
            }

            self.as_mut().state_changed(
                new_state.mode as i32,
                new_state.length as i32,
                new_state.position as i32,
                convert_maybe_mode_i32(new_state.maybe_next_mode),
                new_state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32,
                new_cycle_nr,
            );

            if prev_state.mode != new_state.mode {
                debug!(self, "mode: {:?} -> {:?}", prev_state.mode, new_state.mode);
                self.as_mut()
                    .mode_changed(prev_state.mode as i32, new_state.mode as i32);
            }
            if prev_state.length != new_state.length {
                trace!(
                    self,
                    "length: {} -> {}",
                    prev_state.length,
                    new_state.length
                );
                self.as_mut()
                    .length_changed(prev_state.length as i32, new_state.length as i32);
            }
            if prev_state.position != new_state.position {
                trace!(
                    self,
                    "position: {} -> {}",
                    prev_state.position,
                    new_state.position
                );
                self.as_mut()
                    .position_changed(prev_state.position as i32, new_state.position as i32);
            }
            if prev_state.maybe_next_mode != new_state.maybe_next_mode {
                debug!(
                    self,
                    "next mode: {:?} -> {:?}",
                    prev_state.maybe_next_mode,
                    new_state.maybe_next_mode
                );
                let prev_mode = convert_maybe_mode_i32(prev_state.maybe_next_mode);
                let new_mode = convert_maybe_mode_i32(new_state.maybe_next_mode);
                self.as_mut().next_mode_changed(prev_mode, new_mode);
            }
            if prev_state.maybe_next_mode_delay != new_state.maybe_next_mode_delay {
                debug!(
                    self,
                    "next delay: {:?} -> {:?}",
                    prev_state.maybe_next_mode_delay,
                    new_state.maybe_next_mode_delay
                );
                let prev_delay: i32 = prev_state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32;
                let new_delay: i32 = new_state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32;
                self.as_mut()
                    .next_transition_delay_changed(prev_delay, new_delay);
            }
            if prev_cycle_nr != new_cycle_nr {
                debug!(self, "cycle nr: {} -> {}", prev_cycle_nr, new_cycle_nr);
                self.as_mut().cycle_nr_changed(new_cycle_nr, prev_cycle_nr);
                if (new_cycle_nr - prev_cycle_nr) == 1 {
                    debug!(self, "cycled");
                    self.as_mut().cycled(new_cycle_nr);
                }
            }

            Ok(())
        }();
        match result {
            Ok(_) => {}
            Err(e) => {
                error!(self, "Error while updating backend loop: {}", e);
            }
        }
    }

    pub fn transition_multiple(
        self: Pin<&mut LoopGui>,
        loops: QList_QVariant,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        let _span = tracing::info_span!(
            "frontend.control.transition_loops",
            loops = loops.len(),
            to_mode,
            cycles_delay = maybe_cycles_delay
        )
        .entered();
        let to_mode_enum = match LoopMode::try_from(to_mode) {
            Ok(m) => m,
            Err(e) => {
                error!(self, "Invalid loop mode: {}", e);
                return;
            }
        };

        let loop_ptrs: Vec<*mut QObject> = loops
            .iter()
            .filter_map(|variant| match qvariant_to_qobject_ptr(variant) {
                Ok(ptr) => Some(ptr),
                Err(e) => {
                    error!(self, "Failed to convert QVariant: {}", e);
                    None
                }
            })
            .collect();

        if let Err(e) = transition_frontend_loops(
            loop_ptrs,
            to_mode_enum,
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

    pub fn transition_multiple_backend_in_unison(
        self: Pin<&mut LoopGui>,
        loops: QList_QVariant,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        let _span = tracing::info_span!(
            "frontend.control.transition_engine_loops",
            loops = loops.len(),
            to_mode,
            cycles_delay = maybe_cycles_delay
        )
        .entered();
        raw_debug!(
            "Transitioning {} loops to {} with delay {}, sync at cycle {}",
            loops.len(),
            to_mode,
            maybe_cycles_delay,
            maybe_to_sync_at_cycle
        );
        let result: Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            let mut backend_loop_refs: Vec<&shoop_engine::app_backend::Loop> = Vec::new();
            backend_loop_refs.reserve(loops.len() as usize);

            // Increment the reference count for all loops involved
            loops
                .iter()
                .map(|loop_variant| -> Result<(), anyhow::Error> {
                    unsafe {
                        let loop_qobj: *mut QObject = qvariant_to_qobject_ptr(loop_variant)?;
                        let loop_ptr: *mut LoopGui = qobject_to_loop_gui_ptr(loop_qobj);
                        {
                            let loop_pin = std::pin::Pin::new_unchecked(&mut *loop_ptr);
                            loop_pin.maybe_initialize_backend();
                        }
                        let backend_loop_ref: &shoop_engine::app_backend::Loop = loop_ptr
                            .as_ref()
                            .ok_or_else(|| anyhow!("Loop pointer is null"))?
                            .backend_loop
                            .as_ref()
                            .ok_or_else(|| anyhow!("Backend loop not set"))?;
                        backend_loop_refs.push(backend_loop_ref);
                        Ok(())
                    }
                })
                .for_each(|result| match result {
                    Ok(_) => (),
                    Err(err) => {
                        raw_error!("Failed to get backend loop loop: {:?}", err);
                    }
                });

            shoop_engine::app_backend::transition_multiple_loops(
                &backend_loop_refs,
                to_mode.try_into()?,
                maybe_cycles_delay,
                maybe_to_sync_at_cycle,
            )
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                raw_error!("Failed to transition multiple loops: {:?}", err);
            }
        }
    }

    pub fn transition(
        mut self: Pin<&mut LoopGui>,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        let _span = tracing::info_span!(
            "frontend.control.transition_loop",
            to_mode,
            cycles_delay = maybe_cycles_delay
        )
        .entered();
        if !self.as_mut().maybe_initialize_backend() {
            error!(self, "transition: not initialized");
            return;
        }
        let result: Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            self.as_ref()
                .backend_loop
                .as_ref()
                .ok_or_else(|| anyhow!("Backend loop is null"))?
                .transition(
                    to_mode.try_into()?,
                    maybe_cycles_delay,
                    maybe_to_sync_at_cycle,
                )?;
            debug!(
                self,
                "Transitioning to {} with delay {}, sync at cycle {}",
                to_mode,
                maybe_cycles_delay,
                maybe_to_sync_at_cycle
            );
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to transition loop: {:?}", err);
            }
        }
    }

    pub fn clear(mut self: Pin<&mut LoopGui>, length: i32) {
        if !self.as_mut().maybe_initialize_backend() {
            error!(self, "clear: not initialized");
            return;
        }
        let result: Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            debug!(self, "clearing to length {length}");
            self.as_ref()
                .backend_loop
                .as_ref()
                .ok_or_else(|| anyhow!("Backend loop is null"))?
                .clear(length as u32)?;
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to clear loop: {:?}", err);
            }
        }
    }

    pub fn adopt_ringbuffers(
        mut self: Pin<&mut LoopGui>,
        maybe_reverse_start_cycle: QVariant,
        maybe_cycles_length: QVariant,
        maybe_go_to_cycle: QVariant,
        go_to_mode: i32,
    ) {
        if !self.as_mut().maybe_initialize_backend() {
            error!(self, "adopt_ringbuffers: not initialized");
            return;
        }
        debug!(self, "Adopting ringbuffers");
        let result: Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            self.as_ref()
                .backend_loop
                .as_ref()
                .ok_or_else(|| anyhow!("Backend loop is null"))?
                .adopt_ringbuffer_contents(
                    maybe_reverse_start_cycle.value::<i32>(),
                    maybe_cycles_length.value::<i32>(),
                    maybe_go_to_cycle.value::<i32>(),
                    go_to_mode.try_into()?,
                )?;
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to adopt ringbuffers: {:?}", err);
            }
        }
    }

    fn guarded_sync_source(&self) -> *mut QObject {
        if self.sync_source_guard.is_null() {
            std::ptr::null_mut()
        } else {
            unsafe { qpointer_to_qobject(&self.sync_source_guard) }
        }
    }

    fn sync_source_is_ready_for_session(&self, sync_source: *mut QObject, session_id: u64) -> bool {
        if sync_source.is_null() {
            return true;
        }
        unsafe {
            let loop_ptr = qobject_to_loop_gui_ptr(sync_source);
            !loop_ptr.is_null()
                && (&*loop_ptr)
                    .rust()
                    .backend_loop
                    .as_ref()
                    .is_some_and(|loop_| loop_.session_id() == session_id)
        }
    }

    unsafe fn set_backend_sync_source(mut self: Pin<&mut LoopGui>, sync_source: *mut QObject) {
        debug!(self, "set sync source -> {:?}", sync_source);
        let session_id = self
            .as_ref()
            .backend_loop
            .as_ref()
            .map(|loop_| loop_.session_id());
        let result: Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            if !sync_source.is_null() {
                let loop_ptr = qobject_to_loop_gui_ptr(sync_source);
                if loop_ptr.is_null() {
                    return Err(anyhow!("Failed to cast sync source QObject to LoopGui"));
                }
                self.as_ref()
                    .backend_loop
                    .as_ref()
                    .ok_or_else(|| anyhow!("Backend loop is null"))?
                    .set_sync_source(
                        loop_ptr
                            .as_ref()
                            .ok_or_else(|| anyhow!("Loop pointer is null"))?
                            .backend_loop
                            .as_ref(),
                    )?;
            } else {
                self.as_ref()
                    .backend_loop
                    .as_ref()
                    .ok_or_else(|| anyhow!("Backend loop is null"))?
                    .set_sync_source(None)?;
            }

            Ok(())
        })();
        match result {
            Ok(_) => {
                self.as_mut().rust_mut().sync_source_applied_session_id = session_id;
            }
            Err(err) => {
                error!(self, "Failed to update backend sync source: {:?}", err);
            }
        }
    }

    pub unsafe fn set_sync_source(mut self: Pin<&mut LoopGui>, sync_source_ptr: *mut QObject) {
        let old_source = self.as_ref().guarded_sync_source();
        let changed = old_source != sync_source_ptr;
        if changed {
            let sync_source_guard = if sync_source_ptr.is_null() {
                cxx::UniquePtr::null()
            } else {
                unsafe { qpointer_from_qobject(sync_source_ptr) }
            };
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.sync_source = sync_source_ptr;
            rust_mut.sync_source_guard = sync_source_guard;
            rust_mut.sync_source_applied_session_id = None;
        }

        if !self.as_mut().maybe_initialize_backend() {
            debug!(self, "set_sync_source -> {:?} (deferred)", sync_source_ptr);
        } else {
            let session_id = self
                .as_ref()
                .backend_loop
                .as_ref()
                .map(|loop_| loop_.session_id())
                .unwrap_or_default();
            if self
                .as_ref()
                .sync_source_is_ready_for_session(sync_source_ptr, session_id)
            {
                self.as_mut().set_backend_sync_source(sync_source_ptr);
            }
        }

        if changed {
            self.as_mut().sync_source_changed(sync_source_ptr);
        }
    }

    pub fn get_mode(self: &LoopGui) -> i32 {
        self.rust().prev_state.mode as i32
    }

    pub fn get_length(self: &LoopGui) -> i32 {
        self.rust().prev_state.length as i32
    }

    pub fn get_position(self: &LoopGui) -> i32 {
        self.rust().prev_state.position as i32
    }

    pub fn get_next_mode(self: &LoopGui) -> i32 {
        convert_maybe_mode_i32(self.rust().prev_state.maybe_next_mode)
    }

    pub fn get_next_transition_delay(self: &LoopGui) -> i32 {
        self.rust()
            .prev_state
            .maybe_next_mode_delay
            .unwrap_or(u32::MAX) as i32
    }

    pub fn get_cycle_nr(self: &LoopGui) -> i32 {
        self.rust().prev_cycle_nr
    }

    pub fn get_initialized(self: &LoopGui) -> bool {
        self.rust().backend_loop.is_some()
    }

    pub fn deinit(mut self: Pin<&mut LoopGui>) {
        self.as_mut().rust_mut().backend_loop = None;
        self.as_mut().initialized_changed(false);
    }

    pub fn metatype_name() -> String {
        unsafe {
            loop_gui_metatype_name(std::ptr::null_mut()).unwrap_or_else(|_| "Unknown".to_string())
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut module_name = String::from(module_name);
    let mut type_name = String::from(type_name);
    unsafe {
        register_qml_type_loop_gui(std::ptr::null_mut(), &mut module_name, 1, 0, &mut type_name);
    }
}
