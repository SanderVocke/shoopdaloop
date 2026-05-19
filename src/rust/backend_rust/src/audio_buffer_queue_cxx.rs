//! CXX bridge for AudioBufferQueue to expose to C++.

#![allow(dead_code)]

use crate::audio_buffer_queue::{
    AudioBufferQueue, AudioBufferQueueF32, AudioBufferQueueSnapshot, AudioBufferQueueSnapshotF32,
};

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    // Snapshot for C++ to read buffer data
    struct AudioBufferQueueSnapshotData {
        n_samples: u32,
        buffer_size: u32,
        n_buffers: usize,
    }

    extern "Rust" {
        // Snapshot type for f32 samples
        type AudioBufferQueueSnapshotF32;

        // Snapshot accessors
        fn snapshot_f32_n_samples(snap: &AudioBufferQueueSnapshotF32) -> u32;
        fn snapshot_f32_buffer_size(snap: &AudioBufferQueueSnapshotF32) -> u32;
        fn snapshot_f32_n_buffers(snap: &AudioBufferQueueSnapshotF32) -> usize;

        // Get buffer data as slice - returns number of elements copied
        // out_data: pointer to output buffer
        // max_len: maximum elements to copy
        // Returns: actual number of elements copied
        unsafe fn snapshot_f32_get_buffer(
            snap: &AudioBufferQueueSnapshotF32,
            buffer_idx: usize,
            out_data: *mut f32,
            max_len: usize,
        ) -> usize;

        // Main BufferQueue type for f32 samples
        type AudioBufferQueueF32;

        // Constructor
        fn new_audio_buffer_queue_f32(max_buffers: u32, buffer_size: u32) -> Box<AudioBufferQueueF32>;

        // State queries
        fn buffer_queue_f32_n_samples(queue: &AudioBufferQueueF32) -> u32;
        fn buffer_queue_f32_single_buffer_size(queue: &AudioBufferQueueF32) -> u32;
        fn buffer_queue_f32_get_max_buffers(queue: &AudioBufferQueueF32) -> u32;
        fn buffer_queue_f32_n_buffers(queue: &AudioBufferQueueF32) -> usize;

        // Mutations
        // data: pointer to sample data
        // length: number of samples
        unsafe fn buffer_queue_f32_put(
            queue: &mut AudioBufferQueueF32,
            data: *const f32,
            length: u32,
        );

        // Configuration
        fn buffer_queue_f32_set_max_buffers(queue: &mut AudioBufferQueueF32, max: u32);
        fn buffer_queue_f32_set_min_n_samples(queue: &mut AudioBufferQueueF32, n: u32);

        // Snapshot
        fn buffer_queue_f32_snapshot(queue: &AudioBufferQueueF32) -> Box<AudioBufferQueueSnapshotF32>;

        // Clear
        fn buffer_queue_f32_clear(queue: &mut AudioBufferQueueF32);
    }
}

// Snapshot implementation for f32

fn new_audio_buffer_queue_f32(max_buffers: u32, buffer_size: u32) -> Box<AudioBufferQueueF32> {
    Box::new(AudioBufferQueueF32::new(max_buffers, buffer_size))
}

fn buffer_queue_f32_n_samples(queue: &AudioBufferQueueF32) -> u32 {
    queue.n_samples()
}

fn buffer_queue_f32_single_buffer_size(queue: &AudioBufferQueueF32) -> u32 {
    queue.single_buffer_size()
}

fn buffer_queue_f32_get_max_buffers(queue: &AudioBufferQueueF32) -> u32 {
    queue.get_max_buffers()
}

fn buffer_queue_f32_n_buffers(queue: &AudioBufferQueueF32) -> usize {
    queue.n_buffers()
}

unsafe fn buffer_queue_f32_put(
    queue: &mut AudioBufferQueueF32,
    data: *const f32,
    length: u32,
) {
    let slice = std::slice::from_raw_parts(data, length as usize);
    queue.put(slice);
}

fn buffer_queue_f32_set_max_buffers(queue: &mut AudioBufferQueueF32, max: u32) {
    queue.set_max_buffers(max);
}

fn buffer_queue_f32_set_min_n_samples(queue: &mut AudioBufferQueueF32, n: u32) {
    queue.set_min_n_samples(n);
}

fn buffer_queue_f32_snapshot(queue: &AudioBufferQueueF32) -> Box<AudioBufferQueueSnapshotF32> {
    Box::new(queue.snapshot())
}

// Snapshot accessors

fn snapshot_f32_n_samples(snap: &AudioBufferQueueSnapshotF32) -> u32 {
    snap.n_samples
}

fn snapshot_f32_buffer_size(snap: &AudioBufferQueueSnapshotF32) -> u32 {
    snap.buffer_size
}

fn snapshot_f32_n_buffers(snap: &AudioBufferQueueSnapshotF32) -> usize {
    snap.buffers.len()
}

unsafe fn snapshot_f32_get_buffer(
    snap: &AudioBufferQueueSnapshotF32,
    buffer_idx: usize,
    out_data: *mut f32,
    max_len: usize,
) -> usize {
    if buffer_idx >= snap.buffers.len() {
        return 0;
    }
    let buf = &snap.buffers[buffer_idx];
    let to_copy = buf.data.len().min(max_len);
    std::ptr::copy_nonoverlapping(buf.data.as_ptr(), out_data, to_copy);
    to_copy
}

fn buffer_queue_f32_clear(queue: &mut AudioBufferQueueF32) {
    queue.clear();
}