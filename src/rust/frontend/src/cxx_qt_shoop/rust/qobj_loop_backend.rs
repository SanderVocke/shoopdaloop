use backend_bindings::AudioChannel;
use backend_bindings::MidiChannel;
use common::logging::macros::{shoop_log_unit, debug as raw_debug, trace as raw_trace, error as raw_error};
use cxx_qt::CxxQtType;
use crate::cxx_qt_lib_shoop::connect::connect_or_report;
use crate::cxx_qt_lib_shoop::connection_types;
use crate::cxx_qt_lib_shoop::qobject::qobject_property_bool;
use crate::cxx_qt_lib_shoop::qvariant_qobject::qvariant_to_qobject_ptr;
use crate::cxx_qt_shoop::qobj_backend_wrapper::qobject_ptr_to_backend_ptr;
use crate::cxx_qt_shoop::qobj_loop_backend_bridge::LoopBackend;                                   
use crate::cxx_qt_shoop::qobj_loop_backend_bridge::ffi::*;  
use crate::loop_mode_helpers::*;   
use cxx_qt_lib::{QVariant, QString};                      
use std::pin::Pin;
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

fn convert_maybe_mode_i32(value: Option<backend_bindings::LoopMode>) -> i32 {
    match value {
        Some(v) => v as i32,
        None => backend_bindings::LoopMode::Unknown as i32,
    }
}

impl LoopBackend {
    pub fn initialize_impl(self: Pin<&mut LoopBackend>) {}

    pub fn set_length(mut self: Pin<&mut LoopBackend>, length: i32) {
        if ! self.as_mut().maybe_initialize_backend() {
            debug!(self, "set length -> {length} (deferred)");
            let mut rust = self.as_mut().rust_mut();
            rust.prev_state.length = length as u32;
            return;
        } else {
            debug!(self, "set length -> {}", length);
            let mut rust = self.as_mut().rust_mut();
            rust.backend_loop.as_mut().unwrap().set_length(length as u32).unwrap();
        }
    }

    pub fn set_position(mut self: Pin<&mut LoopBackend>, position: i32) {
        if ! self.as_mut().maybe_initialize_backend() {
            debug!(self, "set position -> {position} (deferred)");
            let mut rust = self.as_mut().rust_mut();
            rust.prev_state.position = position as u32;
            return;
        } else {
            debug!(self, "set position -> {}", position);
            let mut rust = self.as_mut().rust_mut();
            rust.backend_loop.as_mut().unwrap().set_position(position as u32).unwrap();
        }
    }

    pub fn set_backend(mut self: Pin<&mut LoopBackend>, backend: *mut QObject) {
        debug!(self, "set backend -> {:?}", backend);
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut.backend = backend;

        self.as_mut().maybe_initialize_backend();
        if ! self.get_initialized() && !backend.is_null() {
            unsafe {
                connect_or_report(
                    & *backend,
                    "readyChanged()".to_string(),
                    self.as_ref().get_ref(),
                    "maybe_initialize_backend()".to_string(),
                    connection_types::QUEUED_CONNECTION);
            }
        }

        unsafe { self.as_mut().backend_changed(backend); }
    }

    pub fn set_instance_identifier(mut self: Pin<&mut LoopBackend>, instance_identifier: QString) {
        let mut extended : QString = instance_identifier.clone();
        extended.append(&QString::from("-backend"));
        debug!(self, "set instance identifier -> {:?}", &extended);
        let mut rust_mut = self.as_mut().rust_mut();
        rust_mut.instance_identifier = extended.clone();
        self.as_mut().instance_identifier_changed(extended);
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
                }

                {
                    let sync_source = *self.sync_source();
                    let length = self.as_ref().prev_state.length;
                    let position = self.as_ref().prev_state.position;
                    self.as_mut().set_backend_sync_source(sync_source);
                    self.as_mut().set_length(length as i32);
                    self.as_mut().set_position(position as i32);
                }

