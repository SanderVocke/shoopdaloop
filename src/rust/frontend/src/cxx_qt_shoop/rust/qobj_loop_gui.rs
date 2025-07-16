use crate::cxx_qt_lib_shoop::connect::connect_or_report;
use crate::cxx_qt_lib_shoop::connection_types;
use crate::cxx_qt_lib_shoop::qobject::ffi::qobject_move_to_thread;
use crate::cxx_qt_lib_shoop::qobject::ffi::qobject_object_name;
use crate::cxx_qt_lib_shoop::qobject::AsQObject;
use crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem;
use crate::cxx_qt_lib_shoop::qsharedpointer_qobject::QSharedPointer_QObject;
use crate::cxx_qt_lib_shoop::qvariant_qobject::qvariant_to_qobject_ptr;
use crate::cxx_qt_lib_shoop::qvariant_qsharedpointer_qobject::qsharedpointer_qobject_to_qvariant;
use crate::cxx_qt_shoop::qobj_loop_backend_bridge::ffi::loop_backend_qobject_from_ptr;
use crate::cxx_qt_shoop::qobj_loop_backend_bridge::ffi::make_raw_loop_backend;
use crate::cxx_qt_shoop::qobj_loop_backend_bridge::ffi::qobject_to_loop_backend_ptr;
use crate::cxx_qt_shoop::qobj_loop_gui_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_loop_gui_bridge::LoopGui;
use crate::engine_update_thread;
use backend_bindings::AudioChannel;
use backend_bindings::MidiChannel;
use common::logging::macros::{
    debug as raw_debug, error as raw_error, shoop_log_unit, trace as raw_trace,
};
use cxx_qt::CxxQtType;

use cxx_qt_lib::{QList, QString, QVariant};
use std::pin::Pin;
shoop_log_unit!("Frontend.Loop");

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

