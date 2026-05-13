use crate::midi_state_diff_tracker::MidiStateDiffTracker;
use crate::midi_state_tracker::MidiStateTracker;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiStateDiffTracker;
        fn new_midi_state_diff_tracker() -> Box<MidiStateDiffTracker>;
        unsafe fn do_reset_with_ptrs(
            self: &mut MidiStateDiffTracker,
            a: *mut u8,
            b: *mut u8,
            action: i32,
        );
        fn add_diff(self: &mut MidiStateDiffTracker, d0: u8, d1: u8);
        fn delete_diff(self: &mut MidiStateDiffTracker, d0: u8, d1: u8);
        fn check_note(self: &mut MidiStateDiffTracker, channel: u8, note: u8);
        fn check_cc(self: &mut MidiStateDiffTracker, channel: u8, controller: u8);
        fn check_program(self: &mut MidiStateDiffTracker, channel: u8);
        fn check_channel_pressure(self: &mut MidiStateDiffTracker, channel: u8);
        fn check_pitch_wheel(self: &mut MidiStateDiffTracker, channel: u8);
        fn rescan_diff(self: &mut MidiStateDiffTracker);
        fn clear_diff(self: &mut MidiStateDiffTracker);
        unsafe fn do_resolve_to_wrapper(
            self: &mut MidiStateDiffTracker,
            to: *mut u8,
            notes: bool,
            controls: bool,
            programs: bool,
        ) -> Vec<u8>;
        fn get_diff_flat(self: &MidiStateDiffTracker) -> Vec<u8>;
        fn set_diff_flat(self: &mut MidiStateDiffTracker, data: &[u8]);
    }
}

fn new_midi_state_diff_tracker() -> Box<MidiStateDiffTracker> {
    crate::midi_state_diff_tracker::new_midi_state_diff_tracker()
}

unsafe fn do_reset_with_ptrs(this: &mut MidiStateDiffTracker, a: *mut u8, b: *mut u8, action: i32) {
    this.reset_with_ptrs(
        a as *mut MidiStateTracker,
        b as *mut MidiStateTracker,
        action,
    )
}

fn add_diff(this: &mut MidiStateDiffTracker, d0: u8, d1: u8) {
    this.add_diff(d0, d1)
}

fn delete_diff(this: &mut MidiStateDiffTracker, d0: u8, d1: u8) {
    this.delete_diff(d0, d1)
}

fn check_note(this: &mut MidiStateDiffTracker, channel: u8, note: u8) {
    this.check_note(channel, note)
}

fn check_cc(this: &mut MidiStateDiffTracker, channel: u8, controller: u8) {
    this.check_cc(channel, controller)
}

fn check_program(this: &mut MidiStateDiffTracker, channel: u8) {
    this.check_program(channel)
}

fn check_channel_pressure(this: &mut MidiStateDiffTracker, channel: u8) {
    this.check_channel_pressure(channel)
}

fn check_pitch_wheel(this: &mut MidiStateDiffTracker, channel: u8) {
    this.check_pitch_wheel(channel)
}

fn rescan_diff(this: &mut MidiStateDiffTracker) {
    this.rescan_diff()
}

fn clear_diff(this: &mut MidiStateDiffTracker) {
    this.clear_diff()
}

unsafe fn do_resolve_to_wrapper(
    this: &mut MidiStateDiffTracker,
    to: *mut u8,
    notes: bool,
    controls: bool,
    programs: bool,
) -> Vec<u8> {
    this.resolve_to_wrapper(
        &mut *(to as *mut MidiStateTracker),
        notes,
        controls,
        programs,
    )
}

fn get_diff_flat(this: &MidiStateDiffTracker) -> Vec<u8> {
    this.get_diff_flat()
}

fn set_diff_flat(this: &mut MidiStateDiffTracker, data: &[u8]) {
    this.set_diff_flat(data)
}
