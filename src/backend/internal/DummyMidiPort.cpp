#include "DummyMidiPort.h"
#include <algorithm>
#include "fmt/format.h"
#include <fmt/ranges.h>

#ifdef _WIN32
#undef min
#undef max
#endif

MidiStorageElem DummyMidiPort::get_event(uint32_t idx) const {
    try {
        if (!m_queued_msgs.empty()) {
            auto &m = m_queued_msgs.at(idx);
            ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Read queued midi message @ {}", m.time);
            return m;
        } else {
            auto &m = m_buffer_data.at(idx);
            ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Read buffer midi message @ {}", m.time);
            return m;
        }
    } catch (std::out_of_range &e) {
        throw std::runtime_error("Dummy midi port read error");
    }
}

void DummyMidiPort::write_event(MidiStorageElem event) {
    ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Write midi message value to internal buffer @ {}", event.time);
    m_buffer_data.push_back(event);
}

DummyMidiPort::DummyMidiPort(std::string name, shoop_port_direction_t direction, shoop_weak_ptr<DummyExternalConnections> external_connections)
    : MidiPort(true, true, true),
      DummyPort(name, direction, PortDataType::Midi, external_connections),
      WithCommandQueue(100, 1000, 1000) {}

unsigned DummyMidiPort::input_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? ShoopPortConnectability_External : ShoopPortConnectability_Internal;
}

unsigned DummyMidiPort::output_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? ShoopPortConnectability_Internal : ShoopPortConnectability_External;
}

void DummyMidiPort::clear_queues() {
    exec_process_thread_command([=]() {
        this->m_queued_msgs.clear();
        this->m_written_requested_msgs.clear();
        this->n_original_requested_frames = 0;
        this->n_requested_frames = 0;
    });
}

void DummyMidiPort::queue_msg(uint32_t size, uint32_t time, uint8_t const *data) {
    exec_process_thread_command([=]() {
        ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Queueing midi message @ {}", time);
        MidiStorageElem msg;
        msg.time = time;
        msg.size = size;
        memcpy(msg.bytes, data, size);
        m_queued_msgs.push_back(msg);
        std::stable_sort(this->m_queued_msgs.begin(), this->m_queued_msgs.end(), [](const MidiStorageElem &a, const MidiStorageElem &b) {
            return a.time < b.time;
        });
    });
}

bool DummyMidiPort::get_queue_empty() {
    bool is_empty = false;
    exec_process_thread_command([=, &is_empty]() {
        is_empty = this->m_queued_msgs.empty();
    });
    return is_empty;
}

void DummyMidiPort::request_data(uint32_t n_frames) {
    exec_process_thread_command([=]() {
        if (this->n_requested_frames > 0) {
            throw std::runtime_error("Previous request not yet completed");
        }
        ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("request {} frames", n_frames);
        this->n_requested_frames = n_frames;
        this->n_original_requested_frames = n_frames;
    });
}

void DummyMidiPort::PROC_prepare(uint32_t nframes) {
    PROC_handle_command_queue();
    m_buffer_data.clear();
    auto progress_by = n_processed_last_round.load();
    progress_by -= std::min(n_requested_frames.load(), progress_by);
    if (progress_by > 0 && m_queued_msgs.size() > 0) {
        this->ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("Progress queue by {}", progress_by);

        // The queue was used last pass and needs to be truncated now for the current pass.
        // (first erase msgs that will end up having a negative timestamp)
        std::erase_if(m_queued_msgs, [&, this](const MidiStorageElem &msg) {
            auto rval = msg.time < progress_by;
            if (rval) {
                this->ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("msg dropped from MIDI dummy input queue");
            }
            return rval;
        });
        std::for_each(m_queued_msgs.begin(), m_queued_msgs.end(), [&](MidiStorageElem &msg) {
            auto new_val = msg.time - progress_by;
            ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("msg in queue: time {} -> {}", msg.time, new_val);
            msg.time = new_val;
        });
    }
    n_processed_last_round = 0;
    current_buf_frames = nframes;
    MidiPort::PROC_prepare(nframes);
}

void DummyMidiPort::PROC_process(uint32_t nframes) {
    if (nframes > 0) {
        ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("Process {}", nframes);
    }
    if (m_direction == shoop_port_direction_t::ShoopPortDirection_Output) {
        std::stable_sort(m_buffer_data.begin(), m_buffer_data.end(), [](const MidiStorageElem &a, const MidiStorageElem &b) {
            return a.time < b.time;
        });
        if (!get_muted()) {
            for (auto &msg: m_buffer_data) {
                if (msg.time < n_requested_frames) {
                    uint32_t new_time = msg.time + (n_original_requested_frames - n_requested_frames);
                    ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Write midi message value to external queue @ {} -> {} {} {}", msg.time, new_time, n_original_requested_frames.load(), n_requested_frames.load());
                    MidiStorageElem out_msg = msg;
                    out_msg.time = new_time;
                    m_written_requested_msgs.push_back(out_msg);
                    // Track event through base
                    base().increment_output_events(1);
                }
            }
        } else {
            ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("{} messages not propagated to external due to being muted", m_buffer_data.size());
        }
    }
    n_processed_last_round = nframes;
    n_requested_frames -= std::min(nframes, n_requested_frames.load());
    MidiPort::PROC_process(nframes);
}

MidiReadableBuffer *
DummyMidiPort::PROC_get_read_output_data_buffer(uint32_t n_frames) {
    return this;
}

IMidiReadableBuffer *
DummyMidiPort::get_readable_buffer() {
    return this;
}

MidiWriteableBuffer *
DummyMidiPort::PROC_get_write_data_into_port_buffer(uint32_t n_frames) {
    current_buf_frames = n_frames;
    m_buffer_data.clear();
    return this;
}

IMidiWriteableBuffer *
DummyMidiPort::get_writeable_buffer() {
    return this;
}

uint32_t DummyMidiPort::n_events() const {
    uint32_t r = 0;
    if (!m_queued_msgs.empty()) {
        for (auto it = m_queued_msgs.begin(); it != m_queued_msgs.end(); ++it) {
            if (it->time < current_buf_frames) {
                r++;
            } else {
                break;
            }
        }
        return r;
    }
    return m_buffer_data.size();
}

std::vector<DummyMidiPort::StoredMessage> DummyMidiPort::get_written_requested_msgs() {
    std::vector<DummyMidiPort::StoredMessage> result;
    exec_process_thread_command([=, &result]() {
        ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Dequeue (have {} msgs)", this->m_written_requested_msgs.size());
        result = this->m_written_requested_msgs;
        this->m_written_requested_msgs.clear();
    });
    return result;
}

DummyMidiPort::~DummyMidiPort() { DummyPort::close(); }