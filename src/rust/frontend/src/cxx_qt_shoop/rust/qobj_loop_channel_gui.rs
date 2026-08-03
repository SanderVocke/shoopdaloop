use crate::any_backend_port::AnyBackendPort;
use crate::cxx_qt_shoop::qobj_async_task_bridge::ffi::make_raw_async_task_with_parent;
use crate::cxx_qt_shoop::qobj_loop_channel_gui_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_port_gui_bridge::PortGui;
use crate::midi_event_helpers::MidiEventToQVariant;
use crate::{any_backend_channel::AnyBackendChannel, cxx_qt_shoop::qobj_loop_gui_bridge::LoopGui};
use anyhow::anyhow;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt::QObject;
use cxx_qt_lib::QVector;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    invokable::invoke,
    qobject::{qobject_property_bool, AsQObject, FromQObject},
    qpointer::{qpointer_from_qobject, qpointer_to_qobject},
    qsharedpointer_qvector_qvariant::QSharedPointer_QVector_QVariant,
    qvariant_helpers::{
        qobject_ptr_to_qvariant, qsharedpointer_qvector_qvariant_to_qvariant,
        qvariant_to_qobject_ptr,
    },
};
use shoop_engine::{ChannelMode, MidiEvent, PortConnectability, PortDataType};
use std::{
    collections::HashSet,
    pin::Pin,
    sync::{atomic::Ordering, Arc},
};
shoop_log_unit!("Frontend.LoopChannel");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}-backend] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

impl LoopChannelGui {
    pub fn initialize_impl(self: Pin<&mut LoopChannelGui>) {}

    fn live_channel_loop(&self) -> *mut QObject {
        if self.channel_loop_guard.is_null() {
            std::ptr::null_mut()
        } else {
            unsafe { qpointer_to_qobject(&self.channel_loop_guard) }
        }
    }

    unsafe fn channel_loop_session_id(&self) -> Option<u64> {
        let loop_ptr = self.live_channel_loop();
        if loop_ptr.is_null() {
            return None;
        }
        LoopGui::from_qobject_ref_ptr(loop_ptr)
            .ok()?
            .backend_loop
            .as_ref()
            .map(|loop_| loop_.session_id())
    }

