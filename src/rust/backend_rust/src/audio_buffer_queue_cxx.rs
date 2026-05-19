//! CXX bridge for AudioBufferQueue to expose to C++.

#![allow(dead_code)]

use crate::audio_buffer_queue::{AudioBufferQueue, AudioBufferQueueSnapshot};

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        // Snapshot type
        type AudioBufferQueueSnapshot;

        // Snapshot accessors
        fn snapshot_f32_n_samples(snap: &AudioBufferQueueSnapshot) -> u32;
        fn snapshot_f32_buffer_size(snap: &AudioBufferQueueSnapshot) -> u32;
        fn snapshot_f32_n_buffers(snap: &AudioBufferQueueSnapshot) -> usize;

        // Get buffer data as slice - returns number of elements copied
        // out_data: pointer to output buffer
        // max_len: maximum elements to copy
        // Returns: actual number of elements copied
        unsafe fn snapshot_f32_get_buffer(
            snap: &AudioBufferQueueSnapshot,
            buffer_idx: usize,
            out_data: *mut f32,
            max_len: usize,
        ) -> usize;

        // Main BufferQueue type
        type AudioBufferQueue;

        // Constructor - takes pool configuration
        // pool_capacity: number of pre-allocated buffers
        // low_water_mark: refill when below this count
        // buffer_size: size of each buffer in elements (f32)
        // max_buffers: max buffers in queue
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
        // data: pointer to sample data
        // length: number of samples
        unsafe fn buffer_queue_f32_put(
            queue: &mut AudioBufferQueue,
            data: *const f32,
            length: u32,
        );

        // Configuration
        fn buffer_queue_f32_set_max_buffers(queue: &mut AudioBufferQueue, max: u32);
        fn buffer_queue_f32_set_min_n_samples(queue: &mut AudioBufferQueue, n: u32);

        // Snapshot
        fn buffer_queue_f32_snapshot(queue: &AudioBufferQueue) -> Box<AudioBufferQueueSnapshot>;

        // Clear
        fn buffer_queue_f32_clear(queue: &mut AudioBufferQueue);
    }
}

// Snapshot implementation

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

fn buffer_queue_f32_snapshot(queue: &AudioBufferQueue) -> Box<AudioBufferQueueSnapshot> {
    Box::new(queue.snapshot())
}

// Snapshot accessors

fn snapshot_f32_n_samples(snap: &AudioBufferQueueSnapshot) -> u32 {
    snap.n_samples
}

fn snapshot_f32_buffer_size(snap: &AudioBufferQueueSnapshot) -> u32 {
    snap.buffer_size
}

fn snapshot_f32_n_buffers(snap: &AudioBufferQueueSnapshot) -> usize {
    snap.buffers.len()
}

unsafe fn snapshot_f32_get_buffer(
    snap: &AudioBufferQueueSnapshot,
    buffer_idx: usize,
    out_data: *mut f32,
    max_len: usize,
) -> usize {
    if buffer_idx >= snap.buffers.len() {
        return 0;
    }
    let buf = &snap.buffers[buffer_idx];
    let to_copy = buf.len().min(max_len);
    std::ptr::copy_nonoverlapping(buf.as_ptr(), out_data, to_copy);
    to_copy
}

fn buffer_queue_f32_clear(queue: &mut AudioBufferQueue) {
    queue.clear();
}