#include "JackMidiPort.h"
#include <string>
#include "MidiPort.h"
#include "MidiSortingReadWritePort.h"
#include <stdexcept>

template<typename API>
MidiReadableBufferInterface *GenericJackMidiInputPort<API>::PROC_internal_read_input_data_buffer(uint32_t nframes) {
    return static_cast<MidiReadableBufferInterface*>(this);
}

template<typename API>
MidiWriteableBufferInterface *GenericJackMidiOutputPort<API>::PROC_internal_write_output_data_to_buffer(uint32_t nframes) {
    return nullptr;
}

template<typename API>
void GenericJackMidiOutputPort<API>::PROC_prepare(uint32_t nframes) {
    MidiSortingReadWritePort::PROC_prepare(nframes);
    GenericJackMidiPort<API>::PROC_prepare(nframes);
}

template class GenericJackMidiInputPort<JackApi>;
template class GenericJackMidiInputPort<JackTestApi>;
template class GenericJackMidiOutputPort<JackApi>;
template class GenericJackMidiOutputPort<JackTestApi>;