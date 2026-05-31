#pragma once

#include "MidiBuffer.h"
#include "MidiStorageElem.h"
#include "backend_rust/src/midi_sorting_buffer_cxx.rs.h"
#include <array>
#include <memory>
#include <stdexcept>

// RustMidiSortingBuffer - A wrapper that uses Rust for MIDI message sorting.
// Provides the same interface as the original MidiSortingBuffer but with Rust implementation.

class RustMidiSortingBuffer : public MidiReadableBuffer, public MidiWriteableBuffer {
public:
    RustMidiSortingBuffer() : m_rust(backend_rust::new_midi_sorting_buffer()) {}

    uint32_t n_events() const override {
        return backend_rust::sorting_buffer_n_events(*m_rust);
    }

    MidiStorageElem get_event(uint32_t idx) const override {
        // Note: This can panic in Rust if buffer is dirty
        uint32_t time;
        uint16_t size;
        std::array<uint8_t, 4> bytes;
        backend_rust::sorting_buffer_get_event(*m_rust, idx, time, size, bytes);
        MidiStorageElem elem;
        elem.time = time;
        elem.size = size;
        memcpy(elem.bytes, bytes.data(), 4);
        return elem;
    }

    void write_event(MidiStorageElem event) override {
        if (!backend_rust::sorting_buffer_write_event(*m_rust, event.time, event.size, event.bytes)) {
            throw std::runtime_error(
                "Midi merging buffer: message dropped because size > 4");
        }
    }

    // Sorting and clearing methods
    void PROC_sort() {
        backend_rust::sorting_buffer_sort(*m_rust);
    }

    void PROC_clear() {
        backend_rust::sorting_buffer_clear(*m_rust);
    }

    void PROC_process(uint32_t nframes) { (void)nframes; PROC_sort(); }

    void PROC_prepare(uint32_t nframes) { (void)nframes; PROC_clear(); }

private:
    rust::Box<backend_rust::MidiSortingBuffer> m_rust;
};