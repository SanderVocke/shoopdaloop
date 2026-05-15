#include "MidiSortingBuffer.h"
#include <vector>
#include <algorithm>
#include <iostream>

MidiSortingBuffer::MidiSortingBuffer() {
    messages.reserve(buffer_size);
}

uint32_t MidiSortingBuffer::n_events() const {
    return messages.size();
}

MidiStorageElem MidiSortingBuffer::get_event(uint32_t idx) const {
    if (dirty) {
        throw std::runtime_error("Access in merging buffer which is unsorted");
    }
    return messages[idx];
}

void MidiSortingBuffer::PROC_sort() {
    if (dirty) {
        std::stable_sort(messages.begin(), messages.end(), [](const MidiStorageElem &a, const MidiStorageElem &b) {
            return a.time < b.time;
        });
        dirty = false;
    }
}

void MidiSortingBuffer::PROC_process(uint32_t nframes) {
    PROC_sort();
}

void MidiSortingBuffer::PROC_prepare(uint32_t nframes) {
    PROC_clear();
}

void MidiSortingBuffer::PROC_clear() {
    messages.clear();
    dirty = false;
}

void MidiSortingBuffer::write_event(MidiStorageElem event) {
    if (event.size > 4) {
        throw std::runtime_error(
            "Midi merging buffer: message dropped because size > 4");
    }
    if (messages.size() >= messages.capacity()) {
        std::cerr << "Warning: expanded MIDI buffer on processing thread\n";
        messages.reserve(messages.size() * 2);
    }
    messages.push_back(event);
    dirty = true;
}