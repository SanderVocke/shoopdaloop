#pragma once
#include "LoggingEnabled.h"
#include "MidiPort.h"
#include "MidiBuffer.h"
#include <memory>
#include <vector>
#include <stdint.h>
#include "backend_rust/src/decoupled_midi_port_cxx.rs.h"

class AudioMidiDriver;

// A decoupled MIDI port is a MIDI port with message queues attached to it.
// Incoming messages are stored into the queue and outgoing ones taken form a queue.
// This way, port messaging can be easily handled outside of the processing thread.
// Time information is discarded for decoupled messages (intended for controllers).
class DecoupledMidiPort : public shoop_enable_shared_from_this<DecoupledMidiPort>,
                          private ModuleLoggingEnabled<"Backend.DecoupledMidiPort"> {
    using Message = MidiStorageElem;

    const shoop_shared_ptr<MidiPort> port;
    const shoop_port_direction_t direction;
    rust::Box<backend_rust::DecoupledMidiPort> m_rust;
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