    pub fn update(mut self: Pin<&mut LoopChannelGui>) {
        let span = tracing::debug_span!(
            "frontend.channel.update",
            mode = tracing::field::Empty,
            length = tracing::field::Empty,
            events = tracing::field::Empty
        );
        let _entered = span.enter();
        if !self.as_mut().maybe_initialize_backend() {
            return;
        }
        // Parent loops and ports are created independently. Retry relationship resolution on
        // refresh so a port that became ready before its queued Qt notification was handled is
        // still connected deterministically.
        self.as_mut().update_port_connections_impl();

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let channel = self
                .maybe_backend_channel
                .as_ref()
                .ok_or(anyhow!("Backend channel is None in update"))?;
            let prev_state = self.prev_state.clone();
            let new_state = match channel.poll_state() {
                Ok(state) => state,
                Err(e) => {
                    debug!(self, "Skipping update: {e}");
                    prev_state.clone()
                }
            };
            span.record("mode", new_state.mode as u32);
            span.record("length", new_state.length);
            span.record("events", new_state.n_events_triggered);
            if common::tracing_helpers::is_tracing_enabled() {
                if new_state.mode != prev_state.mode {
                    tracy_client::plot!("engine.channel.mode", new_state.mode as u32 as f64);
                }
                if new_state.length != prev_state.length {
                    tracy_client::plot!("engine.channel.length", new_state.length as f64);
                }
                if new_state.n_events_triggered != prev_state.n_events_triggered {
                    tracy_client::plot!(
                        "engine.channel.events_triggered",
                        new_state.n_events_triggered as f64
                    );
                }
            }

            {
                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.prev_state = new_state.clone();
            }

            let new_played_back_sample_variant = match new_state.played_back_sample {
                Some(sample) => QVariant::from(&(sample as i32)),
                None => QVariant::default(),
            };

            // Self "state_changed" signal
            unsafe {
                let initialized = self.initialized;
                self.as_mut().state_changed(
                    initialized,
                    new_state.mode as i32,
                    new_state.length as i32,
                    new_state.start_offset,
                    new_played_back_sample_variant.clone(),
                    new_state.n_preplay_samples as i32,
                    new_state.data_dirty,
                    new_state.audio_gain as f32,
                    new_state.audio_output_peak as f32,
                    new_state.n_events_triggered as i32,
                    new_state.n_notes_active as i32,
                );
            }

            // Update individual field signals
            unsafe {
                if new_state.mode != prev_state.mode {
                    self.as_mut().mode_changed(new_state.mode as i32);
                }
                if new_state.length != prev_state.length {
                    self.as_mut().data_length_changed(new_state.length as i32);
                }
                if new_state.start_offset != prev_state.start_offset {
                    self.as_mut().start_offset_changed(new_state.start_offset);
                }
                if new_state.played_back_sample != prev_state.played_back_sample {
                    self.as_mut()
                        .last_played_sample_changed(new_played_back_sample_variant);
                }
                if new_state.n_preplay_samples != prev_state.n_preplay_samples {
                    self.as_mut()
                        .n_preplay_samples_changed(new_state.n_preplay_samples as i32);
                }
                if new_state.data_dirty != prev_state.data_dirty {
                    self.as_mut().data_dirty_changed(new_state.data_dirty);
                }
                if new_state.audio_gain != prev_state.audio_gain {
                    self.as_mut()
                        .audio_gain_changed(new_state.audio_gain as f32);
                }
                if new_state.audio_output_peak != prev_state.audio_output_peak {
                    self.as_mut()
                        .audio_output_peak_changed(new_state.audio_output_peak as f32);
                }
                if new_state.n_events_triggered != prev_state.n_events_triggered {
                    self.as_mut()
                        .midi_n_events_triggered_changed(new_state.n_events_triggered as i32);
                }
                if new_state.n_notes_active != prev_state.n_notes_active {
                    self.as_mut()
                        .midi_n_notes_active_changed(new_state.n_notes_active as i32);
                }
            }

            Ok(())
        }() {
            error!(self, "Could not update: {e}")
        }
    }

    pub fn display_name(self: &LoopChannelGui) -> String {
        self.instance_identifier.to_string()
    }

    pub fn maybe_initialize_backend(mut self: Pin<&mut LoopChannelGui>) -> bool {
        match || -> Result<bool, anyhow::Error> {
            if self.initialized {
                let channel_session_id = self
                    .maybe_backend_channel
                    .as_ref()
                    .map(AnyBackendChannel::session_id);
                let loop_session_id = unsafe { self.channel_loop_session_id() };
                if channel_session_id.is_some() && channel_session_id == loop_session_id {
                    return Ok(true);
                }
                {
                    let mut rust_mut = self.as_mut().rust_mut();
                    rust_mut.maybe_backend_channel = None;
                    rust_mut.initialized = false;
                }
                unsafe {
                    self.as_mut().initialized_changed(false);
                }
                // QML may synchronously rebuild the channel backend in response.
                return Ok(false);
            }

            let mut non_ready_vars: HashSet<String> = HashSet::new();
            let channel_loop_ptr = self.live_channel_loop();
            unsafe {
                if self.backend.is_null() {
                    non_ready_vars.insert("backend".to_string());
                }
                if !self.backend.is_null() {
                    let ready = qobject_property_bool(
                        self.backend.as_ref().ok_or(anyhow!("Backend null ref"))?,
                        "ready",
                    )
                    .unwrap_or(false);
                    if !ready {
                        non_ready_vars.insert("backend_ready".to_string());
                    }
                }
                if channel_loop_ptr.is_null() {
                    non_ready_vars.insert("channel_loop".to_string());
                } else if !qobject_property_bool(
                    channel_loop_ptr
                        .as_ref()
                        .ok_or(anyhow!("channel_loop is null"))?,
                    "initialized",
                )
                .unwrap_or(false)
                {
                    non_ready_vars.insert("channel_loop initialized".to_string());
                }
                if self.data_type.is_none() {
                    non_ready_vars.insert("data_type".to_string());
                }
            }
            let initialize_condition: bool = !self.initialized && non_ready_vars.is_empty();

            if initialize_condition {
                unsafe {
                    debug!(self, "Initializing back-end");
                    let channel_loop = LoopGui::from_qobject_ref_ptr(channel_loop_ptr)?;
                    let channel_loop = channel_loop
                        .backend_loop
                        .as_ref()
                        .ok_or(anyhow!("No backend loop in loop object"))?;
                    let mode = ChannelMode::try_from(self.prev_state.mode as i32)?;
                    let backend_channel =
                        match self.data_type.ok_or(anyhow!("data_type is None"))? {
                            PortDataType::Audio => {
                                AnyBackendChannel::Audio(channel_loop.add_audio_channel(mode)?)
                            }
                            PortDataType::Midi => {
                                AnyBackendChannel::Midi(channel_loop.add_midi_channel(mode)?)
                            }
                            PortDataType::Any => {
                                return Err(anyhow!("No specific port data type"));
                            }
                        };

                    // Push initial state that was already set
                    let state = &self.prev_state;
                    debug!(self, "Push deferred state: {state:?}");
                    backend_channel.push_state(state)?;
                    self.as_mut().update_port_connections_impl();

                    // Store the newly created backend port
                    let mut rust_mut = self.as_mut().rust_mut();
                    rust_mut.maybe_backend_channel = Some(backend_channel);
                    rust_mut.initialized = true;

                    self.as_mut().initialized_changed(true);

                    Ok(true)
                }
            } else {
                trace!(
                    self,
                    "Not initializing backend yet. Non-ready variables: {non_ready_vars:?}"
                );
                return Ok(false);
            }
        }() {
            Ok(result) => result,
            Err(e) => {
                error!(self, "Could not initialize backend: {e}");
                false
            }
        }
    }

    pub fn get_initialized(self: &LoopChannelGui) -> bool {
        self.initialized
    }

    pub unsafe fn set_backend(mut self: Pin<&mut LoopChannelGui>, backend: *mut QObject) {
        if self.backend != backend {
            let was_initialized = self.initialized;
            {
                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.backend = backend;
                rust_mut.maybe_backend_channel = None;
                rust_mut.initialized = false;
            }
            if was_initialized {
                self.as_mut().initialized_changed(false);
            }
            unsafe {
                if !backend.is_null() {
                    let self_qobject = loop_channel_gui_qobject_from_ptr(
                        self.as_mut().get_unchecked_mut() as *mut Self,
                    );
                    trace!(self, "Connect back-end ready signal");
                    connect_or_report(
                        &mut *backend,
                        "readyChanged()",
                        &mut *self_qobject,
                        "maybe_initialize_backend()",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        &mut *backend,
                        "updated_on_gui_thread()",
                        &mut *self_qobject,
                        "update()",
                        connection_types::DIRECT_CONNECTION,
                    );
                }
                self.as_mut().backend_changed(backend);
            }
        }
        self.as_mut().maybe_initialize_backend();
    }

    pub fn set_instance_identifier(
        mut self: Pin<&mut LoopChannelGui>,
        instance_identifier: QString,
    ) {
        debug!(
            self,
            "set instance identifier -> {:?}", &instance_identifier
        );
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut.instance_identifier = instance_identifier.clone();
        unsafe {
            self.as_mut()
                .instance_identifier_changed(instance_identifier);
        }
    }

    pub fn set_is_midi(mut self: Pin<&mut LoopChannelGui>, is_midi: bool) {
        if self.maybe_backend_channel.is_some() {
            error!(self, "cannot set data type after initialization");
            return;
        };

        let data_type = if is_midi {
            PortDataType::Midi
        } else {
            PortDataType::Audio
        };
        debug!(self, "data type -> {data_type:?}");
        if !self.data_type.as_ref().is_some_and(|v| data_type == *v) {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.data_type = Some(data_type);
            unsafe {
                self.as_mut().is_midi_changed(is_midi);
            }
        }
        self.as_mut().maybe_initialize_backend();
    }

    pub unsafe fn set_channel_loop(mut self: Pin<&mut LoopChannelGui>, channel_loop: *mut QObject) {
        if self.maybe_backend_channel.is_some() {
            error!(self, "cannot set loop after initialization");
            return;
        }
        if self.channel_loop == channel_loop {
            return;
        }
        let guard = if channel_loop.is_null() {
            cxx::UniquePtr::null()
        } else {
            qpointer_from_qobject(channel_loop)
        };
        debug!(self, "channel loop -> {channel_loop:?}");
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut.channel_loop = channel_loop;
        rust_mut.channel_loop_guard = guard;
        self.as_mut().channel_loop_changed(channel_loop);
        self.as_mut().maybe_initialize_backend();
    }

    pub fn get_is_midi(self: Pin<&mut LoopChannelGui>) -> bool {
        self.data_type.unwrap_or(PortDataType::Audio) == PortDataType::Midi
    }

    pub fn get_channel_loop(self: Pin<&mut LoopChannelGui>) -> *mut QObject {
        self.live_channel_loop()
    }

    pub fn update_port_connections(self: Pin<&mut LoopChannelGui>) {
        self.update_port_connections_impl();
    }

    pub fn update_port_connections_impl(mut self: Pin<&mut LoopChannelGui>) {
        let Some(channel) = self.maybe_backend_channel.as_ref().cloned() else {
            return;
        };

        enum Action {
            Connect,
            Disconnect,
        }
        let apply = |port: &AnyBackendPort, action: Action| {
            let is_output = port
                .input_connectability()
                .contains(PortConnectability::INTERNAL);
            match (port, action, is_output) {
                (AnyBackendPort::Audio(port), Action::Connect, true) => {
                    channel.audio_connect_output(port)
                }
                (AnyBackendPort::Audio(port), Action::Disconnect, true) => {
                    channel.audio_disconnect(port)
                }
                (AnyBackendPort::Audio(port), Action::Connect, false) => {
                    channel.audio_connect_input(port)
                }
                (AnyBackendPort::Audio(port), Action::Disconnect, false) => {
                    channel.audio_disconnect(port)
                }
                (AnyBackendPort::Midi(port), Action::Connect, true) => {
                    channel.midi_connect_output(port)
                }
                (AnyBackendPort::Midi(port), Action::Disconnect, true) => {
                    channel.midi_disconnect(port)
                }
                (AnyBackendPort::Midi(port), Action::Connect, false) => {
                    channel.midi_connect_input(port)
                }
                (AnyBackendPort::Midi(port), Action::Disconnect, false) => {
                    channel.midi_disconnect(port)
                }
            }
        };

        let desired: Vec<*mut QObject> = self
            .ports_to_connect
            .iter()
            .filter_map(|value| qvariant_to_qobject_ptr(value).ok())
            .filter(|ptr| !ptr.is_null())
            .collect();
        let previous: Vec<*mut QObject> = self
            .ports_connected
            .iter()
            .filter_map(|value| qvariant_to_qobject_ptr(value).ok())
            .filter(|ptr| !ptr.is_null())
            .collect();

        for ptr in previous.iter().filter(|ptr| !desired.contains(ptr)) {
            if let Ok(port) = unsafe { PortGui::from_qobject_mut_ptr(*ptr) } {
                if let Some(handle) = port.maybe_backend_port.as_ref() {
                    apply(handle, Action::Disconnect);
                }
            }
        }

        let mut connected = QList_QVariant::default();
        for ptr in desired {
            let Ok(mut port) = (unsafe { PortGui::from_qobject_mut_ptr(ptr) }) else {
                continue;
            };
            if port.maybe_backend_port.is_none() {
                unsafe {
                    connect_or_report(
                        &*ptr,
                        "initialized_changed(bool)",
                        self.as_ref().get_ref(),
                        "update_port_connections()",
                        connection_types::QUEUED_CONNECTION,
                    );
                }
                continue;
            }
            if !previous.contains(&ptr) {
                if let Some(handle) = port.as_mut().rust_mut().maybe_backend_port.as_ref() {
                    apply(handle, Action::Connect);
                }
            }
            if let Ok(value) = qobject_ptr_to_qvariant(&ptr) {
                connected.append(value);
            }
        }

        if connected != self.ports_connected {
            self.as_mut().rust_mut().ports_connected = connected.clone();
            unsafe {
                self.as_mut().connected_ports_changed(connected);
            }
        }
    }

    pub fn push_mode(mut self: Pin<&mut LoopChannelGui>, mode: i32) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            trace!(self, "push mode: {mode}");
            match ChannelMode::try_from(mode) {
                Ok(m) => chan.set_mode(m),
                Err(e) => error!(self, "Invalid mode {mode}: {e}"),
            }
        } else {
            debug!(self, "mode (deferred) -> {mode}");
            match ChannelMode::try_from(mode) {
                Ok(m) => self.as_mut().rust_mut().prev_state.mode = m,
                Err(e) => error!(self, "Invalid mode {mode}: {e}"),
            }
        }
    }

    pub fn push_audio_gain(mut self: Pin<&mut LoopChannelGui>, audio_gain: f32) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            trace!(self, "push audio gain: {audio_gain}");
            chan.audio_set_gain(audio_gain as f32);
        } else {
            debug!(self, "gain (deferred) -> {audio_gain}");
            self.as_mut().rust_mut().prev_state.audio_gain = audio_gain as f32;
        }
    }

    pub fn push_n_preplay_samples(mut self: Pin<&mut LoopChannelGui>, n_preplay_samples: i32) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.set_n_preplay_samples(n_preplay_samples as u32);
        } else {
            debug!(self, "n preplay samples (deferred) -> {n_preplay_samples}");
            self.as_mut().rust_mut().prev_state.n_preplay_samples = n_preplay_samples as u32;
        }
    }

    pub fn set_ports_to_connect(
        mut self: Pin<&mut LoopChannelGui>,
        ports_to_connect: QList_QVariant,
    ) {
        if ports_to_connect == self.ports_to_connect {
            return;
        }
        self.as_mut().rust_mut().ports_to_connect = ports_to_connect.clone();
        unsafe {
            self.as_mut().ports_to_connect_changed(ports_to_connect);
        }
        self.as_mut().maybe_initialize_backend();
        self.as_mut().update_port_connections_impl();
    }

    pub fn push_start_offset(mut self: Pin<&mut LoopChannelGui>, start_offset: i32) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.set_start_offset(start_offset);
        } else {
            debug!(self, "start offset (deferred) -> {start_offset}");
            self.as_mut().rust_mut().prev_state.start_offset = start_offset as i32;
        }
    }

    pub fn load_audio_data(self: Pin<&mut LoopChannelGui>, data: QVector_f32) {
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not load audio data: not yet initialized");
        }
        let vec: Vec<f32> = data.iter().map(|v| *v).collect();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.audio_load_data(&vec);
        } else {
            error!(
                self,
                "could not load audio data: not yet initialized (option is None)"
            );
        }
    }

    pub fn load_midi_data(self: Pin<&mut LoopChannelGui>, data: QVector_QVariant) {
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not load MIDI data: not yet initialized");
        }
        let mut conversion_error: Option<anyhow::Error> = None;
        let vec: Vec<MidiEvent> = data
            .iter()
            .map(|v| match MidiEvent::from_qvariant(&v) {
                Ok(event) => event,
                Err(e) => {
                    conversion_error = Some(e);
                    MidiEvent {
                        time: 0,
                        data: Vec::default(),
                    }
                }
            })
            .collect();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.midi_load_data(&vec);
        } else {
            error!(
                self,
                "could not load MIDI data: not yet initialized (option is None)"
            );
        }
    }

    pub fn get_audio_data(self: Pin<&mut LoopChannelGui>) -> QVector_f32 {
        // NOTE: this is one of the few APIs which may be called from any thread.
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not get audio data: not yet initialized");
        }
        let mut rval: QVector_f32 = QVector::default();
        let vec = match self.maybe_backend_channel.as_ref() {
            Some(chan) => chan.audio_get_data(),
            None => {
                error!(
                    self,
                    "could not get audio data: not yet initialized (option is None)"
                );
                return QVector::default();
            }
        };
        rval.reserve(vec.len() as isize);
        vec.iter().for_each(|v| rval.append(*v));
        debug!(self, "extracted {} frames of audio data", rval.len());
        rval
    }

    pub fn get_midi_data(self: Pin<&mut LoopChannelGui>) -> QVector_QVariant {
        // NOTE: this is one of the few APIs which may be called from any thread.
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not get MIDI data: not yet initialized");
        }
        let mut rval: QVector_QVariant = QVector::default();
        let vec = match self.maybe_backend_channel.as_ref() {
            Some(chan) => chan.midi_get_data(),
            None => {
                error!(
                    self,
                    "could not get MIDI data: not yet initialized (option is None)"
                );
                return QVector::default();
            }
        };
        rval.reserve(vec.len() as isize);
        vec.iter().for_each(|v| rval.append(v.to_qvariant()));
        debug!(self, "extracted {} msgs of MIDI data", rval.len());
        rval
    }

    pub fn get_data_length(self: Pin<&mut LoopChannelGui>) -> i32 {
        self.prev_state.length as i32
    }

    pub fn reset_state_tracking(self: Pin<&mut LoopChannelGui>) {
        if let Some(channel) = self.maybe_backend_channel.as_ref() {
            channel.midi_reset_state_tracking();
        }
    }

    pub fn get_data(self: Pin<&mut LoopChannelGui>) -> QVector_QVariant {
        // NOTE: this is one of the few APIs which may be called from any thread.
        match self.data_type {
            Some(PortDataType::Audio) => {
                let mut variantlist: QVector_QVariant = QVector::default();
                let data = self.get_audio_data();
                for elem in data.iter() {
                    variantlist.append(QVariant::from(elem));
                }
                return variantlist;
            }
            Some(PortDataType::Midi) => {
                return self.get_midi_data();
            }
            _ => {
                error!(self, "Cannot get data: no data type found");
                return QVector::default();
            }
        }
    }

    pub fn get_data_async_and_send_to(
        mut self: Pin<&mut LoopChannelGui>,
        send_to_object: *mut QObject,
        method_signature: QString,
    ) -> *mut QObject {
        let self_qobj = unsafe { self.as_mut().pin_mut_qobject_ptr() };
        let async_task = unsafe { make_raw_async_task_with_parent(self_qobj) };
        let mut task = unsafe { Pin::new_unchecked(&mut *async_task) };
        task.as_mut().set_cpp_ownership();

        let mut setup = || -> Result<(), anyhow::Error> {
            if send_to_object.is_null() {
                return Err(anyhow!("target object is null"));
            }
            let channel = self
                .maybe_backend_channel
                .as_ref()
                .cloned()
                .ok_or(anyhow!("channel is not initialized"))?;
            let send_to_object = send_to_object as usize;
            let method_signature = method_signature.to_string();
            let is_midi = matches!(channel, AnyBackendChannel::Midi(_));
            let delivered_revision = Arc::clone(&self.last_delivered_data_revision);
            task.as_mut()
                .exec_concurrent_rust_then_finish(move || -> Result<(), anyhow::Error> {
                    let mut data = QVector_QVariant::default();
                    let revision;
                    if is_midi {
                        let read = channel.midi_latest_data()?;
                        revision = read.snapshot.revision;
                        for event in read.snapshot.events() {
                            data.append(event.to_qvariant());
                        }
                    } else {
                        let read = channel.audio_latest_data()?;
                        revision = read.snapshot.revision;
                        for sample in read.snapshot.samples() {
                            data.append(QVariant::from(&sample));
                        }
                    }
                    raw_trace!("Got {} channel data elements asynchronously", data.len());
                    let data = Box::into_raw(Box::new(data));
                    let data = QSharedPointer_QVector_QVariant::from_ptr(data)
                        .map_err(|_| anyhow!("failed to retain channel data"))?;
                    let data = qsharedpointer_qvector_qvariant_to_qvariant(&data)?;
                    let send_to_object = send_to_object as *mut QObject;
                    if !send_to_object.is_null() {
                        unsafe {
                            invoke::<_, (), _>(
                                &mut *send_to_object,
                                method_signature.as_str(),
                                connection_types::QUEUED_CONNECTION,
                                &(data),
                            )?;
                        }
                    }
                    delivered_revision.store(revision.0, Ordering::Release);
                    Ok(())
                });
            Ok(())
        };
        if let Err(error) = setup() {
            error!(self, "Failed to get data asynchronously: {error}");
            task.as_mut().finish_dummy();
        }
        unsafe { task.pin_mut_qobject_ptr() }
    }

    pub fn clear_data_dirty(self: Pin<&mut LoopChannelGui>) {
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            let revision = shoop_engine::ContentRevision(
                self.last_delivered_data_revision.load(Ordering::Acquire),
            );
            chan.acknowledge_data_revision(revision);
        }
    }

    pub fn get_connected_ports(self: Pin<&mut LoopChannelGui>) -> QList_QVariant {
        self.ports_connected.clone()
    }

    pub fn get_mode(&self) -> i32 {
        self.prev_state.mode as i32
    }

    pub fn get_start_offset(&self) -> i32 {
        self.prev_state.start_offset
    }

    pub fn get_n_preplay_samples(&self) -> i32 {
        self.prev_state.n_preplay_samples as i32
    }

    pub fn get_data_dirty(&self) -> bool {
        self.prev_state.data_dirty
    }

    pub fn get_last_played_sample(&self) -> QVariant {
        self.prev_state
            .played_back_sample
            .map(|sample| QVariant::from(&(sample as i32)))
            .unwrap_or_default()
    }

    pub fn get_audio_gain(&self) -> f32 {
        self.prev_state.audio_gain
    }

    pub fn get_audio_output_peak(&self) -> f32 {
        self.prev_state.audio_output_peak
    }

    pub fn get_midi_n_events_triggered(&self) -> i32 {
        self.prev_state.n_events_triggered as i32
    }

    pub fn get_midi_n_notes_active(&self) -> i32 {
        self.prev_state.n_notes_active as i32
    }

    pub fn clear(self: Pin<&mut LoopChannelGui>, length: i32) {
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not clear: not yet initialized");
        }
        debug!(self, "clear -> {length}");
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.clear(length as u32);
        } else {
            error!(
                self,
                "could not clear: not yet initialized (option is None)"
            );
        }
    }

    pub fn deinit(mut self: Pin<&mut LoopChannelGui>) {
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut.maybe_backend_channel = None;
        rust_mut.initialized = false;
        drop(rust_mut);
        unsafe {
            self.as_mut().initialized_changed(false);
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut module_name = String::from(module_name);
    let mut type_name = String::from(type_name);
    unsafe {
        register_qml_type_loop_channel_gui(
            std::ptr::null_mut(),
            &mut module_name,
            1,
            0,
            &mut type_name,
        );
    }
}