                {
                    // Force getting of the initial state
                    self.as_mut().update();

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

        let result = || -> Result<(), anyhow::Error> {
            self.as_mut().starting_update();
            let mut rust = self.as_mut().rust_mut();
            let new_state = rust.backend_loop.as_mut()
                .ok_or(anyhow::anyhow!("backend loop object doesn't exist"))?
                .get_state()?;

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
                    if new_state.position < prev_state.position &&
                        is_playing_mode(prev_state.mode.try_into().unwrap()) &&
                        is_playing_mode(new_state.mode.try_into().unwrap()) 
                    { rust.prev_cycle_nr + 1 } else { rust.prev_cycle_nr };

                rust.prev_state = new_state.clone();
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
                trace!(self, "length: {} -> {}", prev_state.length, new_state.length);
                self.as_mut().length_changed(prev_state.length as i32, new_state.length as i32);
            }
            if prev_state.position != new_state.position {
                trace!(self, "position: {} -> {}", prev_state.position, new_state.position);
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
                if (new_cycle_nr - prev_cycle_nr) == 1 {
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
            self.as_ref().backend_loop.as_ref().unwrap().transition
                (to_mode.try_into()?,
                    maybe_cycles_delay,
                    maybe_to_sync_at_cycle)?;
            debug!(self, "Transitioning to {} with delay {}, sync at cycle {}",
                        to_mode, maybe_cycles_delay, maybe_to_sync_at_cycle);
            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to transition loop: {:?}", err);
            }
        }
    }

    pub fn add_audio_channel(self: Pin<&mut LoopBackend>, _mode: i32) -> Result<AudioChannel, anyhow::Error> {
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

    pub fn add_midi_channel(self: Pin<&mut LoopBackend>, _mode: i32) -> Result<MidiChannel, anyhow::Error> {
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
            debug!(self, "clearing to length {length}");
            self.as_ref().backend_loop.as_ref().unwrap().clear(length as u32)?;
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
            self.as_ref().backend_loop.as_ref().unwrap().adopt_ringbuffer_contents
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

    unsafe fn set_backend_sync_source(self: Pin<&mut LoopBackend>, sync_source: *mut QObject) {
        debug!(self, "set sync source -> {:?}", sync_source);
        let result : Result<(), anyhow::Error> = (|| -> Result<(), anyhow::Error> {
            if !sync_source.is_null() {
                let loop_ptr = qobject_to_loop_backend_ptr(sync_source);
                if loop_ptr.is_null() {
                    return Err(anyhow::anyhow!("Failed to cast sync source QObject to LoopBackend"));
                }
                self.as_ref().backend_loop.as_ref().unwrap().set_sync_source(loop_ptr.as_ref().unwrap().backend_loop.as_ref())?;
            } else {
                self.as_ref().backend_loop.as_ref().unwrap().set_sync_source(None)?;
            }

            Ok(())
        })();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!(self, "Failed to update backend sync source: {:?}", err);
            }
        }
    }

    pub unsafe fn set_sync_source(mut self: Pin<&mut LoopBackend>, sync_source: QVariant) {
        let maybe_sync_source_ptr = qvariant_to_qobject_ptr(&sync_source);
        let sync_source_ptr : *mut QObject =
           if maybe_sync_source_ptr.is_some() { maybe_sync_source_ptr.unwrap() }
           else { std::ptr::null_mut() };

        if ! self.as_mut().maybe_initialize_backend() {
            debug!(self, "set_sync_source -> {:?} (deferred)", sync_source_ptr);
        } else {
            self.as_mut().set_backend_sync_source(sync_source_ptr);
        }

        let old_source = self.as_mut().rust_mut().sync_source;
        let changed = old_source != sync_source_ptr;
        self.as_mut().rust_mut().sync_source = sync_source_ptr;

        if changed {
            self.as_mut().sync_source_changed(sync_source_ptr);
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