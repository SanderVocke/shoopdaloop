#include "MidiMergingBuffer.h"
#include <vector>
#include <algorithm>
#include <array>
#include <iostream>

namespace {
struct {
    bool operator()(const MidiSortableMessageInterface *a,
                    const MidiSortableMessageInterface *b) const {
        return a->get_time() < b->get_time();
    }
} compare;
} // namespace

MidiMergingBuffer::MidiMergingBuffer() {
    references.reserve(buffer_size);
    stored_messages.reserve(stored_messages_size);
}

size_t MidiMergingBuffer::PROC_get_n_events() const {
    return references.size();
}

MidiSortableMessageInterface const &
MidiMergingBuffer::PROC_get_event_reference(size_t idx) {
    if (dirty) {
        throw std::runtime_error("Access in merging buffer which is unsorted");
    }
    return *references[idx];
}

void MidiMergingBuffer::PROC_sort() {
    if (dirty) {
        std::stable_sort(references.begin(), references.end(), compare);
        dirty = false;
    }
}

void MidiMergingBuffer::PROC_clear() {
    references.clear();
    stored_messages.clear();
    dirty = false;
}

bool MidiMergingBuffer::write_by_value_supported() const { return true; }
bool MidiMergingBuffer::write_by_reference_supported() const { return true; }

void MidiMergingBuffer::PROC_write_event_value(uint32_t size, uint32_t time,
                                               const uint8_t *data) {
    if (size > 3) {
        throw std::runtime_error(
            "Midi merging buffer: message value dropped because size > 3");
    }
    if (stored_messages.size() >= stored_messages.capacity()) {
        std::cerr << "Warning: expanded MIDI buffer on processing thread\n";
        stored_messages.reserve(stored_messages.size() * 2);
    }
    stored_messages.push_back(Message(time, size, data));
    auto ptr = (MidiSortableMessageInterface *)&stored_messages.back();
    PROC_write_event_reference(*ptr);
}

void MidiMergingBuffer::PROC_write_event_reference(
    MidiSortableMessageInterface const &m) {
    references.push_back(&m);
    dirty = true;
}