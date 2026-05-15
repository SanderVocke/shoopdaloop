#include "DummyMidiBufs.h"
#include <stdexcept>

uint32_t DummyReadMidiBuf::n_events() const { return 0; }

MidiStorageElem DummyReadMidiBuf::get_event(uint32_t idx) const {
    (void)idx;
    throw std::runtime_error("Attempt to read from dummy buffer");
}

void DummyWriteMidiBuf::write_event(MidiStorageElem event) {
    (void)event;
    // Discard event
}