//! CXX bridge for MIDI sorting buffer to expose to C++.

use crate::midi_sorting_buffer::MidiSortingBuffer;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiSortingBuffer;

        fn new_midi_sorting_buffer() -> Box<MidiSortingBuffer>;

        fn sorting_buffer_n_events(buf: &MidiSortingBuffer) -> u32;

        // Get event at index - writes to output parameters
        fn sorting_buffer_get_event(
            buf: &MidiSortingBuffer,
            idx: u32,
            out_time: &mut u32,
            out_size: &mut u16,
            out_bytes: &mut [u8; 4],
        );

        fn sorting_buffer_sort(buf: &mut MidiSortingBuffer);
        fn sorting_buffer_clear(buf: &mut MidiSortingBuffer);

        // Write event - raw parameters
        unsafe fn sorting_buffer_write_event(
            buf: &mut MidiSortingBuffer,
            time: u32,
            size: u16,
            data: *const u8,
        ) -> bool;
    }
}

/// Create a new MidiSortingBuffer
pub fn new_midi_sorting_buffer() -> Box<MidiSortingBuffer> {
    Box::new(MidiSortingBuffer::new())
}

/// Get the number of events
fn sorting_buffer_n_events(buf: &MidiSortingBuffer) -> u32 {
    buf.n_events()
}

/// Get an event by index - writes to output parameters
fn sorting_buffer_get_event(
    buf: &MidiSortingBuffer,
    idx: u32,
    out_time: &mut u32,
    out_size: &mut u16,
    out_bytes: &mut [u8; 4],
) {
    let elem = buf.get_event(idx);
    *out_time = elem.time;
    *out_size = elem.size;
    *out_bytes = elem.bytes;
}

/// Sort the buffer
fn sorting_buffer_sort(buf: &mut MidiSortingBuffer) {
    buf.sort();
}

/// Clear the buffer
fn sorting_buffer_clear(buf: &mut MidiSortingBuffer) {
    buf.clear();
}

/// Write an event with raw parameters
unsafe fn sorting_buffer_write_event(
    buf: &mut MidiSortingBuffer,
    time: u32,
    size: u16,
    data: *const u8,
) -> bool {
    let slice = std::slice::from_raw_parts(data, size as usize);
    if let Some(elem) = crate::midi_storage::MidiStorageElem::new(time, size, slice) {
        buf.write_event(elem)
    } else {
        false
    }
}
