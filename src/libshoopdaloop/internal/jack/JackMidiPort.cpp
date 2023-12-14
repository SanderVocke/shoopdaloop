#include "JackMidiPort.h"
#include <string>
#include "MidiPort.h"
#include "MidiSortingReadWritePort.h"
#include <stdexcept>

template<typename API>
GenericJackMidiInputPort<API>::GenericJackMidiInputPort(
        std::string name,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackMidiPort<API>(name, Input, client, all_ports_tracker),
        MidiBufferingInputPort()
{}

template<typename API>
void GenericJackMidiInputPort<API>::PROC_prepare(uint32_t nframes) {
    MidiBufferingInputPort::PROC_prepare(nframes);
    GenericJackMidiPort<API>::PROC_prepare(nframes);

    // TODO
}

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