#include "DummyMidiBufs.h"
#include <stdexcept>

uint32_t DummyReadMidiBuf::PROC_get_n_events() const { return 0; }

MidiSortableMessageInterface &
DummyReadMidiBuf::PROC_get_event_reference(uint32_t idx) {
    throw std::runtime_error("Attempt to read from dummy buffer");
}

void DummyWriteMidiBuf::PROC_write_event_value(uint32_t size, uint32_t time,
                                               const uint8_t *data) {}

void DummyWriteMidiBuf::PROC_write_event_reference(
    MidiSortableMessageInterface const &m) {}

bool DummyWriteMidiBuf::write_by_value_supported() const { return true; }

bool DummyWriteMidiBuf::write_by_reference_supported() const { return true; }
