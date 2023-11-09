#include "MidiPortInterface.h"
#include "MidiMessage.h"

// The MIDI merging buffer can be used to store unordered MIDI messages,
// sort them efficiently and output them in order.
class MidiMergingBuffer : public MidiReadableBufferInterface,
                          public MidiWriteableBufferInterface {
    static constexpr uint32_t buffer_size = 1024;
    static constexpr uint32_t stored_messages_size = 256;

    using Message = MaxSizeMidiMessage<uint32_t, uint32_t, 3>;

    std::vector<const MidiSortableMessageInterface*> references;
    std::vector<Message> stored_messages;
    bool dirty;

public:
    MidiMergingBuffer();

    uint32_t PROC_get_n_events() const override;

    MidiSortableMessageInterface const & PROC_get_event_reference(uint32_t idx) override;

    void PROC_sort();

    void PROC_clear();

    bool write_by_value_supported() const override;
    bool write_by_reference_supported() const override;

    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override;

    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
};