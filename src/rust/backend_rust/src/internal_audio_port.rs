//! InternalAudioPort - Audio port with local buffer management.
//!
//! Ported from C++ InternalAudioPort.h/cpp
//!
//! The Rust core owns the local audio buffer and metadata.
//! Audio processing (gain/mute/peaks/ringbuffer) is handled by the
//! C++ RustAudioPortF32 base class via the Rust AudioPort.

pub struct InternalAudioPort {
    buffer: Vec<f32>,
    name: String,
    input_connectability: u32,
    output_connectability: u32,
}

impl InternalAudioPort {
    pub fn new(
        name: &str,
        n_frames: u32,
        input_connectability: u32,
        output_connectability: u32,
    ) -> Self {
        InternalAudioPort {
            buffer: vec![0.0f32; n_frames as usize],
            name: name.to_string(),
            input_connectability,
            output_connectability,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn proc_prepare(&mut self, nframes: u32) {
        let nframes = nframes as usize;
        if self.buffer.len() < nframes {
            self.buffer.resize(nframes, 0.0f32);
        }
        for sample in &mut self.buffer[..nframes] {
            *sample = 0.0f32;
        }
    }

    pub fn proc_get_buffer(&mut self, nframes: u32) -> *mut f32 {
        let nframes = nframes as usize;
        if self.buffer.len() < nframes {
            self.buffer.resize(nframes, 0.0f32);
        }
        self.buffer.as_mut_ptr()
    }

    pub fn input_connectability(&self) -> u32 {
        self.input_connectability
    }

    pub fn output_connectability(&self) -> u32 {
        self.output_connectability
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let port = InternalAudioPort::new("test", 16, 1, 2);
        assert_eq!(port.name(), "test");
        assert_eq!(port.input_connectability(), 1);
        assert_eq!(port.output_connectability(), 2);
    }

    #[test]
    fn test_prepare_zero_fills() {
        let mut port = InternalAudioPort::new("test", 4, 0, 0);
        // Fill buffer with non-zero values
        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts_mut(buf, 4);
            slice.copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        }
        port.proc_prepare(4);
        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts(buf, 4);
            assert_eq!(slice, &[0.0, 0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn test_buffer_pointer_non_null_after_prepare() {
        let mut port = InternalAudioPort::new("test", 8, 0, 0);
        port.proc_prepare(8);
        let buf = port.proc_get_buffer(8);
        assert!(!buf.is_null());
    }

    #[test]
    fn test_name_round_trip() {
        let port = InternalAudioPort::new("my_port", 10, 0, 0);
        assert_eq!(port.name(), "my_port");
    }
}
