use crate::any_backend_port::AnyBackendPort;
use crate::cxx_qt_shoop::qobj_loop_channel_backend_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_port_backend_bridge::PortBackend;
use crate::midi_event_helpers::MidiEventToQVariant;
use crate::{
    any_backend_channel::AnyBackendChannel, cxx_qt_shoop::qobj_loop_backend_bridge::LoopBackend,
};
use backend_bindings::{ChannelMode, MidiEvent, PortDataType};
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QList;
use cxx_qt_lib_shoop::qweakpointer_qobject::QWeakPointer_QObject;
use cxx_qt_lib_shoop::{
    connect::connect_or_report,
    connection_types,
    qobject::{qobject_property_bool, AsQObject, FromQObject},
    qsharedpointer_qobject::QSharedPointer_QObject,
    qvariant_helpers::qvariant_to_qsharedpointer_qobject,
};
use std::{collections::HashSet, pin::Pin};
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

impl LoopChannelBackend {
    pub fn initialize_impl(self: Pin<&mut LoopChannelBackend>) {}

    pub fn update(mut self: Pin<&mut LoopChannelBackend>) {
        if self.maybe_backend_channel.is_none() {
            return;
        }

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let channel = self.maybe_backend_channel.as_ref().unwrap();
            let prev_state = self.prev_state.clone();
            let new_state = channel.get_state()?;

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

    pub fn display_name(self: &LoopChannelBackend) -> String {
        // TODO
        "unknown".to_string()
    }

    pub fn maybe_initialize_backend(mut self: Pin<&mut LoopChannelBackend>) -> bool {
        match || -> Result<bool, anyhow::Error> {
            if self.initialized {
                return Ok(true);
            }

            let mut non_ready_vars: HashSet<String> = HashSet::new();
            let mut channel_loop: Option<cxx::UniquePtr<QSharedPointer_QObject>> = None;
            unsafe {
                if self.backend.is_null() {
                    non_ready_vars.insert("backend".to_string());
                }
                if !self.backend.is_null()
                    && !qobject_property_bool(self.backend.as_ref().unwrap(), "ready")
                        .unwrap_or(false)
                {
                    non_ready_vars.insert("backend_ready".to_string());
                }
                if self.channel_loop.is_none() {
                    non_ready_vars.insert("channel_loop".to_string());
                } else {
                    channel_loop = Some(self.channel_loop.as_ref().unwrap().to_strong()?);
                    if channel_loop.as_ref().unwrap().data()?.is_null() {
                        non_ready_vars.insert("channel_loop (null)".to_string());
                    } else {
                        if !qobject_property_bool(
                            channel_loop.as_ref().unwrap().data()?.as_ref().unwrap(),
                            "initialized",
                        )
                        .unwrap_or(false)
                        {
                            non_ready_vars.insert("channel_loop initialized".to_string());
                        }
                    }
                }
                if self.data_type.is_none() {
                    non_ready_vars.insert("data_type".to_string());
                }
            }
            let initialize_condition: bool = !self.initialized && non_ready_vars.is_empty();

            if initialize_condition {
                unsafe {
                    let channel_loop =
                        LoopBackend::from_qobject_ref_ptr(channel_loop.as_ref().unwrap().data()?)?;
                    let channel_loop = channel_loop
                        .backend_loop
                        .as_ref()
                        .ok_or(anyhow::anyhow!("No backend loop in loop object"))?;
                    let mode = ChannelMode::try_from(self.prev_state.mode as i32)?;
                    let backend_channel = match self.data_type.unwrap() {
                        PortDataType::Audio => {
                            AnyBackendChannel::Audio(channel_loop.add_audio_channel(mode)?)
                        }
                        PortDataType::Midi => {
                            AnyBackendChannel::Midi(channel_loop.add_midi_channel(mode)?)
                        }
                        PortDataType::Any => {
                            return Err(anyhow::anyhow!("No specific port data type"));
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

    pub fn get_initialized(self: &LoopChannelBackend) -> bool {
        self.initialized
    }

    pub unsafe fn set_backend(mut self: Pin<&mut LoopChannelBackend>, backend: *mut QObject) {
        if self.maybe_backend_channel.is_some() {
            error!(
                self,
                "Can't change backend after backend has been initialized"
            );
            return;
        }
        if self.backend != backend {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.backend = backend;
            unsafe {
                if !backend.is_null() {
                    let self_qobject = loop_channel_backend_qobject_from_ptr(
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
                }
                self.as_mut().backend_changed(backend);
            }
        }
        self.as_mut().maybe_initialize_backend();
    }

    pub fn set_is_midi(mut self: Pin<&mut LoopChannelBackend>, is_midi: bool) {
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

    pub fn set_channel_loop(mut self: Pin<&mut LoopChannelBackend>, channel_loop: QVariant) {
        if self.maybe_backend_channel.is_some() {
            error!(self, "cannot set loop after initialization");
            return;
        };

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let shared = qvariant_to_qsharedpointer_qobject(&channel_loop)?;
            let weak = QWeakPointer_QObject::from_strong(&shared);
            let raw = shared.data()?;
            debug!(self, "channel loop -> {raw:?}");
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.channel_loop = Some(weak);
            unsafe {
                self.as_mut().channel_loop_changed(raw);
            }
            Ok(())
        }() {
            error!(self, "cannot set loop: {e}");
        }
        self.as_mut().maybe_initialize_backend();
    }

    pub fn get_is_midi(self: Pin<&mut LoopChannelBackend>) -> bool {
        self.data_type.unwrap_or(PortDataType::Audio) == PortDataType::Midi
    }

    pub fn get_channel_loop(self: Pin<&mut LoopChannelBackend>) -> *mut QObject {
        match self.channel_loop.as_ref() {
            Some(channel_loop) => match channel_loop.as_ref() {
                Some(channel_loop) => match channel_loop.to_strong() {
                    Ok(channel_loop) => channel_loop.data().unwrap_or(std::ptr::null_mut()),
                    Err(_) => std::ptr::null_mut(),
                },
                None => std::ptr::null_mut(),
            },
            None => std::ptr::null_mut(),
        }
    }

    pub fn update_port_connections_impl(mut self: Pin<&mut LoopChannelBackend>) {
        if !self.maybe_backend_channel.is_some() {
            return;
        }

        enum Action {
            Connect,
            Disconnect,
        }
        let do_port = |channel: &AnyBackendChannel, port: &AnyBackendPort, action: Action| {
            match port.input_connectability().internal {
                true => {
                    // output port
                    match port {
                        AnyBackendPort::Audio(audio_port) => match action {
                            Action::Connect => {
                                channel.audio_connect_output(audio_port);
                            }
                            Action::Disconnect => {
                                channel.audio_disconnect(audio_port);
                            }
                        },
                        AnyBackendPort::Midi(midi_port) => match action {
                            Action::Connect => {
                                channel.midi_connect_output(midi_port);
                            }
                            Action::Disconnect => {
                                channel.midi_disconnect(midi_port);
                            }
                        },
                    }
                }
                false => {
                    // input port
                    match port {
                        AnyBackendPort::Audio(audio_port) => match action {
                            Action::Connect => {
                                channel.audio_connect_input(audio_port);
                            }
                            Action::Disconnect => {
                                channel.audio_disconnect(audio_port);
                            }
                        },
                        AnyBackendPort::Midi(midi_port) => match action {
                            Action::Connect => {
                                channel.midi_connect_input(midi_port);
                            }
                            Action::Disconnect => {
                                channel.midi_disconnect(midi_port);
                            }
                        },
                    }
                }
            }
        };

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let strong_to_connect: Vec<(cxx::UniquePtr<QSharedPointer_QObject>, *mut QObject)> =
                self.ports_to_connect
                    .iter()
                    .map(
                        |weak| -> Result<cxx::UniquePtr<QSharedPointer_QObject>, anyhow::Error> {
                            Ok(weak.to_strong()?)
                        },
                    )
                    .filter(|result| {
                        result.is_ok()
                            && !result
                                .as_ref()
                                .unwrap()
                                .data()
                                .unwrap_or(std::ptr::null_mut())
                                .is_null()
                    })
                    .map(|result| result.unwrap())
                    .map(|strong| (strong.copy().unwrap(), strong.data().unwrap()))
                    .collect();

            let mut strong_connected: Vec<(cxx::UniquePtr<QSharedPointer_QObject>, *mut QObject)> =
                self.ports_connected
                    .iter()
                    .map(
                        |weak| -> Result<cxx::UniquePtr<QSharedPointer_QObject>, anyhow::Error> {
                            Ok(weak.to_strong()?)
                        },
                    )
                    .filter(|result| {
                        result.is_ok()
                            && !result
                                .as_ref()
                                .unwrap()
                                .data()
                                .unwrap_or(std::ptr::null_mut())
                                .is_null()
                    })
                    .map(|result| result.unwrap())
                    .map(|strong| (strong.copy().unwrap(), strong.data().unwrap()))
                    .collect();

            let amount_to_connect = strong_to_connect.len();
            let amount_connected = strong_to_connect.len();
            debug!(
                self,
                "{amount_connected} already connected, {amount_to_connect} to connect"
            );

            // Disconnect and remove any we don't want
            strong_connected.retain(|(_, connected_port_raw)| {
                if connected_port_raw.is_null() {
                    false
                } else if !strong_to_connect
                    .iter()
                    .any(|(_, raw)| *raw == *connected_port_raw)
                {
                    // We should disconnect
                    match unsafe { PortBackend::from_qobject_mut_ptr(*connected_port_raw) } {
                        Ok(backend_port) => match backend_port.maybe_backend_port.as_ref() {
                            Some(port) => {
                                let name = backend_port.get_name();
                                debug!(self, "disconnect from {name:?}");
                                do_port(
                                    self.maybe_backend_channel.as_ref().unwrap(),
                                    port,
                                    Action::Disconnect,
                                );
                                false
                            }
                            None => false,
                        },
                        Err(_) => false,
                    }
                } else {
                    true
                }
            });

            // Connect any newly added
            strong_to_connect
                .iter()
                .for_each(|(to_connect_strong, to_connect_raw)| {
                    if to_connect_raw.is_null() {
                        return;
                    } else if !strong_connected
                        .iter()
                        .any(|(_, connected_raw)| *connected_raw == *to_connect_raw)
                    {
                        // We should connect
                        match unsafe { PortBackend::from_qobject_mut_ptr(*to_connect_raw) } {
                            Ok(backend_port) => match backend_port.maybe_backend_port.as_ref() {
                                Some(port) => {
                                    let name = backend_port.get_name();
                                    debug!(self, "connect to {name:?}");
                                    do_port(
                                        self.maybe_backend_channel.as_ref().unwrap(),
                                        port,
                                        Action::Connect,
                                    );
                                    strong_connected
                                        .push((to_connect_strong.copy().unwrap(), *to_connect_raw));
                                }
                                None => {
                                    // Subscribe for future changes
                                    unsafe {
                                        connect_or_report(
                                            &**to_connect_raw,
                                            "initialized_changed(bool)",
                                            &*self.as_mut().pin_mut_qobject_ptr(),
                                            "update_port_connections()",
                                            connection_types::QUEUED_CONNECTION,
                                        );
                                    }
                                }
                            },
                            Err(e) => {
                                error!(self, "Cannot connect to port: {e}");
                            }
                        }
                    }
                });

            // Store our new connection list
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.ports_connected = strong_connected
                .iter()
                .map(|(strong, _)| QWeakPointer_QObject::from_strong(strong))
                .collect();

            Ok(())
        }() {
            error!(self, "Could not update port connections: {e}");
        }
    }

    pub fn push_mode(mut self: Pin<&mut LoopChannelBackend>, mode: i32) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.set_mode(ChannelMode::try_from(mode).unwrap());
        } else {
            debug!(self, "mode (deferred) -> {mode}");
            self.as_mut().rust_mut().prev_state.mode = ChannelMode::try_from(mode).unwrap();
        }
    }

    pub fn push_audio_gain(mut self: Pin<&mut LoopChannelBackend>, audio_gain: f32) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.audio_set_gain(audio_gain as f32);
        } else {
            debug!(self, "gain (deferred) -> {audio_gain}");
            self.as_mut().rust_mut().prev_state.audio_gain = audio_gain as f32;
        }
    }

    pub fn push_n_preplay_samples(mut self: Pin<&mut LoopChannelBackend>, n_preplay_samples: i32) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.set_n_preplay_samples(n_preplay_samples as u32);
        } else {
            debug!(self, "n preplay samples (deferred) -> {n_preplay_samples}");
            self.as_mut().rust_mut().prev_state.n_preplay_samples = n_preplay_samples as u32;
        }
    }

    pub fn set_ports_to_connect(
        mut self: Pin<&mut LoopChannelBackend>,
        ports_to_connect: QList_QVariant,
    ) {
        self.as_mut().maybe_initialize_backend();
        if let Err(e) =
            || -> Result<(), anyhow::Error> {
                let weak_ptrs: Vec<cxx::UniquePtr<QWeakPointer_QObject>> = ports_to_connect
                .iter()
                .map(
                    |port_qvariant| -> Result<cxx::UniquePtr<QWeakPointer_QObject>, anyhow::Error> {
                        let strong = qvariant_to_qsharedpointer_qobject(port_qvariant)
                            .map_err(|e| { debug!(self, "could not convert to shared: {e}"); e})?;
                        let addr = strong.data();
                        debug!(self, "got shared ptr: {addr:?}");
                        let weak = QWeakPointer_QObject::from_strong(&strong);
                        Ok(weak)
                    },
                )
                .collect::<Result<Vec<cxx::UniquePtr<QWeakPointer_QObject>>, anyhow::Error>>()?;

                let amount = weak_ptrs.len();
                debug!(self, "ports to connect -> amount {amount}");

                self.as_mut().rust_mut().ports_to_connect = weak_ptrs;
                self.as_mut().update_port_connections_impl();
                Ok(())
            }()
        {
            error!(self, "Could not set ports to connect: {e}");
        }
    }

    pub fn push_start_offset(mut self: Pin<&mut LoopChannelBackend>, start_offset: i32) {
        self.as_mut().maybe_initialize_backend();
        if let Some(chan) = self.maybe_backend_channel.as_ref() {
            chan.set_start_offset(start_offset);
        } else {
            debug!(self, "start offset (deferred) -> {start_offset}");
            self.as_mut().rust_mut().prev_state.start_offset = start_offset as i32;
        }
    }

    pub fn load_audio_data(self: Pin<&mut LoopChannelBackend>, data: QList_f32) {
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not load audio data: not yet initialized");
        }
        let vec: Vec<f32> = data.iter().map(|v| *v).collect();
        self.maybe_backend_channel
            .as_ref()
            .unwrap()
            .audio_load_data(&vec);
    }