impl LoopGui {
    pub fn initialize_impl(mut self: Pin<&mut LoopGui>) {
        debug!(self, "Initializing");

        unsafe {
            // let backend_qobj : *mut QObject;
            // {
            //     let rust_mut = self.as_mut().rust_mut();
            //     backend_qobj = rust_mut.backend;
            // }
            // let backend_ptr : *mut BackendWrapper = BackendWrapper::from_qobject_ptr(backend_qobj);
            // let backend_thread = (*backend_ptr).get_backend_thread();

            // if backend_qobj.is_null() {
            //     raw_error!("Failed to convert backend QObject to backend pointer");
            // } else {
            let backend_loop = make_raw_loop_backend();
            let backend_loop_qobj = loop_backend_qobject_from_ptr(backend_loop);
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

                // Connections : update thread -> backend object
                connect_or_report(
                    backend_thread_wrapper,
                    "update()".to_string(),
                    backend_ref,
                    "update()".to_string(),
                    connection_types::DIRECT_CONNECTION,
                );

                // Connections : GUI -> GUI

                // Connections : backend object -> GUI
                connect_or_report(
                    backend_ref,
                    "stateChanged(::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
                    self_ref,
                    "on_backend_state_changed(::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
                    connection_types::QUEUED_CONNECTION);

                // Connections : GUI -> backend object
                connect_or_report(
                    self_ref,
                    "backend_set_position(::std::int32_t)".to_string(),
                    backend_ref,
                    "set_position(::std::int32_t)".to_string(),
                    connection_types::QUEUED_CONNECTION,
                );
                connect_or_report(
                    self_ref,
                    "backend_set_length(::std::int32_t)".to_string(),
                    backend_ref,
                    "set_length(::std::int32_t)".to_string(),
                    connection_types::QUEUED_CONNECTION,
                );
                connect_or_report(
                    self_ref,
                    "backend_clear(::std::int32_t)".to_string(),
                    backend_ref,
                    "clear(::std::int32_t)".to_string(),
                    connection_types::QUEUED_CONNECTION,
                );
                connect_or_report(
                    self_ref,
                    "backend_transition(::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
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
                    "backend_transition_multiple(QList_QVariant,::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
                    backend_ref,
                    "transition_multiple(QList_QVariant,::std::int32_t,::std::int32_t,::std::int32_t)".to_string(),
                    connection_types::QUEUED_CONNECTION);
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
                    "backend_set_sync_source(QVariant)".to_string(),
                    backend_ref,
                    "set_sync_source(QVariant)".to_string(),
                    connection_types::QUEUED_CONNECTION,
                );

                // Connections : backend object -> GUI
                connect_or_report(
                    backend_ref,
                    "initializedChanged(bool)".to_string(),
                    self_ref,
                    "set_initialized(bool)".to_string(),
                    connection_types::QUEUED_CONNECTION,
                );
            }

            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.backend_loop_wrapper =
                QSharedPointer_QObject::from_ptr_delete_later(backend_loop_qobj).unwrap();
        }
    }

    pub fn on_backend_state_changed(
        mut self: Pin<&mut LoopGui>,
        mode: i32,
        length: i32,
        position: i32,
        next_mode: i32,
        next_transition_delay: i32,
        cycle_nr: i32,
    ) {
        trace!(self, "on_backend_state_changed");
        let mut rust_mut = self.as_mut().rust_mut();
        let prev_mode = rust_mut.mode;
        let prev_length = rust_mut.length;
        let prev_position = rust_mut.position;
        let prev_next_mode = rust_mut.next_mode;
        let prev_next_transition_delay = rust_mut.next_transition_delay;
        let prev_cycle_nr = rust_mut.cycle_nr;

        rust_mut.mode = mode;
        rust_mut.length = length;
        rust_mut.position = position;
        rust_mut.next_mode = next_mode;
        rust_mut.next_transition_delay = next_transition_delay;
        rust_mut.cycle_nr = cycle_nr;

        unsafe {
            if mode != prev_mode {
                self.as_mut().mode_changed();
            }
            if length != prev_length {
                self.as_mut().length_changed();
            }
            if position != prev_position {
                self.as_mut().position_changed();
            }
            if next_mode != prev_next_mode {
                self.as_mut().next_mode_changed();
            }
            if next_transition_delay != prev_next_transition_delay {
                self.as_mut().next_transition_delay_changed();
            }
            if cycle_nr != prev_cycle_nr {
                self.as_mut().cycle_nr_changed();
                if (cycle_nr - prev_cycle_nr) == 1 {
                    self.as_mut().cycled(cycle_nr);
                }
            }
        }
    }

    pub fn close(self: Pin<&mut LoopGui>) {
        debug!(self, "close");
        self.rust_mut().backend_loop_wrapper = cxx::UniquePtr::null();
    }

    pub fn queue_set_length(self: Pin<&mut LoopGui>, length: i32) {
        debug!(self, "queue set length -> {}", length);
        self.backend_set_length(length);
    }

    pub fn queue_set_position(self: Pin<&mut LoopGui>, position: i32) {
        debug!(self, "queue set position -> {}", position);
        self.backend_set_position(position);
    }

    pub fn get_children_with_object_name(
        self: Pin<&mut LoopGui>,
        find_object_name: &str,
    ) -> QList<QVariant> {
        let mut result = QList_QVariant::default();
        unsafe {
            let ref_self = self.as_ref();
            let qquickitem = ref_self.qquickitem_ref();
            for child in qquickitem.child_items().iter().filter(|child| {
                let object_name =
                    qobject_object_name(qvariant_to_qobject_ptr(child).unwrap().as_ref().unwrap())
                        .unwrap();
                object_name == find_object_name
            }) {
                result.append(child.clone());
            }
        }
        result
    }

    pub fn get_audio_channels(self: Pin<&mut LoopGui>) -> QList<QVariant> {
        self.get_children_with_object_name("LoopAudioChannel")
    }

    pub fn get_midi_channels(self: Pin<&mut LoopGui>) -> QList<QVariant> {
        self.get_children_with_object_name("LoopMidiChannel")
    }

    pub fn transition_multiple(
        self: Pin<&mut LoopGui>,
        loops: QList_QVariant,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        raw_trace!(
            "GUI thread: transitioning {} loops to {} with delay {}, sync at cycle {}",
            loops.len(),
            to_mode,
            maybe_cycles_delay,
            maybe_to_sync_at_cycle
        );

        let backend_loop_handles = LoopGui::get_backend_loop_handles_variant_list(&loops).unwrap();
        self.backend_transition_multiple(
            backend_loop_handles,
            to_mode,
            maybe_cycles_delay,
            maybe_to_sync_at_cycle,
        );
    }

    pub fn get_backend_loop_handles_variant_list(
        loop_guis: &QList_QVariant,
    ) -> Result<QList_QVariant, anyhow::Error> {
        let mut backend_loop_handles: QList_QVariant = QList_QVariant::default();

        // Increment the reference count for all loops involved by storing shared pointers
        // into QVariants.
        loop_guis
            .iter()
            .map(|loop_variant| -> Result<(), anyhow::Error> {
                unsafe {
                    let loop_gui_qobj: *mut QObject = qvariant_to_qobject_ptr(loop_variant).ok_or(
                        anyhow::anyhow!("Failed to convert QVariant to QObject pointer"),
                    )?;
                    let loop_gui_ptr: *mut LoopGui = qobject_to_loop_ptr(loop_gui_qobj);
                    let backend_loop_ptr: &cxx::UniquePtr<QSharedPointer_QObject> =
                        &loop_gui_ptr.as_ref().ok_or(anyhow::anyhow!("Failed to get loop GUI reference"))?.backend_loop_wrapper;
                    if backend_loop_ptr.is_null() {
                        return Err(anyhow::anyhow!("Loop GUI has no back-end"));
                    }
                    let backend_loop_handle: QVariant =
                        qsharedpointer_qobject_to_qvariant(&backend_loop_ptr.as_ref().unwrap());
                    backend_loop_handles.append(backend_loop_handle);
                    Ok(())
                }
            })
            .for_each(|result| match result {
                Ok(_) => (),
                Err(err) => {
                    raw_error!("Failed to increment reference count for loop. This loop will be omitted. Details: {:?}", err)
                }
            });

        Ok(backend_loop_handles)
    }

    pub fn transition(
        self: Pin<&mut LoopGui>,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) {
        debug!(
            self,
            "Transitioning to {} with delay {}, sync at cycle {}",
            to_mode,
            maybe_cycles_delay,
            maybe_to_sync_at_cycle
        );
        self.backend_transition(to_mode, maybe_cycles_delay, maybe_to_sync_at_cycle);
    }

    pub fn add_audio_channel(
        self: Pin<&mut LoopGui>,
        mode: i32,
    ) -> Result<AudioChannel, anyhow::Error> {
        // TODO: this is not thread-safe. Once audio channels have been rustified,
        // make a better solution for this.
        let backend_wrapper = &self.as_ref().backend_loop_wrapper;
        if backend_wrapper.is_null() {
            return Err(anyhow::anyhow!(
                "Cannot add audio channel: no backend loop present"
            ));
        }
        let backend_wrapper_qobj = backend_wrapper.data().unwrap();
        unsafe {
            let backend_wrapper_obj = &mut *qobject_to_loop_backend_ptr(backend_wrapper_qobj);
            let maybe_backend_loop = backend_wrapper_obj.backend_loop.as_ref();
            if maybe_backend_loop.is_some() {
                debug!(self, "Created back-end audio channel");
                return maybe_backend_loop
                    .unwrap()
                    .add_audio_channel(mode.try_into()?);
            } else {
                return Err(anyhow::anyhow!("Backend loop not set"));
            }
        }
    }

    pub fn add_midi_channel(
        self: Pin<&mut LoopGui>,
        mode: i32,
    ) -> Result<MidiChannel, anyhow::Error> {
        // TODO: this is not thread-safe. Once midi channels have been rustified,
        // make a better solution for this.
        let backend_wrapper = &self.as_ref().backend_loop_wrapper;
        if backend_wrapper.is_null() {
            return Err(anyhow::anyhow!(
                "Cannot add midi channel: no backend loop present"
            ));
        }
        let backend_wrapper_qobj = backend_wrapper.data().unwrap();
        unsafe {
            let backend_wrapper_obj = &mut *qobject_to_loop_backend_ptr(backend_wrapper_qobj);
            let maybe_backend_loop = backend_wrapper_obj.backend_loop.as_ref();
            if maybe_backend_loop.is_some() {
                debug!(self, "Created back-end MIDI channel");
                return maybe_backend_loop
                    .unwrap()
                    .add_midi_channel(mode.try_into()?);
            } else {
                return Err(anyhow::anyhow!("Backend loop not set"));
            }
        }
    }

    pub fn clear(self: Pin<&mut LoopGui>, length: i32) {
        debug!(self, "Clearing to length {length}");
        self.backend_clear(length);
    }

    pub fn adopt_ringbuffers(
        self: Pin<&mut LoopGui>,
        maybe_reverse_start_cycle: QVariant,
        maybe_cycles_length: QVariant,
        maybe_go_to_cycle: QVariant,
        go_to_mode: i32,
    ) {
        debug!(self, "Adopting ringbuffers");
        self.backend_adopt_ringbuffers(
            maybe_reverse_start_cycle,
            maybe_cycles_length,
            maybe_go_to_cycle,
            go_to_mode,
        );
    }

    pub unsafe fn set_sync_source(mut self: Pin<&mut LoopGui>, sync_source: *mut QObject) {
        debug!(self, "sync source -> {:?}", sync_source);
        let changed = self.as_mut().rust_mut().sync_source != sync_source;
        self.as_mut().rust_mut().sync_source = sync_source;

        if changed {
            self.as_mut().sync_source_changed(sync_source);
        }

        self.update_backend_sync_source();
    }

    pub unsafe fn set_backend(mut self: Pin<&mut LoopGui>, backend: *mut QObject) {
        debug!(self, "backend -> {:?}", backend);
        let changed = self.as_mut().rust_mut().backend != backend;
        self.as_mut().rust_mut().backend = backend;

        if changed {
            self.as_mut().backend_changed(backend);
        }
    }

    pub unsafe fn set_instance_identifier(
        mut self: Pin<&mut LoopGui>,
        instance_identifier: QString,
    ) {
        debug!(self, "instance identifier -> {:?}", instance_identifier);
        let changed = self.as_mut().rust_mut().instance_identifier != instance_identifier;
        self.as_mut().rust_mut().instance_identifier = instance_identifier.clone();

        if changed {
            self.as_mut()
                .instance_identifier_changed(instance_identifier);
        }
    }

    pub fn set_initialized(mut self: Pin<&mut LoopGui>, initialized: bool) {
        debug!(self, "initialized -> {:?}", initialized);
        let changed = self.as_mut().rust_mut().initialized != initialized;
        self.as_mut().rust_mut().initialized = initialized;

        if changed {
            unsafe {
                self.as_mut().initialized_changed(initialized);
            }
        }
    }

    pub fn update_backend_sync_source(mut self: Pin<&mut LoopGui>) {
        debug!(self, "Updating backend sync source");
        let sync_source_in: *mut QObject = *self.sync_source();
        let mut sync_source_out: QVariant = QVariant::default();

        if !sync_source_in.is_null() {
            unsafe {
                let loop_gui_ptr: *mut LoopGui = qobject_to_loop_ptr(sync_source_in);
                let backend_loop_ptr: &cxx::UniquePtr<QSharedPointer_QObject> =
                    &loop_gui_ptr.as_ref().unwrap().backend_loop_wrapper;
                sync_source_out =
                    qsharedpointer_qobject_to_qvariant(&backend_loop_ptr.as_ref().unwrap());
            }
        }

        self.as_mut().backend_set_sync_source(sync_source_out);
    }

    pub fn get_backend_loop_wrapper(mut self: Pin<&mut LoopGui>) -> *mut QObject {
        let wrapper = &self.as_mut().backend_loop_wrapper;
        if wrapper.is_null() {
            return std::ptr::null_mut();
        }
        wrapper.data().unwrap()
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let obj = make_unique_loop();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_loop(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}
