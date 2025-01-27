use common::logging::macros::*;
use cxx_qt::ConnectionType;
use cxx_qt::CxxQtType;
use crate::cxx_qt_lib_shoop::qobject::qobject_property_bool;
use crate::cxx_qt_shoop::qobj_loop_bridge::Loop;                                   
use crate::cxx_qt_shoop::qobj_loop_bridge::ffi::*;                                           
use std::pin::Pin;
shoop_log_unit!("Frontend.Loop");

impl Loop {
    pub fn initialize_impl(mut self: Pin<&mut Loop>) {
        debug!("Initializing");

        {
            self.as_mut().connect_mode_changed(|o| { o.mode_changed_queued(); }, ConnectionType::QueuedConnection);
            self.as_mut().connect_next_mode_changed(|o| { o.next_mode_changed_queued(); }, ConnectionType::QueuedConnection);
            self.as_mut().connect_length_changed(|o| { o.length_changed_queued(); }, ConnectionType::QueuedConnection);
            self.as_mut().connect_position_changed(|o| { o.position_changed_queued(); }, ConnectionType::QueuedConnection);
            self.as_mut().connect_next_transition_delay_changed(|o| { o.next_transition_delay_changed_queued(); }, ConnectionType::QueuedConnection);
            self.as_mut().connect_display_peaks_changed(|o| { o.display_peaks_changed_queued(); }, ConnectionType::QueuedConnection);
            self.as_mut().connect_display_midi_notes_active_changed(|o| { o.display_midi_notes_active_changed_queued(); }, ConnectionType::QueuedConnection);
            self.as_mut().connect_display_midi_events_triggered_changed(|o| { o.display_midi_events_triggered_changed_queued(); }, ConnectionType::QueuedConnection);
            self.as_mut().connect_cycled(|o, cycle_nr| { o.cycled_queued(cycle_nr); }, ConnectionType::QueuedConnection);
        }
    }

    pub fn queue_set_length(mut self: Pin<&mut Loop>, length: i32) {
        debug!("queue set length -> {}", length);
        let mut rust = self.as_mut().rust_mut();
        let loop_obj = rust.backend_loop.as_mut().expect("Backend loop not set");
        loop_obj.set_length(length as u32);
    }

    pub fn queue_set_position(mut self: Pin<&mut Loop>, position: i32) {
        debug!("queue set position -> {}", position);
        let mut rust = self.as_mut().rust_mut();
        let loop_obj = rust.backend_loop.as_mut().expect("Backend loop not set");
        loop_obj.set_position(position as u32);
    }

    pub fn update_on_non_gui_thread(mut self: Pin<&mut Loop>) {
        if !self.initialized() {
            return;
        }

        self.as_mut().starting_update_on_non_gui_thread();

        // FIXME: do this with signal/slot connection initiated from
        //        channels instead
        // let audio_chans = self.get_audio_channels_impl();
        // let midi_chans = self.get_midi_channels_impl();
        // for channel in &audio_chans {
        //     channel.update_on_non_gui_thread();
        // }
        // for channel in &midi_chans {
        //     channel.update_on_non_gui_thread();
        // }

        let loop_obj = self.backend_loop.as_ref().expect("Backend loop not set");
        let state = loop_obj.get_state().unwrap();

        let mut rust = self.as_mut().rust_mut();
        
        let prev_position = rust.position;
        let prev_mode = rust.mode;
        let prev_length = rust.length;
        let prev_next_mode = rust.next_mode;
        let prev_next_delay = rust.next_transition_delay;
        // let prev_display_peaks = self.display_peaks().clone();
        // let prev_display_midi_notes_active = *self.display_midi_notes_active();
        // let prev_display_midi_events_triggered = *self.display_midi_events_triggered();

        let new_mode = state.mode as i32;
        let new_next_mode = state.maybe_next_mode.unwrap_or(state.mode) as i32;
        let new_length = state.length as i32;
        let new_position = state.position as i32;
        let new_next_transition_delay = state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32;
        // self.set_display_peaks(audio_chans.iter().map(|c| c.output_peak()).collect());
        // self.set_display_midi_notes_active(midi_chans.iter().map(|c| c.n_notes_active()).sum());
        // self.set_display_midi_events_triggered(midi_chans.iter().map(|c| c.n_events_triggered()).sum());

        // let display_peaks = self.display_peaks().clone();
        // let display_midi_notes_active = *self.display_midi_notes_active();
        // let display_midi_events_triggered = *self.display_midi_events_triggered();

        rust.mode = new_mode;
        rust.length = new_length;
        rust.position = new_position;
        rust.next_mode = new_next_mode;
        rust.next_transition_delay = new_next_transition_delay;

        if prev_mode != new_mode {
            debug!("mode: {} -> {}", prev_mode, new_mode);
            self.as_mut().mode_changed();
        }
        if prev_length != new_length {
            debug!("length: {} -> {}", prev_length, new_length);
            self.as_mut().length_changed();
        }
        if prev_position != new_position {
            debug!("position: {} -> {}", prev_position, new_position);
            self.as_mut().position_changed();
        }
        if prev_next_mode != new_next_mode {
            debug!("next mode: {} -> {}", prev_next_mode, new_next_mode);
            self.as_mut().next_mode_changed();
        }
        if prev_next_delay != new_next_transition_delay {
            debug!("next delay: {} -> {}", prev_next_delay, new_next_transition_delay);
            self.as_mut().next_transition_delay_changed();
        }
        // if prev_display_peaks != display_peaks {
        //     self.as_mut().display_peaks_changed_unsafe(&display_peaks);
        // }
        // if prev_display_midi_notes_active != display_midi_notes_active {
        //     self.as_mut().display_midi_notes_active_changed_unsafe(display_midi_notes_active);
        // }
        // if prev_display_midi_events_triggered != display_midi_events_triggered {
        //     self.as_mut().display_midi_events_triggered_changed_unsafe(display_midi_events_triggered);
        // }

        // for transition in self.pending_transitions() {
        //     if transition.is_list() {
        //         self.transition_multiple_impl(transition);
        //     } else {
        //         self.transition_impl(transition);
        //     }
        // }
        // self.clear_pending_transitions();

        // if self.position() < prev_position && is_playing_mode(prev_mode) && is_playing_mode(self.mode()) {
        //     self.increment_cycle_nr();
        //     self.cycled_unsafe(self.cycle_nr());
        // }
    }

    pub fn update_on_gui_thread(self: Pin<&mut Loop>) {
        // Stub implementation
        println!("Updating Loop on GUI Thread");
    }

    fn maybe_initialize(self: Pin<&mut Loop>) {
        let initialize_condition : bool;

        unsafe {
            initialize_condition =
               !self.initialized() &&
                self.backend != std::ptr::null_mut() &&
                qobject_property_bool(self.backend.as_ref().unwrap(), "initialized".to_string()).unwrap_or(false) &&
                self.backend_loop.is_none();
        }

        if initialize_condition {
            debug!("Found backend, initializing");
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let obj = make_unique_loop();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_loop(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}
