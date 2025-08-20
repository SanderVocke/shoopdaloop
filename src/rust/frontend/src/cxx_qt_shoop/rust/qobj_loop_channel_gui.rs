use crate::cxx_qt_shoop::qobj_port_gui_bridge::PortGui;
use crate::cxx_qt_shoop::rust::qobj_loop_channel_backend_bridge::ffi::*;
use crate::cxx_qt_shoop::rust::qobj_loop_channel_gui_bridge::ffi::*;
use crate::cxx_qt_shoop::rust::qobj_loop_channel_gui_bridge::ffi::{
    QList_QVariant, QList_f32, QObject, QVariant,
};
use crate::engine_update_thread;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QMap};
use cxx_qt_lib_shoop::connect::connect_or_report;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::qobject::{AsQObject, FromQObject};
use cxx_qt_lib_shoop::qsharedpointer_qobject::QSharedPointer_QObject;
use cxx_qt_lib_shoop::qvariant_helpers::{
    qsharedpointer_qobject_to_qvariant, qvariant_to_qobject_ptr,
    qvariant_to_qsharedpointer_qobject, qweakpointer_qobject_to_qvariant,
};
use cxx_qt_lib_shoop::qweakpointer_qobject::QWeakPointer_QObject;
use cxx_qt_lib_shoop::{invokable, qobject::ffi::qobject_move_to_thread};
use std::pin::Pin;
shoop_log_unit!("Frontend.LoopChannel");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}] {}", $self.display_name().to_string(), format!($($arg)*))
    };
}

impl LoopChannelGui {
    pub fn display_name(self: &Self) -> String {
        // TODO
        String::from("unknown")
    }

    pub fn initialize_impl(mut self: Pin<&mut LoopChannelGui>) {
        debug!(self, "Initializing");

        unsafe {
            let backend_channel = make_raw_loop_channel_backend();
            let backend_channel_qobj = loop_channel_backend_qobject_from_ptr(backend_channel);
            qobject_move_to_thread(
                backend_channel_qobj,
                engine_update_thread::get_engine_update_thread().thread,
            )
            .unwrap();

            let self_ref = self.as_ref().get_ref();

            {
                let backend_ref = &*backend_channel_qobj;
                let backend_thread_wrapper =
                    &*engine_update_thread::get_engine_update_thread().ref_qobject_ptr();

                {
                    // Connections: update thread -> backend object
                    connect_or_report(
                        backend_thread_wrapper,
                        "update()",
                        backend_ref,
                        "update()",
                        connection_types::DIRECT_CONNECTION,
                    );
                }
                {
                    // Connections: GUI -> backend object
                    connect_or_report(
                        self_ref,
                        "backend_set_backend(QObject*)",
                        backend_ref,
                        "set_backend(QObject*)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_is_midi(bool)",
                        backend_ref,
                        "set_is_midi(bool)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_ports_to_connect(QList<QVariant>)",
                        backend_ref,
                        "set_ports_to_connect(QList<QVariant>)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_mode(::std::int32_t)",
                        backend_ref,
                        "push_mode(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_start_offset(::std::int32_t)",
                        backend_ref,
                        "push_start_offset(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_n_preplay_samples(::std::int32_t)",
                        backend_ref,
                        "push_n_preplay_samples(::std::int32_t)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_push_audio_gain(float)",
                        backend_ref,
                        "push_audio_gain(float)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_set_channel_loop(QVariant)",
                        backend_ref,
                        "set_channel_loop(QVariant)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_load_audio_data(QList<float>)",
                        backend_ref,
                        "load_audio_data(QList<float>)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        self_ref,
                        "backend_load_midi_data(QList<QVariant>)",
                        backend_ref,
                        "load_midi_data(QList<QVariant>)",
                        connection_types::QUEUED_CONNECTION,
                    );
                }
                {
                    // Connections: backend object -> GUI
                    connect_or_report(
                        backend_ref,
                        "state_changed(bool,::std::int32_t,::std::int32_t,::std::int32_t,QVariant,::std::int32_t,bool,float,float,::std::int32_t,::std::int32_t)",
                        self_ref,
                        "backend_state_changed(bool,::std::int32_t,::std::int32_t,::std::int32_t,QVariant,::std::int32_t,bool,float,float,::std::int32_t,::std::int32_t)",
                        connection_types::QUEUED_CONNECTION
                    );
                }

                let mut rust_mut = self.as_mut().rust_mut();
                rust_mut.backend_channel_wrapper =
                    QSharedPointer_QObject::from_ptr_delete_later(backend_channel_qobj).unwrap();
            }
        }
    }

