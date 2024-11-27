use anyhow;
use crate::ffi;
use std::sync::Mutex;

pub struct MidiChannel {
    obj : Mutex<*mut ffi::shoopdaloop_loop_midi_channel_t>,
    pub fn available(&self) -> bool {
        !self.unsafe_backend_ptr().is_null()
    }

    pub fn get_all_midi_data(&self) -> Vec<MidiEvent> {
        unsafe {
            let data_ptr = ffi::get_midi_channel_data(self.unsafe_backend_ptr());
            if data_ptr.is_null() {
                return Vec::new();
            }
            let events = std::slice::from_raw_parts((*data_ptr).events, (*data_ptr).n_events as usize);
            let result: Vec<MidiEvent> = events.iter().map(|event| MidiEvent::from(*event)).collect();
            ffi::destroy_midi_sequence(data_ptr);
            result
        }
    }

    pub fn load_all_midi_data(&self, msgs: &[MidiEvent]) {
        unsafe {
            let sequence = ffi::alloc_midi_sequence(msgs.len() as u32);
            if sequence.is_null() {
                return;
            }
            for (i, msg) in msgs.iter().enumerate() {
                (*sequence).events[i] = msg.to_backend();
            }
            ffi::load_midi_channel_data(self.unsafe_backend_ptr(), sequence);
            ffi::destroy_midi_sequence(sequence);
        }
    }

    pub fn connect_input(&self, port: &MidiPort) {
        unsafe {
            ffi::connect_midi_input(self.unsafe_backend_ptr(), port.unsafe_backend_ptr());
        }
    }

    pub fn connect_output(&self, port: &MidiPort) {
        unsafe {
            ffi::connect_midi_output(self.unsafe_backend_ptr(), port.unsafe_backend_ptr());
        }
    }

    pub fn disconnect(&self, port: &MidiPort) {
        unsafe {
            ffi::disconnect_midi_port(self.unsafe_backend_ptr(), port.unsafe_backend_ptr());
        }
    }

    pub fn get_state(&self) -> Result<MidiChannelState, anyhow::Error> {
        unsafe {
            let state_ptr = ffi::get_midi_channel_state(self.unsafe_backend_ptr());
            if state_ptr.is_null() {
                return Err(anyhow::anyhow!("Failed to retrieve MIDI channel state"));
            }
            let state = MidiChannelState::new(&(*state_ptr));
            ffi::destroy_midi_channel_state_info(state_ptr);
            Ok(state)
        }
    }

    pub fn set_mode(&self, mode: ChannelMode) {
        unsafe {
            ffi::set_midi_channel_mode(self.unsafe_backend_ptr(), mode as u32);
        }
    }

    pub fn set_start_offset(&self, offset: i32) {
        unsafe {
            ffi::set_midi_channel_start_offset(self.unsafe_backend_ptr(), offset);
        }
    }

    pub fn set_n_preplay_samples(&self, n: u32) {
        unsafe {
            ffi::set_midi_channel_n_preplay_samples(self.unsafe_backend_ptr(), n);
        }
    }

    pub fn clear_data_dirty(&self) {
        unsafe {
            ffi::clear_midi_channel_data_dirty(self.unsafe_backend_ptr());
        }
    }

    pub fn clear(&self) {
        unsafe {
            ffi::clear_midi_channel(self.unsafe_backend_ptr());
        }
    }

    pub fn reset_state_tracking(&self) {
        unsafe {
            ffi::reset_midi_channel_state_tracking(self.unsafe_backend_ptr());
        }
    }
}

unsafe impl Send for MidiChannel {}
unsafe impl Sync for MidiChannel {}

impl MidiChannel {
    pub fn new(raw : *mut ffi::shoopdaloop_loop_midi_channel_t) -> Result<Self, anyhow::Error> {
        if raw.is_null() {
            Err(anyhow::anyhow!("Cannot create MidiChannel from null pointer"))
        } else {
            let wrapped = Mutex::new(raw);
            Ok(MidiChannel { obj: wrapped })
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_loop_midi_channel_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
}

impl Drop for MidiChannel {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_midi_channel(obj) };
    }
}
