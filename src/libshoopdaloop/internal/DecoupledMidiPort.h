#pragma once
#include "LoggingEnabled.h"
#include "MidiPort.h"
#include <memory>
#include <vector>
#include <stdint.h>
#include <boost/lockfree/spsc_queue.hpp>

class AudioMidiDriver;

// We discard time information for decoupled midi messages.
// The intended use cases is controllers, where the time is
// not very relevant.
struct DecoupledMidiMessage {
    std::vector<unsigned char> data;
};

// A decoupled MIDI port is a MIDI port with message queues attached to it.
// Incoming messages are stored into the queue and outgoing ones taken form a queue.
// This way, port messaging can be easily handled outside of the processing thread.
template<typename TimeType, typename SizeType>
class DecoupledMidiPort : public shoop_enable_shared_from_this<DecoupledMidiPort<TimeType, SizeType>>,
                          private ModuleLoggingEnabled<"Backend.DecoupledMidiPort"> {
    using Message = DecoupledMidiMessage;
    using Queue = boost::lockfree::spsc_queue<Message>;

    const shoop_shared_ptr<MidiPort> port;
    const shoop_port_direction_t direction;
    Queue ma_queue;
    shoop_weak_ptr<AudioMidiDriver> maybe_driver;
public:
    DecoupledMidiPort (shoop_shared_ptr<MidiPort> port,
                       shoop_weak_ptr<AudioMidiDriver> driver,
                       uint32_t queue_size,
                       shoop_port_direction_t direction);

    // Call this on the process thread to update message queues.
    void PROC_process(uint32_t n_frames);
    const char* name() const;
    void close();

    shoop_shared_ptr<AudioMidiDriver> get_maybe_driver() const;
    void forget_driver();

    std::optional<Message> pop_incoming();
    void push_outgoing (Message m);

    shoop_shared_ptr<MidiPort> const& get_port();
};

extern template class DecoupledMidiPort<uint32_t, uint16_t>;
extern template class DecoupledMidiPort<uint32_t, uint32_t>;
extern template class DecoupledMidiPort<uint16_t, uint16_t>;
extern template class DecoupledMidiPort<uint16_t, uint32_t>;