    pub fn set_backend(mut self: Pin<&mut LoopChannelGui>, backend: *mut QObject) {
        unsafe {
            self.as_mut().backend_set_backend(backend);
        }
        if backend != self.backend {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.backend = backend;
            unsafe {
                self.as_mut().backend_changed(backend);
            }
        }
    }

    pub fn set_is_midi(mut self: Pin<&mut LoopChannelGui>, is_midi: bool) {
        unsafe {
            self.as_mut().backend_set_is_midi(is_midi);
        }
        if is_midi != self.is_midi {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.is_midi = is_midi;
            unsafe {
                self.as_mut().is_midi_changed(is_midi);
            }
        }
    }

    pub fn set_ports_to_connect(mut self: Pin<&mut LoopChannelGui>, ports: QList_QVariant) {
        unsafe {
            // We are getting a list of QObjects. For the back-end side
            // we want to create shared pointers to the back-end wrapper
            // objects instead.
            let mut shared_ptrs: QList_QVariant = QList::default();
            ports.iter().for_each(|variant| {
                if let Ok(obj) = qvariant_to_qobject_ptr(&variant) {
                    if let Ok(chan) = PortGui::from_qobject_mut_ptr(obj) {
                        shared_ptrs.append(
                            qsharedpointer_qobject_to_qvariant(
                                &chan.backend_port_wrapper.as_ref().unwrap(),
                            )
                            .unwrap(),
                        );
                    }
                }
            });
            self.as_mut().backend_set_ports_to_connect(shared_ptrs);
        }
        if ports != self.ports_to_connect {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.ports_to_connect = ports.clone();
            unsafe {
                self.as_mut().ports_to_connect_changed(ports);
            }
        }
    }

    pub fn push_mode(self: Pin<&mut LoopChannelGui>, mode: i32) {
        unsafe {
            self.backend_push_mode(mode);
        }
    }

    pub fn push_start_offset(self: Pin<&mut LoopChannelGui>, start_offset: i32) {
        unsafe {
            self.backend_push_start_offset(start_offset);
        }
    }

    pub fn push_n_preplay_samples(self: Pin<&mut LoopChannelGui>, n_preplay_samples: i32) {
        unsafe {
            self.backend_push_n_preplay_samples(n_preplay_samples);
        }
    }

    pub fn push_audio_gain(self: Pin<&mut LoopChannelGui>, audio_gain: f32) {
        unsafe {
            self.backend_push_audio_gain(audio_gain);
        }
    }