    pub fn load_midi_data(self: Pin<&mut LoopChannelBackend>, data: QList_QVariant) {
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not load MIDI data: not yet initialized");
        }
        let mut conversion_error : Option<anyhow::Error> = None;
        let vec: Vec<MidiEvent> = data.iter().map(|v| {
            match MidiEvent::from_qvariant(&v) {
                Ok(event) => event,
                Err(e) => {
                    conversion_error = Some(e);
                    MidiEvent {
                        time: 0,
                        data: Vec::default(),
                    }
                }
            }
        }).collect();
        self.maybe_backend_channel
            .as_ref()
            .unwrap()
            .midi_load_data(&vec);
    }

    pub fn get_audio_data(self: Pin<&mut LoopChannelBackend>) -> QList_f32 {
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not get audio data: not yet initialized");
        }
        let mut rval : QList_f32 = QList::default();
        let vec = self.maybe_backend_channel.as_ref().unwrap().audio_get_data();
        rval.reserve(vec.len() as isize);
        vec.iter().for_each(|v| rval.append(*v));
        debug!(self, "extracted {} frames of audio data", rval.len());
        rval
    }

    pub fn get_midi_data(self: Pin<&mut LoopChannelBackend>) -> QList_QVariant {
        if self.maybe_backend_channel.is_none() {
            error!(self, "could not get MIDI data: not yet initialized");
        }
        let mut rval : QList_QVariant = QList::default();
        let vec = self.maybe_backend_channel.as_ref().unwrap().midi_get_data();
        rval.reserve(vec.len() as isize);
        vec.iter().for_each(|v| rval.append(v.to_qvariant()));
        debug!(self, "extracted {} msgs of MIDI data", rval.len());
        rval
    }

    pub fn get_data_length(self: Pin<&mut LoopChannelBackend>) -> i32 {
        self.prev_state.length as i32
    }
}
