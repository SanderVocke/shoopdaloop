use crate::cxx_qt_shoop::qobj_loop_bridge::Loop;                                   
use crate::cxx_qt_shoop::qobj_loop_bridge::ffi::*;                                           
use std::pin::Pin;

impl Loop {
    pub fn initialize_impl(self: Pin<&mut Loop>) {
        // Stub implementation
        println!("Initializing Loop");
    }

    pub fn update_on_non_gui_thread(self: Pin<&mut Loop>) {
        if !self.initialized() {
            return;
        }

        // let audio_chans = self.get_audio_channels_impl();
        // let midi_chans = self.get_midi_channels_impl();

        // for channel in &audio_chans {
        //     channel.update_on_non_gui_thread();
        // }
        // for channel in &midi_chans {
        //     channel.update_on_non_gui_thread();
        // }

        let prev_position = self.position();
        let prev_mode = self.mode();
        let prev_length = self.length();
        let prev_next_mode = self.next_mode();
        let prev_next_delay = self.next_transition_delay();
        let prev_display_peaks = self.display_peaks();
        let prev_display_midi_notes_active = self.display_midi_notes_active();
        let prev_display_midi_events_triggered = self.display_midi_events_triggered();

        let state = self.backend_loop.unwrap().get_state().unwrap();
        self.set_mode(state.mode as i32);
        self.set_length(state.length as i32);
        self.set_position(state.position as i32);
        self.set_next_mode(state.maybe_next_mode.unwrap_or(state.mode) as i32);
        self.set_next_transition_delay(state.maybe_next_mode_delay.unwrap_or(u32::MAX) as i32);
        // self.set_display_peaks(audio_chans.iter().map(|c| c.output_peak()).collect());
        // self.set_display_midi_notes_active(midi_chans.iter().map(|c| c.n_notes_active()).sum());
        // self.set_display_midi_events_triggered(midi_chans.iter().map(|c| c.n_events_triggered()).sum());

        if prev_mode != self.mode() {
            self.mode_changed_unsafe(*self.mode());
        }
        if prev_length != self.length() {
            self.length_changed_unsafe(*self.length());
        }
        if prev_position != self.position() {
            self.position_changed_unsafe(*self.position());
        }
        if prev_next_mode != self.next_mode() {
            self.next_mode_changed_unsafe(*self.next_mode());
        }
        if prev_next_delay != self.next_transition_delay() {
            self.next_transition_delay_changed_unsafe(*self.next_transition_delay());
        }
        if prev_display_peaks != self.display_peaks() {
            self.display_peaks_changed_unsafe(self.display_peaks());
        }
        if prev_display_midi_notes_active != self.display_midi_notes_active() {
            self.display_midi_notes_active_changed_unsafe(*self.display_midi_notes_active());
        }
        if prev_display_midi_events_triggered != self.display_midi_events_triggered() {
            self.display_midi_events_triggered_changed_unsafe(*self.display_midi_events_triggered());
        }

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
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let obj = make_unique_loop();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_loop(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}
