#pragma once
#include "MidiBuffer.h"

// The MIDI merging buffer can be used to store unordered MIDI messages,
// sort them efficiently and output them in order.
class MidiSortingBuffer : public MidiReadableBuffer, public MidiWriteableBuffer {
    static constexpr uint32_t buffer_size = 1024;

    std::vector<MidiStorageElem> messages;
    bool dirty = false;

public:
    MidiSortingBuffer();

    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;

    void PROC_process(uint32_t nframes);
    void PROC_prepare(uint32_t nframes);
    
    void PROC_sort();
    void PROC_clear();

    void write_event(MidiStorageElem event) override;
};