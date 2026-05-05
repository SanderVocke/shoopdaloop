#pragma once
#include "LoggingEnabled.h"
#include "MidiPort.h"
#include <memory>
#include <vector>
#include <stdint.h>
#include <boost/lockfree/spsc_queue.hpp>
#include "TracyPlotter.h"
#include "Checksum.h"

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

    // Tracy plotters for decoupled midi port debugging
    TracyPlotter m_plot_incoming_queue_size;
    TracyPlotter m_plot_outgoing_queue_size;

    // Checksum tracking for data consistency verification
    std::atomic<double> ma_input_checksum{0.0};
    std::atomic<double> ma_output_checksum{0.0};
    TracyPlotter m_plot_input_checksum;
    TracyPlotter m_plot_output_checksum;
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