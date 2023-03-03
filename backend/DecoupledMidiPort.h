#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <memory>
#include <optional>
#include <boost/lockfree/spsc_queue.hpp>
#include <cstring>

// We discard time information for decoupled midi messages.
// The intended use cases is controllers, where the time is
// not very relevant.
struct DecoupledMidiMessage {
    uint32_t size;
    std::vector<uint8_t> data;
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
                       PortDirection direction) :
        port(port), ma_queue(queue_size), direction(direction) {};

    // Call this on the process thread to update message queues.
    void PROC_process(size_t n_frames) {
        if (direction == PortDirection::Input) {
            auto buf = port->PROC_get_read_buffer(n_frames);
            auto n = buf->PROC_get_n_events();
            for(size_t idx=0; idx < n; idx++) {
                Message m;
                const uint8_t *data;
                uint32_t time;
                uint32_t size;
                buf->PROC_get_event_reference(idx).get(size, time, data);
                m.size = size;
                m.data = std::vector<uint8_t>(m.size);
                memcpy((void*)m.data.data(), (void*)data, m.size);
                ma_queue.push(m);
            }
        } else {
            auto buf = port->PROC_get_write_buffer(n_frames);
            Message m;
            while(ma_queue.pop(m)) {
                buf->PROC_write_event_value(m.size, 0, m.data.data());
            }
        }
    }

    std::optional<Message> pop_incoming() {
        if (direction != PortDirection::Input) {
            throw std::runtime_error("Attempt to pop input message from output port");
        }
        Message m;
        if(ma_queue.pop(m)) {
            return m;
        }
        return std::nullopt;
    }

    void push_outgoing (Message m) {
        ma_queue.push(m);
    }
};