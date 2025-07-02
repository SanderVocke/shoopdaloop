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
use crate::cxx_qt_lib_shoop::qvariant_qobject::qvariant_to_qobject_ptr;
use crate::cxx_qt_shoop::qobj_backend_wrapper::qobject_ptr_to_backend_ptr;
use crate::cxx_qt_shoop::qobj_loop_backend_bridge::LoopBackend;                                   
use crate::cxx_qt_shoop::qobj_loop_backend_bridge::ffi::*;  
use crate::loop_mode_helpers::*;   
use cxx_qt_lib::{QList, QVariant, QString};                      
use std::ops::Deref;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
shoop_log_unit!("Frontend.Loop");

macro_rules! trace {
    ($self:ident, $($arg:tt)*) => {
        raw_trace!("[{}-backend] {}", $self.rust().instance_identifier, format!($($arg)*));
    };
}

macro_rules! debug {
    ($self:ident, $($arg:tt)*) => {
        raw_debug!("[{}-backend] {}", $self.rust().instance_identifier, format!($($arg)*));
    };
}

macro_rules! error {
    ($self:ident, $($arg:tt)*) => {
        raw_error!("[{}-backend] {}", $self.rust().instance_identifier, format!($($arg)*));
    };
}

fn convert_maybe_mode_i32(value: Option<backend_bindings::LoopMode>) -> i32 {
    match value {
        Some(v) => v as i32,
        None => backend_bindings::LoopMode::Unknown as i32,
    }
}

impl LoopBackend {
    // pub unsafe fn initialize_impl(mut self: Pin<&mut LoopBackend>) {
    //     unsafe {
    //         let backend_ptr = qobject_ptr_to_backend_ptr(backend_obj);
    //         if backend_ptr.is_null() {
    //             error!(self, "Failed to convert backend QObject to backend pointer");
    //         } else {
    //             // Connect signals
    //             cxx_qt_lib_shoop::connect::connect_or_report
    //                 (backend_ptr.as_mut().unwrap(),
    //                 "updated_on_backend_thread()".to_string(),
    //                 self.as_mut().get_unchecked_mut(),
    //                 "update()".to_string(),
    //                 cxx_qt_lib_shoop::connection_types::DIRECT_CONNECTION);
    //         }
    //     }
    // }

    pub fn set_length(mut self: Pin<&mut LoopBackend>, length: i32) {
        if ! self.as_mut().maybe_initialize_backend() {
            error!(self, "set_length: not initialized");
            return;
        }
        debug!(self, "set length -> {}", length);
        let mut rust = self.as_mut().rust_mut();
        rust.backend_loop.as_mut().unwrap().set_length(length as u32).unwrap();
    }

    pub fn set_position(mut self: Pin<&mut LoopBackend>, position: i32) {
        if ! self.as_mut().maybe_initialize_backend() {
            error!(self, "set_position: not initialized");
            return;
        }
        debug!(self, "set position -> {}", position);
        let mut rust = self.as_mut().rust_mut();
        rust.backend_loop.as_mut().unwrap().set_position(position as u32).unwrap();
    }

    pub fn set_backend_indirect(mut self: Pin<&mut LoopBackend>, backend: *mut QObject) {
        self.set_backend(backend);
    }

