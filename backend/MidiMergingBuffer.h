#include "MidiPortInterface.h"
#include <vector>
#include <algorithm>

// The MIDI merging buffer can be used to store unordered MIDI messages
// by reference, sort them efficiently and output them in order.
class MidiMergingBuffer : public MidiReadableBufferInterface,
                          public MidiWriteableBufferInterface {
    static constexpr size_t buffer_size = 1024;
    std::vector<const MidiSortableMessageInterface*> references;
    bool dirty;

    struct {
        bool operator()(const MidiSortableMessageInterface* a, const MidiSortableMessageInterface* b) const {
            return a->get_time() < b->get_time();
        }
    } compare;

public:
    MidiMergingBuffer() {
        references.reserve(buffer_size);
    }

    size_t PROC_get_n_events() const override {
        return references.size();
    }

    MidiSortableMessageInterface const & PROC_get_event_reference(size_t idx) const {
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

    bool write_by_value_supported() const override { return false; }
    bool write_by_reference_supported() const override { return true; }

    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override
    { throw std::runtime_error("Invalid use of buffer"); }

    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override {
        references.push_back(&m);
        dirty = true;
    }
};