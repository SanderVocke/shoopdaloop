#include "MidiPortInterface.h"
#include "MidiMessage.h"
#include <vector>
#include <algorithm>
#include <array>
#include <iostream>

// The MIDI merging buffer can be used to store unordered MIDI messages,
// sort them efficiently and output them in order.
class MidiMergingBuffer : public MidiReadableBufferInterface,
                          public MidiWriteableBufferInterface {
    static constexpr size_t buffer_size = 1024;
    static constexpr size_t stored_messages_size = 256;

    using Message = MaxSizeMidiMessage<uint32_t, uint32_t, 3>;

    std::vector<const MidiSortableMessageInterface*> references;
    std::vector<Message> stored_messages;
    bool dirty;

    struct {
        bool operator()(const MidiSortableMessageInterface* a, const MidiSortableMessageInterface* b) const {
            return a->get_time() < b->get_time();
        }
    } compare;

public:
    MidiMergingBuffer() {
        references.reserve(buffer_size);
        stored_messages.reserve(stored_messages_size);
    }

    size_t PROC_get_n_events() const override {
        return references.size();
    }

    MidiSortableMessageInterface const & PROC_get_event_reference(size_t idx) override {
        if (dirty) {
            throw std::runtime_error("Access in merging buffer which is unsorted");
        }
        return *references[idx];
    }

    void PROC_sort() {
        if (dirty) {
            std::sort(references.begin(), references.end(), compare);
            dirty = false;
        }
    }

    void PROC_clear() {
        references.clear();
        stored_messages.clear();
        dirty = false;
    }

    bool write_by_value_supported() const override { return true; }
    bool write_by_reference_supported() const override { return true; }

    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override
    { 
        if (size > 3) {
            throw std::runtime_error("Midi merging buffer: message value dropped because size > 3");
        }
        if (stored_messages.size() >= stored_messages.capacity()) {
            std::cerr << "Warning: expanded MIDI buffer on processing thread\n";
            stored_messages.reserve(stored_messages.size() * 2);
        }
        stored_messages.push_back(Message(time, size, data));
        auto ptr = (MidiSortableMessageInterface*)&stored_messages.back();
        PROC_write_event_reference(*ptr);
    }

    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override {
        references.push_back(&m);
        dirty = true;
    }
};