use anyhow;
use crate::ffi;
use crate::integer_enum;
use std::sync::Mutex;

use crate::backend_session::BackendSession;
use crate::audio_channel::AudioChannel;
use crate::midi_channel::MidiChannel;
use crate::channel::ChannelMode;

integer_enum!{
    pub enum LoopMode {
        Unknown = ffi::shoop_loop_mode_t_LoopMode_Unknown,
        Stopped = ffi::shoop_loop_mode_t_LoopMode_Stopped,
        Playing = ffi::shoop_loop_mode_t_LoopMode_Playing,
        Recording = ffi::shoop_loop_mode_t_LoopMode_Recording,
        Replacing = ffi::shoop_loop_mode_t_LoopMode_Replacing,
        PlayingDryThroughWet = ffi::shoop_loop_mode_t_LoopMode_PlayingDryThroughWet,
        RecordingDryIntoWet = ffi::shoop_loop_mode_t_LoopMode_RecordingDryIntoWet,
    }
}

pub struct LoopState {
    pub mode : LoopMode,
    pub length : u32,
    pub position : u32,
    pub maybe_next_mode : Option<LoopMode>,
    pub maybe_next_mode_delay : Option<u32>,
}

impl LoopState {
    pub fn new(obj : &ffi::shoop_loop_state_info_t) -> Self {
        let has_next_mode = obj.maybe_next_mode == ffi::shoop_loop_mode_t_LOOP_MODE_INVALID;
        return LoopState {
            mode : LoopMode::try_from(obj.mode).unwrap(),
            length : obj.length,
            position : obj.position,
            maybe_next_mode : match has_next_mode {
                true => Some (LoopMode::try_from(obj.maybe_next_mode).unwrap()),
                false => None
            },
            maybe_next_mode_delay : match has_next_mode {
                true => Some(obj.maybe_next_mode_delay),
                false => None
            },
        };
    }
}

pub struct Loop {
    obj: Mutex<*mut ffi::shoopdaloop_loop_t>,
}

unsafe impl Send for Loop {}
unsafe impl Sync for Loop {}

impl Loop {
    pub fn new(backend_session: &BackendSession) -> Result<Self, anyhow::Error> {
        let obj = unsafe { ffi::create_loop(backend_session.unsafe_backend_ptr()) };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create loop"));
        }
        Ok(Loop {
            obj: Mutex::new(obj),
        })
    }

    pub fn add_audio_channel(&self, mode: ChannelMode) -> Result<AudioChannel, anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let channel = unsafe { ffi::add_audio_channel(obj, mode as u32) };
        AudioChannel::new(channel)
    }

    pub fn add_midi_channel(&self, mode: ChannelMode) -> Result<MidiChannel, anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let channel = unsafe { ffi::add_midi_channel(obj, mode as u32) };
        MidiChannel::new(channel)
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_loop_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }

    pub fn transition(
        &self,
        to_mode: LoopMode,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) -> Result<(), anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return Err(anyhow::anyhow!("Invalid backend object"));
        }
        unsafe {
            ffi::loop_transition(obj, to_mode as u32, maybe_cycles_delay, maybe_to_sync_at_cycle)
        };
        Ok(())
    }

    pub fn transition_multiple(
        loops: &[Loop],
        to_state: LoopMode,
        maybe_cycles_delay: i32,
        maybe_to_sync_at_cycle: i32,
    ) -> Result<(), anyhow::Error> {
        if loops.is_empty() {
            return Ok(());
        }
        let handles: Vec<*mut ffi::shoopdaloop_loop_t> = loops
            .iter()
            .map(|l| unsafe { l.unsafe_backend_ptr() })
            .collect();
        let handles_ptr: *mut *mut ffi::shoopdaloop_loop_t = handles.as_ptr() as *mut *mut ffi::shoopdaloop_loop_t;
        unsafe {
            ffi::loops_transition(
                handles.len() as u32,
                handles_ptr,
                to_state as u32,
                maybe_cycles_delay,
                maybe_to_sync_at_cycle,
            )
        };
        Ok(())
    }

    pub fn get_state(&self) -> Result<LoopState, anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to retrieve loop state"));
        }
        let state = unsafe { ffi::get_loop_state(obj) };
        if state.is_null() {
            return Err(anyhow::anyhow!("Failed to retrieve loop state"));
        }
        let rval = unsafe { LoopState::new(&(*state)) };
        unsafe { ffi::destroy_loop_state_info(state) };
        Ok(rval)
    }

    pub fn set_length(&self, length: u32) -> Result<(), anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return Err(anyhow::anyhow!("Invalid backend object"));
        }
        unsafe { ffi::set_loop_length(obj, length) };
        Ok(())
    }

    pub fn set_position(&self, position: u32) -> Result<(), anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return Err(anyhow::anyhow!("Invalid backend object"));
        }
        unsafe { ffi::set_loop_position(obj, position) };
        Ok(())
    }

    pub fn clear(&self, length: u32) -> Result<(), anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return Err(anyhow::anyhow!("Invalid backend object"));
        }
        unsafe { ffi::clear_loop(obj, length) };
        Ok(())
    }

    pub fn set_sync_source(&self, loop_ref: Option<&Loop>) -> Result<(), anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return Err(anyhow::anyhow!("Invalid backend object"));
        }
        let loop_ptr = match loop_ref {
            Some(loop_ref) => unsafe { loop_ref.unsafe_backend_ptr() },
            None => std::ptr::null_mut(),
        };
        unsafe { ffi::set_loop_sync_source(obj, loop_ptr) };
        Ok(())
    }

    pub fn adopt_ringbuffer_contents(
        &self,
        reverse_start_cycle: Option<i32>,
        cycles_length: Option<i32>,
        go_to_cycle: Option<i32>,
        go_to_mode: LoopMode,
    ) -> Result<(), anyhow::Error> {
        let reverse_start_cycle = reverse_start_cycle.unwrap_or(-1);
        let cycles_length = cycles_length.unwrap_or(-1);
        let go_to_cycle = go_to_cycle.unwrap_or(-1);

        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return Err(anyhow::anyhow!("Invalid backend object"));
        }
        unsafe {
            ffi::adopt_ringbuffer_contents(
                obj,
                reverse_start_cycle,
                cycles_length,
                go_to_cycle,
                go_to_mode as u32,
            )
        };
        Ok(())
    }
}

impl Drop for Loop {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_loop(obj) };
    }
}