    pub fn maybe_initialize_backend(mut self: Pin<&mut LoopBackend>) -> bool {
        let initialize_condition : bool;

        if self.get_initialized() { return true; }

        unsafe {
            initialize_condition =
               !self.get_initialized() &&
                self.backend != std::ptr::null_mut() &&
                qobject_property_bool(self.backend.as_ref().unwrap(), "ready".to_string()).unwrap_or(false) &&
                self.as_ref().backend_loop.is_none();
        }

        if initialize_condition {
            debug!(self, "Initializing");
            unsafe {
                let backend_ptr = qobject_ptr_to_backend_ptr(self.rust().backend);
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
                    rust_mut.backend_loop = Some(backend_loop);

                    let initialized = self.get_initialized();
                    self.initialized_changed(initialized);
                    return initialized
                }
            }
        } else {
            debug!(self, "Not initializing as not all conditions are met");
        }
        return false;
    }

    pub fn update(mut self: Pin<&mut LoopBackend>) {
        if self.rust().backend_loop.is_none() {
            return;
        }

        self.as_mut().starting_update();
        let mut rust = self.as_mut().rust_mut();
        let new_state = rust.backend_loop.as_mut().unwrap().get_state().unwrap();

        // let audio_chans : Vec<*mut QObject> = self.as_mut().get_audio_channels()
        //                                .iter()
        //                                .map(|v| qvariant_to_qobject_ptr(v).unwrap())
        //                                .collect();
        // let midi_chans : Vec<*mut QObject> = self.as_mut().get_midi_channels()
        //                               .iter()
        //                               .map(|v| qvariant_to_qobject_ptr(v).unwrap())
        //                               .collect();

        let prev_state;
        let prev_cycle_nr : i32;
        let new_cycle_nr : i32;
        {
            let mut rust = self.as_mut().rust_mut();
            
            prev_state = rust.prev_state.clone();
            // let prev_length = rust.length;
            // let prev_next_mode = rust.next_mode;
            // let prev_next_delay = rust.next_transition_delay;
            // let prev_display_peaks : Vec<f32> = rust.display_peaks.iter().map(|f| f.clone()).collect();
            // let prev_display_midi_notes_active = rust.display_midi_notes_active.clone();
            // let prev_display_midi_events_triggered = rust.display_midi_events_triggered.clone();
            prev_cycle_nr = rust.prev_cycle_nr;

            // let new_display_peaks : Vec<f32> =
            //    audio_chans.iter()
            //    .filter(|qobj| {
            //         unsafe {
            //             let mode = qobject_property_int(qobj.as_ref().unwrap(), "mode".to_string()).unwrap();
            //             mode == backend_bindings::ChannelMode::Direct as i32 ||
            //                mode == backend_bindings::ChannelMode::Wet as i32
            //         }
            //      })
            //       .map(|qobj| {
            //          unsafe {
            //               let peak = qobject_property_float(qobj.as_ref().unwrap(), "output_peak".to_string()).unwrap();
            //               peak as f32
            //          }
            //         }).collect();
            // let display_peaks_changed = prev_display_peaks != new_display_peaks;
            // let new_display_midi_notes_active : i32 = midi_chans.iter().map(|qobj| -> i32 {
            //     unsafe {
            //         let n_notes_active = qobject_property_int(qobj.as_ref().unwrap(), "n_notes_active".to_string()).unwrap();
            //         n_notes_active
            //     }
            // }).sum();
            // let new_display_midi_events_triggered : i32 = midi_chans.iter().map(|qobj| -> i32 {
            //     unsafe {
            //         let n_events_triggered = qobject_property_int(qobj.as_ref().unwrap(), "n_events_triggered".to_string()).unwrap();
            //         n_events_triggered
            //     }
            // }).sum();
            new_cycle_nr = 
                if (new_state.position < prev_state.position &&
                    is_playing_mode(prev_state.mode.try_into().unwrap()) &&
                    is_playing_mode(new_state.mode.try_into().unwrap())) 
                { rust.prev_cycle_nr + 1 } else { rust.prev_cycle_nr };

            rust.prev_state = new_state.clone();
            // rust.length = new_length;
            // rust.next_mode = new_next_mode;
            // rust.next_transition_delay = new_next_transition_delay;
            // rust.display_peaks = QList::from(new_display_peaks);
            // rust.display_midi_notes_active = new_display_midi_notes_active;
            // rust.display_midi_events_triggered = new_display_midi_events_triggered;
            rust.prev_cycle_nr = new_cycle_nr;
        }

        self.as_mut().state_changed(new_state.mode as i32,
                           new_state.length as i32,
                           new_state.position as i32,
                           convert_maybe_mode_i32(new_state.maybe_next_mode),
                           new_state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32,
                           new_cycle_nr);

        if prev_state.mode != new_state.mode {
            debug!(self, "mode: {:?} -> {:?}", prev_state.mode, new_state.mode);
            self.as_mut().mode_changed(prev_state.mode as i32, new_state.mode as i32);
        }
        if prev_state.length != new_state.length {
            debug!(self, "length: {} -> {}", prev_state.length, new_state.length);
            self.as_mut().length_changed(prev_state.length as i32, new_state.length as i32);
        }
        if prev_state.position != new_state.position {
            debug!(self, "position: {} -> {}", prev_state.position, new_state.position);
            self.as_mut().position_changed(prev_state.position as i32, new_state.position as i32);
        }
        if prev_state.maybe_next_mode != new_state.maybe_next_mode {
            debug!(self, "next mode: {:?} -> {:?}", prev_state.maybe_next_mode, new_state.maybe_next_mode);
            let prev_mode = convert_maybe_mode_i32(prev_state.maybe_next_mode);
            let new_mode = convert_maybe_mode_i32(new_state.maybe_next_mode);
            self.as_mut().next_mode_changed(prev_mode, new_mode);
        }
        if prev_state.maybe_next_mode_delay != new_state.maybe_next_mode_delay {
            debug!(self, "next delay: {:?} -> {:?}", prev_state.maybe_next_mode_delay, new_state.maybe_next_mode_delay);
            let prev_delay : i32 = prev_state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32;
            let new_delay : i32 = new_state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32;
            self.as_mut().next_transition_delay_changed(prev_delay, new_delay);
        }
        // if display_peaks_changed {
        //     trace!(self, "display peaks changed");
        //     self.as_mut().display_peaks_changed();
        // }
        // if prev_display_midi_notes_active != new_display_midi_notes_active {
        //     trace!(self, "midi notes active: {} -> {}", prev_display_midi_notes_active, new_display_midi_notes_active);
        //     self.as_mut().display_midi_notes_active_changed();
        // }
        // if prev_display_midi_events_triggered != new_display_midi_events_triggered {
        //     trace!(self, "midi events triggered: {} -> {}", prev_display_midi_events_triggered, new_display_midi_events_triggered);
        //     self.as_mut().display_midi_events_triggered_changed();
        // }
        if prev_cycle_nr != new_cycle_nr {
            debug!(self, "cycle nr: {} -> {}", prev_cycle_nr, new_cycle_nr);
            self.as_mut().cycle_nr_changed(new_cycle_nr, prev_cycle_nr);
        }
    }

    // pub fn get_audio_channels(self: Pin<&mut LoopBackend>) -> QList<QVariant> {
    //     self.get_children_with_object_name("LoopAudioChannel")
    // }

    // pub fn get_midi_channels(self: Pin<&mut LoopBackend>) -> QList<QVariant> {
    //     self.get_children_with_object_name("LoopMidiChannel")
    // }

    pub fn transition_multiple(self: Pin<&mut LoopBackend>,
        loops: QList_QVariant,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32)
    {
        LoopBackend::transition_multiple_impl(loops, to_mode, maybe_cycles_delay, maybe_to_sync_at_cycle);
    }

    pub fn transition_multiple_impl(
        loops: QList_QVariant,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32)
    {
        raw_debug!("Transitioning {} loops to {} with delay {}, sync at cycle {}",
           loops.len(), to_mode, maybe_cycles_delay, maybe_to_sync_at_cycle);
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            let mut backend_loop_refs : Vec<&backend_bindings::Loop> = Vec::new();
            backend_loop_refs.reserve(loops.len() as usize);

            // Increment the reference count for all loops involved
            loops.iter().map(|loop_variant| -> Result<(), anyhow::Error> {
                unsafe {
                    let loop_qobj : *mut QObject =
                        qvariant_to_qobject_ptr(loop_variant)
                        .ok_or(anyhow::anyhow!("Failed to convert QVariant to QObject pointer"))?;
                    let loop_ptr : *mut LoopBackend =
                        qobject_to_loop_backend_ptr(loop_qobj);
                    {
                        let loop_pin = std::pin::Pin::new_unchecked(&mut *loop_ptr);
                        loop_pin.maybe_initialize_backend();
                    }
                    let backend_loop_ref : &backend_bindings::Loop =
                        loop_ptr.as_ref()
                                .unwrap()
                                .backend_loop
                                .as_ref()
                                .ok_or(anyhow::anyhow!("Backend loop not set"))?;
                    backend_loop_refs.push(backend_loop_ref);
                    Ok(())
                }
            }).for_each(|result| {
                match result {
                    Ok(_) => (),
                    Err(err) => {
                        raw_error!("Failed to get backend loop loop: {:?}", err);
                    }
                }
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
                raw_error!("Failed to transition multiple loops: {:?}", err);
            }
        }
    }

    pub fn transition(mut self: Pin<&mut LoopBackend>,
        to_mode: i32,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32)
    {
        if ! self.as_mut().maybe_initialize_backend() {
            error!(self, "transition: not initialized");
            return;
        }
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            if self.as_ref().backend_loop.is_some() {
                self.as_ref().backend_loop.as_ref().unwrap().transition
                    (to_mode.try_into()?,
                        maybe_cycles_delay,
                        maybe_to_sync_at_cycle)?;
                debug!(self, "Transitioning to {} with delay {}, sync at cycle {}",
                            to_mode, maybe_cycles_delay, maybe_to_sync_at_cycle);
            } else {
                error!(self, "Not transitioning: back-end loop not initialized");
            }
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to transition loop: {:?}", err);
            }
        }
    }

    pub fn add_audio_channel(self: Pin<&mut LoopBackend>, mode: i32) -> Result<AudioChannel, anyhow::Error> {
        // let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
        //         self.as_ref()
        //             .backend_loop
        //             .as_ref()
        //             .ok_or(anyhow::anyhow!("Backend loop not set"))?
        //             .clone();
        // let channel = backend_loop_arc.lock()
        //                 .unwrap()
        //                 .add_audio_channel(mode.try_into()?)?;
        //Ok(channel)
        Err(anyhow::anyhow!("Not implemented"))
    }

    pub fn add_midi_channel(self: Pin<&mut LoopBackend>, mode: i32) -> Result<MidiChannel, anyhow::Error> {
        // let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
        //         self.as_ref()
        //             .backend_loop
        //             .as_ref()
        //             .ok_or(anyhow::anyhow!("Backend loop not set"))?
        //             .clone();
        // let channel = backend_loop_arc.lock()
        //                 .unwrap()
        //                 .add_midi_channel(mode.try_into()?)?;
        //Ok(channel)
        Err(anyhow::anyhow!("Not implemented"))
    }

    pub fn clear(mut self: Pin<&mut LoopBackend>, length : i32) {
        if ! self.as_mut().maybe_initialize_backend() {
            error!(self, "clear: not initialized");
            return;
        }
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            // let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
            //     self.as_ref()
            //         .backend_loop
            //         .as_ref()
            //         .ok_or(anyhow::anyhow!("Backend loop not set"))?
            //         .clone();
            // backend_loop_arc.lock().unwrap().clear(length as u32)?;

            // let audio_chans = self.as_mut().get_audio_channels();
            // let midi_chans = self.as_mut().get_midi_channels();
            // let all_channels_iter = audio_chans.iter().chain(midi_chans.iter());
            // for child in all_channels_iter {
            //     unsafe {
            //         let channel_ptr = qvariant_to_qobject_ptr(child).unwrap();
            //         invoke::<_,(),_>(channel_ptr.as_mut().unwrap(), "clear()".to_string(), connection_types::DIRECT_CONNECTION, &())?;
            //     }
            // }

            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to clear loop: {:?}", err);
            }
        }
    }

    pub fn adopt_ringbuffers(mut self: Pin<&mut LoopBackend>,
        maybe_reverse_start_cycle : QVariant,
        maybe_cycles_length : QVariant,
        maybe_go_to_cycle : QVariant,
        go_to_mode : i32)
    {
        if ! self.as_mut().maybe_initialize_backend() {
            error!(self, "adopt_ringbuffers: not initialized");
            return;
        }
        debug!(self, "Adopting ringbuffers");
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            // let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
            //     self.as_ref()
            //         .backend_loop
            //         .as_ref()
            //         .ok_or(anyhow::anyhow!("Backend loop not set"))?
            //         .clone();
            // backend_loop_arc.lock().unwrap().adopt_ringbuffer_contents
            //    (maybe_reverse_start_cycle.value::<i32>(),
            //     maybe_cycles_length.value::<i32>(),
            //     maybe_go_to_cycle.value::<i32>(),
            //     go_to_mode.try_into()?)?;
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to adopt ringbuffers: {:?}", err);
            }
        }
    }

    pub unsafe fn set_sync_source(mut self: Pin<&mut LoopBackend>, sync_source: *mut QObject) {
        if ! self.as_mut().maybe_initialize_backend() {
            error!(self, "set_sync_source: not initialized");
            return;
        }

        debug!(self, "sync source -> {:?}", sync_source);

        if !sync_source.is_null() {
            let loop_ptr = qobject_to_loop_backend_ptr(sync_source);
            if loop_ptr.is_null() {
                error!(self, "Failed to cast sync source QObject to Loop");
                return;
            }
        }

        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            // let backend_loop_arc : Arc<Mutex<backend_bindings::Loop>> =
            //     self.as_ref()
            //         .backend_loop
            //         .as_ref()
            //         .ok_or(anyhow::anyhow!("Backend loop not set"))?
            //         .clone();
            // let sync_source_backend_loop;
            // unsafe {
            //     let sync_source_as_loop = qobject_to_loop_backend_ptr(self.sync_source);
            //     if sync_source_as_loop.is_null() {
            //         return Err(anyhow::anyhow!("Failed to cast sync source QObject to Loop"));
            //     }
            //     let maybe_sync_source_backend_loop = sync_source_as_loop
            //                                   .as_ref()
            //                                   .unwrap()
            //                                   .backend_loop
            //                                   .as_ref();
            //     if maybe_sync_source_backend_loop.is_none() {
            //         debug!(self, "update_backend_sync_source: sync source back-end loop not set, deferring");
            //         let sync_source_loop_ref = sync_source_as_loop.as_ref().unwrap();
            //         connect(sync_source_loop_ref,
            //                 "initializedChanged()".to_string(), self.as_ref().get_ref(),
            //                 "update_backend_sync_source()".to_string(), connection_types::QUEUED_CONNECTION | connection_types::SINGLE_SHOT_CONNECTION)?;
            //         return Ok(());
            //     }
            //     sync_source_backend_loop = maybe_sync_source_backend_loop
            //                                   .unwrap()
            //                                   .clone();
            // }
            // let locked = sync_source_backend_loop.lock().unwrap();
            // debug!(self, "Setting back-end sync source");
            // backend_loop_arc.lock().unwrap().set_sync_source
            //    (Some(locked.deref()))?;
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to update backend sync source: {:?}", err);
            }
        }

        let changed = self.as_mut().rust_mut().sync_source != sync_source;
        self.as_mut().rust_mut().sync_source = sync_source;

        if changed {
            self.as_mut().sync_source_changed();
        }
    }

    pub fn get_mode(self: &LoopBackend) -> i32 {
        self.rust().prev_state.mode as i32
    }

    pub fn get_length(self: &LoopBackend) -> i32 {
        self.rust().prev_state.length as i32
    }

    pub fn get_position(self: &LoopBackend) -> i32 {
        self.rust().prev_state.position as i32
    }

    pub fn get_next_mode(self: &LoopBackend) -> i32 {
        convert_maybe_mode_i32(self.rust().prev_state.maybe_next_mode)
    }

    pub fn get_next_transition_delay(self: &LoopBackend) -> i32 {
        self.rust().prev_state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32
    }

    pub fn get_cycle_nr(self: &LoopBackend) -> i32 {
        self.rust().prev_cycle_nr
    }

    pub fn get_initialized(self: &LoopBackend) -> bool {
        self.rust().backend_loop.is_some()
    }
}