use crate::ffi;
use anyhow;
use std::sync::Mutex;

use crate::channel::ChannelMode;
use crate::midi::MidiEvent;
use crate::midi_port::MidiPort;

pub struct MidiChannelState {
    pub mode: ChannelMode,
    pub n_events_triggered: u32,
    pub n_notes_active: u32,
    pub length: u32,
    pub start_offset: i32,
    pub played_back_sample: Option<u32>,
    pub n_preplay_samples: u32,
    pub data_dirty: bool,
}

impl MidiChannelState {
    pub fn new(obj: &ffi::shoop_midi_channel_state_info_t) -> Result<Self, anyhow::Error> {
        Ok(MidiChannelState {
            mode: ChannelMode::try_from(obj.mode as i32)?,
            n_events_triggered: obj.n_events_triggered,
            n_notes_active: obj.n_notes_active,
            length: obj.length,
            start_offset: obj.start_offset,
            played_back_sample: match obj.played_back_sample >= 0 {
                true => Some(obj.played_back_sample as u32),
                false => None,
            },
            n_preplay_samples: obj.n_preplay_samples,
            data_dirty: obj.data_dirty != 0,
        })
    }
}

pub struct MidiChannel {
    obj: Mutex<*mut ffi::shoopdaloop_loop_midi_channel_t>,
}

unsafe impl Send for MidiChannel {}
unsafe impl Sync for MidiChannel {}

impl MidiChannel {
    pub fn new(raw: *mut ffi::shoopdaloop_loop_midi_channel_t) -> Result<Self, anyhow::Error> {
        if raw.is_null() {
            Err(anyhow::anyhow!(
                "Cannot create MidiChannel from null pointer"
            ))
        } else {
            let wrapped = Mutex::new(raw);
            Ok(MidiChannel { obj: wrapped })
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_loop_midi_channel_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }

    pub fn get_all_midi_data(&self) -> Vec<MidiEvent> {
        unsafe {
            let data_ptr = ffi::get_midi_channel_data(self.unsafe_backend_ptr());
            if data_ptr.is_null() {
                return Vec::new();
            }
            let events =
                std::slice::from_raw_parts((*data_ptr).events, (*data_ptr).n_events as usize);
            let result: Vec<MidiEvent> = events
                .iter()
                .map(|event| MidiEvent::new(&**event))
                .collect();
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
            let events_ptr = (*sequence).events;
            for i in 0..msgs.len() {
                let event_ptr = events_ptr.wrapping_add(i);
                let event = msgs[i].to_ffi();
                let new_event_ptr = ffi::alloc_midi_event(event.size);
                std::ptr::copy_nonoverlapping(
                    event.data,
                    (*new_event_ptr).data,
                    event.size as usize,
                );
                (*new_event_ptr).time = event.time;
                *event_ptr = new_event_ptr;
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
            let state = MidiChannelState::new(&(*state_ptr))?;
            ffi::destroy_midi_channel_state_info(state_ptr);
            Ok(state)
        }
    }

    pub fn set_mode(&self, mode: ChannelMode) {
        unsafe {
            ffi::set_midi_channel_mode(
                self.unsafe_backend_ptr(),
                mode as ffi::shoop_channel_mode_t,
            );
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
