#include "MidiPortInterface.h"
#include <memory>
#include <vector>
#include <stdint.h>
#include <boost/lockfree/spsc_queue.hpp>

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
class DecoupledMidiPort : public std::enable_shared_from_this<DecoupledMidiPort<TimeType, SizeType>> {
    using Message = DecoupledMidiMessage;
    using Queue = boost::lockfree::spsc_queue<Message>;

    const std::shared_ptr<MidiPortInterface> port;
    const PortDirection direction;
    Queue ma_queue;
public:
    DecoupledMidiPort (std::shared_ptr<MidiPortInterface> port,
                       size_t queue_size,
                       PortDirection direction);

    // Call this on the process thread to update message queues.
    void PROC_process(size_t n_frames);
    const char* name() const;

    std::optional<Message> pop_incoming();
    void push_outgoing (Message m);
};

extern template class DecoupledMidiPort<uint32_t, uint16_t>;
extern template class DecoupledMidiPort<uint32_t, uint32_t>;
extern template class DecoupledMidiPort<uint16_t, uint16_t>;
extern template class DecoupledMidiPort<uint16_t, uint32_t>;