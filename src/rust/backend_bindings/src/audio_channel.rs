use crate::ffi;
use anyhow;
use std::sync::Mutex;

use crate::audio_port::AudioPort;
use crate::channel::ChannelMode;

pub struct AudioChannelState {
    pub mode: ChannelMode,
    pub gain: f32,
    pub output_peak: f32,
    pub length: u32,
    pub start_offset: i32,
    pub played_back_sample: Option<u32>,
    pub n_preplay_samples: u32,
    pub data_dirty: bool,
}

impl AudioChannelState {
    pub fn new(obj: &ffi::shoop_audio_channel_state_info_t) -> Result<Self, anyhow::Error> {
        return Ok(AudioChannelState {
            mode: ChannelMode::try_from(obj.mode as i32)?,
            gain: obj.gain,
            output_peak: obj.output_peak,
            length: obj.length,
            start_offset: obj.start_offset,
            played_back_sample: match obj.played_back_sample >= 0 {
                true => Some(obj.played_back_sample as u32),
                false => None,
            },
            n_preplay_samples: obj.n_preplay_samples,
            data_dirty: obj.data_dirty != 0,
        });
    }
}

pub struct AudioChannel {
    obj: Mutex<*mut ffi::shoopdaloop_loop_audio_channel_t>,
}

unsafe impl Send for AudioChannel {}
unsafe impl Sync for AudioChannel {}

impl AudioChannel {
    pub fn new(raw: *mut ffi::shoopdaloop_loop_audio_channel_t) -> Result<Self, anyhow::Error> {
        if raw.is_null() {
            Err(anyhow::anyhow!(
                "Cannot create AudioChannel from null pointer"
            ))
        } else {
            let wrapped = Mutex::new(raw);
            Ok(AudioChannel { obj: wrapped })
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_loop_audio_channel_t {
        let guard = self.obj.lock();
        guard.map(|g| *g).unwrap_or(std::ptr::null_mut())
    }

    pub fn connect_input(&self, port: &AudioPort) {
        unsafe {
            ffi::connect_audio_input(self.unsafe_backend_ptr(), port.unsafe_backend_ptr());
        }
    }

    pub fn connect_output(&self, port: &AudioPort) {
        unsafe {
            ffi::connect_audio_output(self.unsafe_backend_ptr(), port.unsafe_backend_ptr());
        }
    }

    pub fn disconnect(&self, port: &AudioPort) {
        unsafe {
            ffi::disconnect_audio_port(self.unsafe_backend_ptr(), port.unsafe_backend_ptr());
        }
    }

    pub fn load_data(&self, data: &[f32]) {
        unsafe {
            let backend_data = ffi::alloc_audio_channel_data(data.len() as u32);
            if backend_data.is_null() {
                return;
            }
            for (i, &value) in data.iter().enumerate() {
                (*backend_data)
                    .data
                    .offset(i.try_into().unwrap_or(0))
                    .write(value);
            }
            ffi::load_audio_channel_data(self.unsafe_backend_ptr(), backend_data);
            ffi::destroy_audio_channel_data(backend_data);
        }
    }

    pub fn get_data(&self) -> Vec<f32> {
        unsafe {
            let data_ptr = ffi::get_audio_channel_data(self.unsafe_backend_ptr());
            if data_ptr.is_null() {
                return Vec::new();
            }
            let data = std::slice::from_raw_parts((*data_ptr).data, (*data_ptr).n_samples as usize);
            let result = data.to_vec();
            ffi::destroy_audio_channel_data(data_ptr);
            result
        }
    }

    pub fn get_state(&self) -> Result<AudioChannelState, anyhow::Error> {
        unsafe {
            let state_ptr = ffi::get_audio_channel_state(self.unsafe_backend_ptr());
            if state_ptr.is_null() {
                return Err(anyhow::anyhow!("Failed to retrieve audio channel state"));
            }
            let state = AudioChannelState::new(&(*state_ptr))?;
            ffi::destroy_audio_channel_state_info(state_ptr);
            Ok(state)
        }
    }

    pub fn set_gain(&self, gain: f32) {
        unsafe {
            ffi::set_audio_channel_gain(self.unsafe_backend_ptr(), gain);
        }
    }

    pub fn set_mode(&self, mode: ChannelMode) {
        unsafe {
            ffi::set_audio_channel_mode(
                self.unsafe_backend_ptr(),
                mode as ffi::shoop_channel_mode_t,
            );
        }
    }

    pub fn set_start_offset(&self, offset: i32) {
        unsafe {
            ffi::set_audio_channel_start_offset(self.unsafe_backend_ptr(), offset);
        }
    }

    pub fn set_n_preplay_samples(&self, n: u32) {
        unsafe {
            ffi::set_audio_channel_n_preplay_samples(self.unsafe_backend_ptr(), n);
        }
    }

    pub fn clear_data_dirty(&self) {
        unsafe {
            ffi::clear_audio_channel_data_dirty(self.unsafe_backend_ptr());
        }
    }

    pub fn clear(&self, length: u32) {
        unsafe {
            ffi::clear_audio_channel(self.unsafe_backend_ptr(), length);
        }
    }
}

impl Drop for AudioChannel {
    fn drop(&mut self) {
        if let Ok(guard) = self.obj.lock() {
            let obj = *guard;
            if obj.is_null() {
                return;
            }
            unsafe { ffi::destroy_audio_channel(obj) };
        }
    }
}
