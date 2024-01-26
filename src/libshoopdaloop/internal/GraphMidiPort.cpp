#include "GraphMidiPort.h"
#include "LoggingEnabled.h"
#include "MidiBufferInterfaces.h"
#include "MidiPort.h"
#include "types.h"
#include <memory>

GraphMidiPort::GraphMidiPort (std::shared_ptr<MidiPort> const& port,
                    std::shared_ptr<BackendSession> const& backend) :
    GraphPort(backend), port(port) {}

PortInterface &GraphMidiPort::get_port() const {
    return static_cast<PortInterface&>(*port);
}

MidiPort *GraphMidiPort::maybe_midi_port() const {
    return port.get();
}

void GraphMidiPort::PROC_passthrough(uint32_t n_frames) {
    if (n_frames == 0) { return; }
    
    log<log_level_debug_trace>("process MIDI passthrough ({} samples)", n_frames);
    auto get_buf = [&](auto &maybe_to) -> MidiWriteableBufferInterface* {
        if(auto _to = maybe_to.lock()) {
            if(auto port = _to->maybe_midi_port()) {
                return port->PROC_get_write_data_into_port_buffer(n_frames);
            }
        }
        return nullptr;
    };
    
    auto from_buf = port->PROC_get_read_output_data_buffer(n_frames);
    if (m_passthrough_enabled) {
        for (auto &p : mp_passthrough_to) {
            if(auto buf = get_buf(p)) {
                bool write_refs = buf->write_by_reference_supported();
                auto n = from_buf->PROC_get_n_events();
                log<log_level_debug_trace>("process MIDI passthrough: we have {} msgs total", n);
                for(decltype(n) i = 0; i<n; i++) {
                    auto &msg = from_buf->PROC_get_event_reference(i);
                    log<log_level_debug_trace>("process MIDI passthrough: send msg @ {}", msg.get_time());
                    if (write_refs) {
                        buf->PROC_write_event_reference(msg);
                    } else {
                        buf->PROC_write_event_value(
                            msg.get_size(),
                            msg.get_time(),
                            msg.get_data()
                        );
                    }
                }
            } else {
                log<log_level_debug_trace>("process MIDI passthrough: did not find target buffer.");
            }
        }
    } else {
        log<log_level_debug_trace>("process MIDI passthrough: skip (passthrough disabled)", n_frames);
    }
}

void GraphMidiPort::PROC_prepare(uint32_t n_frames) {
    port->PROC_prepare(n_frames);
}

void GraphMidiPort::PROC_process(uint32_t n_frames) {
    port->PROC_process(n_frames);
    PROC_passthrough(n_frames);
}