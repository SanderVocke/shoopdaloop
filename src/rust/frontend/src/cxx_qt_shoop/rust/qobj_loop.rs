use backend_bindings::AudioChannel;
use backend_bindings::MidiChannel;
use common::logging::macros::{shoop_log_unit, debug as raw_debug, trace as raw_trace, error as raw_error};
use cxx_qt::ConnectionType;
use cxx_qt::CxxQtType;
use crate::cxx_qt_lib_shoop;
use crate::cxx_qt_lib_shoop::connect;
use crate::cxx_qt_lib_shoop::connect::connect;
use crate::cxx_qt_lib_shoop::connection_types;
use crate::cxx_qt_lib_shoop::invokable::invoke;
use crate::cxx_qt_lib_shoop::qobject::ffi::qobject_object_name;
use crate::cxx_qt_lib_shoop::qobject::{qobject_property_bool, qobject_property_float, qobject_property_int};
use crate::cxx_qt_lib_shoop::qquickitem::AsQQuickItem;
use crate::cxx_qt_lib_shoop::qvariant_helpers::qvariant_to_qobject_ptr;
use crate::cxx_qt_shoop::qobj_backend_wrapper::qobject_ptr_to_backend_ptr;
use crate::cxx_qt_shoop::qobj_loop_bridge::Loop;                                   
use crate::cxx_qt_shoop::qobj_loop_bridge::ffi::*;  
use crate::loop_mode_helpers::*;   
use cxx_qt_lib::{QList, QVariant, QString};                                    
use std::ops::Deref;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
shoop_log_unit!("Frontend.Loop");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*));
    };
}

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*));
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}] {}", $self.instance_identifier().to_string(), format!($($arg)*));
    };
}

