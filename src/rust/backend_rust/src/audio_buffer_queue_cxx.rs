//! CXX bridge for AudioBufferQueue to expose to C++.

use crate::audio_buffer_queue::AudioBufferQueue;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    // Buffer info struct - thin copy with just pointers
    struct BufferPtrInfo {
        data_ptr: *const f32,
        len: usize,
    }

    extern "Rust" {
        // Main BufferQueue type
        type AudioBufferQueue;

        // Constructor
        fn new_audio_buffer_queue_f32(
            pool_capacity: usize,
            low_water_mark: usize,
            buffer_size: usize,
            max_buffers: u32,
        ) -> Box<AudioBufferQueue>;

        // State queries
        fn buffer_queue_f32_n_samples(queue: &AudioBufferQueue) -> u32;
        fn buffer_queue_f32_single_buffer_size(queue: &AudioBufferQueue) -> u32;
        fn buffer_queue_f32_get_max_buffers(queue: &AudioBufferQueue) -> u32;
        fn buffer_queue_f32_n_buffers(queue: &AudioBufferQueue) -> usize;

        // Mutations
        unsafe fn buffer_queue_f32_put(
            queue: &mut AudioBufferQueue,
            data: *const f32,
            length: u32,
        );

        // Configuration
        fn buffer_queue_f32_set_max_buffers(queue: &mut AudioBufferQueue, max: u32);
        fn buffer_queue_f32_set_min_n_samples(queue: &mut AudioBufferQueue, n: u32);

        // Snapshot - returns vector of buffer pointers (thin copies)
        fn buffer_queue_f32_snapshot(queue: &AudioBufferQueue) -> Vec<BufferPtrInfo>;

        // Clear
        fn buffer_queue_f32_clear(queue: &mut AudioBufferQueue);
    }
}

fn new_audio_buffer_queue_f32(
    pool_capacity: usize,
    low_water_mark: usize,
    buffer_size: usize,
    max_buffers: u32,
) -> Box<AudioBufferQueue> {
    Box::new(AudioBufferQueue::new(pool_capacity, low_water_mark, buffer_size, max_buffers))
}

fn buffer_queue_f32_n_samples(queue: &AudioBufferQueue) -> u32 {
    queue.n_samples()
}

fn buffer_queue_f32_single_buffer_size(queue: &AudioBufferQueue) -> u32 {
    queue.single_buffer_size()
}

fn buffer_queue_f32_get_max_buffers(queue: &AudioBufferQueue) -> u32 {
    queue.get_max_buffers()
}

fn buffer_queue_f32_n_buffers(queue: &AudioBufferQueue) -> usize {
    queue.n_buffers()
}

unsafe fn buffer_queue_f32_put(
    queue: &mut AudioBufferQueue,
    data: *const f32,
    length: u32,
) {
    let slice = std::slice::from_raw_parts(data, length as usize);
    queue.put(slice);
}

fn buffer_queue_f32_set_max_buffers(queue: &mut AudioBufferQueue, max: u32) {
    queue.set_max_buffers(max);
}

fn buffer_queue_f32_set_min_n_samples(queue: &mut AudioBufferQueue, n: u32) {
    queue.set_min_n_samples(n);
}

fn buffer_queue_f32_snapshot(queue: &AudioBufferQueue) -> Vec<ffi::BufferPtrInfo> {
    let mut result = Vec::new();
    
    let n_buffers = queue.n_buffers();
    let active_pos = queue.active_buffer_pos();
    
    for (i, buf) in queue.iter_buffers().enumerate() {
        let filled_len = if i == n_buffers.saturating_sub(1) && n_buffers > 0 {
            active_pos as usize
        } else {
            buf.len()
        };
        
        if filled_len > 0 {
            result.push(ffi::BufferPtrInfo {
                data_ptr: buf.data_ptr(),
                len: filled_len,
            });
        }
    }
    
    result
}

fn buffer_queue_f32_clear(queue: &mut AudioBufferQueue) {
    queue.clear();
}