    pub fn backend_state_changed(
        mut self: Pin<&mut LoopChannelGui>,
        initialized: bool,
        mode: i32,
        length: i32,
        start_offset: i32,
        played_back_sample: QVariant,
        n_preplay_samples: i32,
        data_dirty: bool,
        audio_gain: f32,
        output_peak: f32,
        n_events_triggered: i32,
        n_notes_active: i32,
    ) {
        if initialized != self.initialized {
            debug!(self, "initialized -> {initialized}");
            self.as_mut().rust_mut().initialized = initialized;
            unsafe {
                self.as_mut().initialized_changed(initialized);
            }
        }
        if mode != self.mode {
            debug!(self, "mode -> {mode}");
            self.as_mut().rust_mut().mode = mode;
            unsafe {
                self.as_mut().mode_changed(mode);
            }
        }
        if length != self.data_length {
            debug!(self, "data_length -> {length}");
            self.as_mut().rust_mut().data_length = length;
            unsafe {
                self.as_mut().data_length_changed(length);
            }
        }
        if start_offset != self.start_offset {
            debug!(self, "start_offset -> {start_offset}");
            self.as_mut().rust_mut().start_offset = start_offset;
            unsafe {
                self.as_mut().start_offset_changed(start_offset);
            }
        }
        if played_back_sample != self.last_played_sample {
            let value = QVariant::value::<i32>(&played_back_sample);
            trace!(self, "last_played_sample -> {value:?}");
            self.as_mut().rust_mut().last_played_sample = played_back_sample.clone();
            unsafe {
                self.as_mut().last_played_sample_changed(played_back_sample);
            }
        }
        if n_preplay_samples != self.n_preplay_samples {
            debug!(self, "n_preplay_samples -> {n_preplay_samples}");
            self.as_mut().rust_mut().n_preplay_samples = n_preplay_samples;
            unsafe {
                self.as_mut().n_preplay_samples_changed(n_preplay_samples);
            }
        }
        if data_dirty != self.data_dirty {
            trace!(self, "data_dirty -> {data_dirty}");
            self.as_mut().rust_mut().data_dirty = data_dirty;
            unsafe {
                self.as_mut().data_dirty_changed(data_dirty);
            }
        }
        if audio_gain != self.audio_gain {
            debug!(self, "audio_gain -> {audio_gain}");
            self.as_mut().rust_mut().audio_gain = audio_gain;
            unsafe {
                self.as_mut().audio_gain_changed(audio_gain);
            }
        }
        if output_peak != self.audio_output_peak {
            trace!(self, "audio_output_peak -> {output_peak}");
            self.as_mut().rust_mut().audio_output_peak = output_peak;
            unsafe {
                self.as_mut().audio_output_peak_changed(output_peak);
            }
        }
        if n_events_triggered != self.midi_n_events_triggered {
            trace!(self, "midi_n_events_triggered -> {n_events_triggered}");
            self.as_mut().rust_mut().midi_n_events_triggered = n_events_triggered;
            unsafe {
                self.as_mut()
                    .midi_n_events_triggered_changed(n_events_triggered);
            }
        }
        if n_notes_active != self.midi_n_notes_active {
            trace!(self, "midi_n_notes_active -> {n_notes_active}");
            self.as_mut().rust_mut().midi_n_notes_active = n_notes_active;
            unsafe {
                self.as_mut().midi_n_notes_active_changed(n_notes_active);
            }
        }
    }

    pub fn get_channel_loop(self: Pin<&mut LoopChannelGui>) -> *mut QObject {
        self.channel_loop
    }

    pub fn set_channel_loop(mut self: Pin<&mut LoopChannelGui>, channel_loop: *mut QObject) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            unsafe {
                let backend_loop: QVariant = invokable::invoke(
                    &mut *channel_loop,
                    "get_backend_loop_wrapper()",
                    invokable::DIRECT_CONNECTION,
                    &(),
                )?;
                self.as_mut().backend_set_channel_loop(backend_loop.clone());
                if self.channel_loop != channel_loop {}
                self.as_mut().rust_mut().channel_loop = channel_loop;
                self.as_mut().channel_loop_changed(channel_loop);
            }
            Ok(())
        }() {
            error!(self, "Could not update channel loop: {e}");
        }
    }

    pub fn load_audio_data(self: Pin<&mut LoopChannelGui>, data: QList_f32) {
        unsafe {
            self.backend_load_audio_data(data);
        }
    }

    pub fn load_midi_data(self: Pin<&mut LoopChannelGui>, data: QList_QVariant) {
        unsafe {
            self.backend_load_midi_data(data);
        }
    }

    pub fn get_audio_data(self: &LoopChannelGui) -> QList_f32 {
        todo!();
    }

    pub fn get_midi_data(self: &LoopChannelGui) -> QList_QVariant {
        todo!();
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_loop_channel_gui(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