impl Loop {
    pub fn initialize_impl(mut self: Pin<&mut Loop>) {
        debug!(self, "Initializing");

        {
            self.as_mut().connect_mode_changed(|o| { o.mode_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_next_mode_changed(|o| { o.next_mode_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_length_changed(|o| { o.length_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_position_changed(|o| { o.position_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_next_transition_delay_changed(|o| { o.next_transition_delay_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_display_peaks_changed(|o| { o.display_peaks_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_display_midi_notes_active_changed(|o| { o.display_midi_notes_active_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_display_midi_events_triggered_changed(|o| { o.display_midi_events_triggered_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_cycle_nr_changed(|o| { o.cycle_nr_changed_queued(); }, ConnectionType::QueuedConnection).release();
            self.as_mut().connect_cycled(|o, cycle_nr| { o.cycled_queued(cycle_nr); }, ConnectionType::QueuedConnection).release();

            // FIXME: Now this will only initialize the loop
            // if the backend was already initialized. Check the "ready"
            // property and if not ready, connect a one-time signal to initialize
            // when ready.
            self.as_mut().connect_backend_changed(|o| { o.maybe_initialize_backend(); }, ConnectionType::QueuedConnection).release();
        }
    }

    pub fn queue_set_length(mut self: Pin<&mut Loop>, length: i32) {
        if !self.initialized() {
            error!(self, "queue_set_position: not initialized");
            return;
        }
        debug!(self, "queue set length -> {}", length);
        let mut rust = self.as_mut().rust_mut();
        let loop_arc = rust.backend_loop.as_mut().unwrap().clone();
        let loop_obj = loop_arc.lock().expect("Backend loop mutex lock failed");
        loop_obj.set_length(length as u32);
    }

    pub fn queue_set_position(mut self: Pin<&mut Loop>, position: i32) {
        if !self.initialized() {
            error!(self, "queue_set_position: not initialized");
            return;
        }
        debug!(self, "queue set position -> {}", position);
        let mut rust = self.as_mut().rust_mut();
        let loop_arc = rust.backend_loop.as_mut().unwrap().clone();
        let loop_obj = loop_arc.lock().expect("Backend loop mutex lock failed");
        loop_obj.set_position(position as u32);
    }

    pub fn update_on_non_gui_thread(mut self: Pin<&mut Loop>) {
        if !self.initialized() {
            return;
        }

        self.as_mut().starting_update_on_non_gui_thread();

        let loop_arc = self.as_mut().backend_loop.as_ref().expect("Backend loop not set").clone();
        let loop_obj = loop_arc.lock().expect("Backend loop mutex lock failed");
        let state = loop_obj.get_state().unwrap();

        let audio_chans : Vec<*mut QObject> = self.as_mut().get_audio_channels()
                                       .iter()
                                       .map(|v| qvariant_to_qobject_ptr(v).unwrap())
                                       .collect();
        let midi_chans : Vec<*mut QObject> = self.as_mut().get_midi_channels()
                                      .iter()
                                      .map(|v| qvariant_to_qobject_ptr(v).unwrap())
                                      .collect();

        let mut rust = self.as_mut().rust_mut();
        
        let prev_position = rust.position;
        let prev_mode = rust.mode;
        let prev_length = rust.length;
        let prev_next_mode = rust.next_mode;
        let prev_next_delay = rust.next_transition_delay;
        let prev_display_peaks : Vec<f32> = rust.display_peaks.iter().map(|f| f.clone()).collect();
        let prev_display_midi_notes_active = rust.display_midi_notes_active.clone();
        let prev_display_midi_events_triggered = rust.display_midi_events_triggered.clone();
        let prev_cycle_nr : i32 = rust.cycle_nr;

        let new_mode = state.mode as i32;
        let new_next_mode = state.maybe_next_mode.unwrap_or(state.mode) as i32;
        let new_length = state.length as i32;
        let new_position = state.position as i32;
        let new_next_transition_delay = state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32;
        let new_display_peaks : Vec<f32> =
           audio_chans.iter()
           .filter(|qobj| {
                unsafe {
                    let mode = qobject_property_int(qobj.as_ref().unwrap(), "mode".to_string()).unwrap();
                    mode == backend_bindings::ChannelMode::Direct as i32 ||
                       mode == backend_bindings::ChannelMode::Wet as i32
                }
             })
              .map(|qobj| {
                 unsafe {
                      let peak = qobject_property_float(qobj.as_ref().unwrap(), "output_peak".to_string()).unwrap();
                      peak as f32
                 }
                }).collect();
        let display_peaks_changed = prev_display_peaks != new_display_peaks;
        let new_display_midi_notes_active : i32 = midi_chans.iter().map(|qobj| -> i32 {
            unsafe {
                let n_notes_active = qobject_property_int(qobj.as_ref().unwrap(), "n_notes_active".to_string()).unwrap();
                n_notes_active
            }
        }).sum();
        let new_display_midi_events_triggered : i32 = midi_chans.iter().map(|qobj| -> i32 {
            unsafe {
                let n_events_triggered = qobject_property_int(qobj.as_ref().unwrap(), "n_events_triggered".to_string()).unwrap();
                n_events_triggered
            }
        }).sum();
        let new_cycle_nr : i32 = 
            if (new_position < prev_position &&
                is_playing_mode(prev_mode.try_into().unwrap()) &&
                is_playing_mode(new_mode.try_into().unwrap())) 
            { prev_cycle_nr + 1 } else { prev_cycle_nr };

        rust.mode = new_mode;
        rust.length = new_length;
        rust.position = new_position;
        rust.next_mode = new_next_mode;
        rust.next_transition_delay = new_next_transition_delay;
        rust.display_peaks = QList::from(new_display_peaks);
        rust.display_midi_notes_active = new_display_midi_notes_active;
        rust.display_midi_events_triggered = new_display_midi_events_triggered;
        rust.cycle_nr = new_cycle_nr;

        if prev_mode != new_mode {
            debug!(self, "mode: {} -> {}", prev_mode, new_mode);
            self.as_mut().mode_changed();
        }
        if prev_length != new_length {
            debug!(self, "length: {} -> {}", prev_length, new_length);
            self.as_mut().length_changed();
        }
        if prev_position != new_position {
            debug!(self, "position: {} -> {}", prev_position, new_position);
            self.as_mut().position_changed();
        }
        if prev_next_mode != new_next_mode {
            debug!(self, "next mode: {} -> {}", prev_next_mode, new_next_mode);
            self.as_mut().next_mode_changed();
        }
        if prev_next_delay != new_next_transition_delay {
            debug!(self, "next delay: {} -> {}", prev_next_delay, new_next_transition_delay);
            self.as_mut().next_transition_delay_changed();
        }
        if display_peaks_changed {
            trace!(self, "display peaks changed");
            self.as_mut().display_peaks_changed();
        }
        if prev_display_midi_notes_active != new_display_midi_notes_active {
            trace!(self, "midi notes active: {} -> {}", prev_display_midi_notes_active, new_display_midi_notes_active);
            self.as_mut().display_midi_notes_active_changed();
        }
        if prev_display_midi_events_triggered != new_display_midi_events_triggered {
            trace!(self, "midi events triggered: {} -> {}", prev_display_midi_events_triggered, new_display_midi_events_triggered);
            self.as_mut().display_midi_events_triggered_changed();
        }
        if prev_cycle_nr != new_cycle_nr {
            debug!(self, "cycle nr: {} -> {}", prev_cycle_nr, new_cycle_nr);
            self.as_mut().cycle_nr_changed();
            self.as_mut().cycled(new_cycle_nr);
        }
    }

    pub fn update_on_gui_thread(self: Pin<&mut Loop>) {
        // Stub implementation
    }

    pub fn maybe_initialize_backend(mut self: Pin<&mut Loop>) {
        let initialize_condition : bool;

        unsafe {
            initialize_condition =
               !self.initialized() &&
                self.backend != std::ptr::null_mut() &&
                qobject_property_bool(self.backend.as_ref().unwrap(), "ready".to_string()).unwrap_or(false) &&
                self.backend_loop.is_none();
        }

        if initialize_condition {
            debug!(self, "Found backend, initializing");
            let backend_qobj = self.backend();
            unsafe {
                let backend_ptr = qobject_ptr_to_backend_ptr(*backend_qobj);
                if backend_ptr.is_null() {
                    error!(self, "Failed to convert backend QObject to backend pointer");
                } else {
                    // FIXME: unwraps
                    let backend_session = backend_ptr.as_mut()
                                                  .unwrap()
                                                  .session
                                                  .as_ref()
                                                  .unwrap();
                    let backend_loop = backend_session.create_loop().unwrap();
                    let mut rust_mut = self.as_mut().rust_mut();
                    rust_mut.backend_loop = Some(Arc::new(Mutex::new(backend_loop)));

                    // Connect signals
                    cxx_qt_lib_shoop::connect::connect
                       (backend_ptr.as_mut().unwrap(),
                        "updated_on_backend_thread()".to_string(),
                        self.as_mut().get_unchecked_mut(),
                        "update_on_non_gui_thread()".to_string(),
                        cxx_qt_lib_shoop::connection_types::DIRECT_CONNECTION);
                    cxx_qt_lib_shoop::connect::connect
                        (backend_ptr.as_mut().unwrap(),
                         "updated_on_gui_thread()".to_string(),
                         self.as_mut().get_unchecked_mut(),
                         "update_on_gui_thread()".to_string(),
                         cxx_qt_lib_shoop::connection_types::DIRECT_CONNECTION);

                    self.set_initialized(true);
                }
            }
        } else {
            debug!(self, "Not initializing as not all conditions are met");
        }
    }

    pub fn get_children_with_object_name(self: Pin<&mut Loop>, find_object_name: &str) -> QList<QVariant> {
        let mut result = QList_QVariant::default();
        unsafe {
            let ref_self = self.as_ref();
            let qquickitem = ref_self.qquickitem_ref();
            for child in qquickitem.child_items().iter()
                            .filter(|child| {
                                let object_name = 
                                qobject_object_name
                                   (qvariant_to_qobject_ptr(child)
                                     .unwrap()
                                     .as_ref()
                                     .unwrap()).unwrap();
                                object_name == find_object_name
                            }) { result.append(child.clone()); }
        }
        result
    }

    pub fn get_audio_channels(self: Pin<&mut Loop>) -> QList<QVariant> {
        self.get_children_with_object_name("LoopAudioChannel")
    }

    pub fn get_midi_channels(self: Pin<&mut Loop>) -> QList<QVariant> {
        self.get_children_with_object_name("LoopMidiChannel")
    }

    pub fn transition_multiple(self: Pin<&mut Loop>,
        loops: QList_QVariant,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32)
    {
        debug!(self, "Transitioning {} loops to {} with delay {}, sync at cycle {}",
           loops.len(), to_mode, maybe_cycles_delay, maybe_to_sync_at_cycle);
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            let mut backend_loop_arcs : Vec<Arc<Mutex<backend_bindings::Loop>>> = Vec::new();
            let mut backend_loop_guards : Vec<MutexGuard<backend_bindings::Loop>> = Vec::new();
            let mut backend_loop_refs : Vec<&backend_bindings::Loop> = Vec::new();
            backend_loop_arcs.reserve(loops.len() as usize);
            backend_loop_guards.reserve(loops.len() as usize);
            backend_loop_refs.reserve(loops.len() as usize);

            // Increment the reference count for all loops involved
            loops.iter().map(|loop_variant| -> Result<(), anyhow::Error> {
                unsafe {
                    let loop_qobj : *mut QObject =
                        qvariant_to_qobject_ptr(loop_variant)
                        .ok_or(anyhow::anyhow!("Failed to convert QVariant to QObject pointer"))?;
                    let loop_ptr : *mut Loop =
                        qobject_to_loop_ptr(loop_qobj);
                    let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
                        loop_ptr.as_ref()
                                .unwrap()
                                .backend_loop
                                .as_ref()
                                .ok_or(anyhow::anyhow!("Backend loop not set"))?
                                .clone();
                    backend_loop_arcs.push(backend_loop_arc);
                    Ok(())
                }
            }).for_each(|result| {
                match result {
                    Ok(_) => (),
                    Err(err) => {
                        error!(self, "Failed to increment reference count for loop: {:?}", err);
                    }
                }
            });

            // Lock all backend loops
            backend_loop_arcs.iter().for_each(|backend_loop_arc| {
                let backend_loop_guard = backend_loop_arc.lock().unwrap();
                backend_loop_guards.push(backend_loop_guard);
            });

            // Get references to all backend loops
            backend_loop_guards.iter().for_each(|backend_loop_guard| {
                backend_loop_refs.push(backend_loop_guard.deref());
            });

            backend_bindings::transition_multiple_loops(
                &backend_loop_refs,
                to_mode.try_into()?,
                maybe_cycles_delay,
                maybe_to_sync_at_cycle)
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to transition multiple loops: {:?}", err);
            }
        }
    }

    pub fn transition(self: Pin<&mut Loop>,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32)
    {
        debug!(self, "Transitioning to {} with delay {}, sync at cycle {}",
           to_mode, maybe_cycles_delay, maybe_to_sync_at_cycle);
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
                self.as_ref()
                    .backend_loop
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Backend loop not set"))?
                    .clone();
            backend_loop_arc.lock().unwrap().transition
               (to_mode.try_into()?,
                maybe_cycles_delay,
                maybe_to_sync_at_cycle)?;
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to transition loop: {:?}", err);
            }
        }
    }

    pub fn add_audio_channel(self: Pin<&mut Loop>, mode: i32) -> Result<AudioChannel, anyhow::Error> {
        let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
                self.as_ref()
                    .backend_loop
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Backend loop not set"))?
                    .clone();
        let channel = backend_loop_arc.lock()
                        .unwrap()
                        .add_audio_channel(mode.try_into()?)?;
        Ok(channel)
    }

    pub fn add_midi_channel(self: Pin<&mut Loop>, mode: i32) -> Result<MidiChannel, anyhow::Error> {
        let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
                self.as_ref()
                    .backend_loop
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Backend loop not set"))?
                    .clone();
        let channel = backend_loop_arc.lock()
                        .unwrap()
                        .add_midi_channel(mode.try_into()?)?;
        Ok(channel)
    }

    pub fn clear(mut self: Pin<&mut Loop>, length : i32) {
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
                self.as_ref()
                    .backend_loop
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Backend loop not set"))?
                    .clone();
            backend_loop_arc.lock().unwrap().clear(length as u32)?;

            let audio_chans = self.as_mut().get_audio_channels();
            let midi_chans = self.as_mut().get_midi_channels();
            let all_channels_iter = audio_chans.iter().chain(midi_chans.iter());
            for child in all_channels_iter {
                unsafe {
                    let channel_ptr = qvariant_to_qobject_ptr(child).unwrap();
                    invoke::<_,(),_>(channel_ptr.as_mut().unwrap(), "clear()".to_string(), connection_types::DIRECT_CONNECTION, &())?;
                }
            }

            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to clear loop: {:?}", err);
            }
        }
    }

    pub fn adopt_ringbuffers(mut self: Pin<&mut Loop>,
        maybe_reverse_start_cycle : QVariant,
        maybe_cycles_length : QVariant,
        maybe_go_to_cycle : QVariant,
        go_to_mode : i32)
    {
        debug!(self, "Adopting ringbuffers");
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
                self.as_ref()
                    .backend_loop
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Backend loop not set"))?
                    .clone();
            backend_loop_arc.lock().unwrap().adopt_ringbuffer_contents
               (maybe_reverse_start_cycle.value::<i32>(),
                maybe_cycles_length.value::<i32>(),
                maybe_go_to_cycle.value::<i32>(),
                go_to_mode.try_into()?)?;
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to adopt ringbuffers: {:?}", err);
            }
        }
    }

    pub fn update_backend_sync_source(self: Pin<&mut Loop>) {
        if !self.initialized() {
            debug!(self, "update_backend_sync_source: not initialized");
            return;
        }
        if self.rust().sync_source.is_null() {
            debug!(self, "update_backend_sync_source: sync source is null");
            return;
        }
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
                self.as_ref()
                    .backend_loop
                    .as_ref()
                    .ok_or(anyhow::anyhow!("Backend loop not set"))?
                    .clone();
            let sync_source_backend_loop;
            unsafe {
                let sync_source_as_loop = qobject_to_loop_ptr(self.sync_source);
                if sync_source_as_loop.is_null() {
                    return Err(anyhow::anyhow!("Failed to cast sync source QObject to Loop"));
                }
                let maybe_sync_source_backend_loop = sync_source_as_loop
                                              .as_ref()
                                              .unwrap()
                                              .backend_loop
                                              .as_ref();
                if maybe_sync_source_backend_loop.is_none() {
                    debug!(self, "update_backend_sync_source: sync source back-end loop not set, deferring");
                    let sync_source_loop_ref = sync_source_as_loop.as_ref().unwrap();
                    connect(sync_source_loop_ref,
                            "initializedChanged()".to_string(), self.as_ref().get_ref(),
                            "update_backend_sync_source()".to_string(), connection_types::QUEUED_CONNECTION | connection_types::SINGLE_SHOT_CONNECTION)?;
                    return Ok(());
                }
                sync_source_backend_loop = maybe_sync_source_backend_loop
                                              .unwrap()
                                              .clone();
            }
            let locked = sync_source_backend_loop.lock().unwrap();
            debug!(self, "Setting back-end sync source");
            backend_loop_arc.lock().unwrap().set_sync_source
               (Some(locked.deref()))?;
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to update backend sync source: {:?}", err);
            }
        }
    }

    pub unsafe fn set_sync_source(mut self: Pin<&mut Loop>, sync_source: *mut QObject) {
        debug!(self, "sync source -> {:?}", sync_source);

        if !sync_source.is_null() {
            let loop_ptr = qobject_to_loop_ptr(sync_source);
            if loop_ptr.is_null() {
                error!(self, "Failed to cast sync source QObject to Loop");
                return;
            }
        }

        if *self.initialized() {
            self.as_mut().update_backend_sync_source();
        } else {
            debug!(self, "Defer updating back-end sync source: loop not initialized");
            let loop_ref = self.as_ref().get_ref();
            connect(loop_ref,
                    "initializedChanged()".to_string(), loop_ref,
                    "update_backend_sync_source()".to_string(), connection_types::QUEUED_CONNECTION | connection_types::SINGLE_SHOT_CONNECTION)
               .unwrap();
        }

        let changed = self.as_mut().rust_mut().sync_source != sync_source;
        self.as_mut().rust_mut().sync_source = sync_source;

        if changed {
            self.as_mut().sync_source_changed();
        }
    }

    pub fn dependent_will_handle_sync_loop_cycle(self: Pin<&mut Loop>, _cycle_nr: i32) {
        ()
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let obj = make_unique_loop();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_loop(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}
