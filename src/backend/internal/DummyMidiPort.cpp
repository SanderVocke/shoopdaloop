#include "DummyMidiPort.h"
#include <algorithm>
#include "fmt/format.h"
#include <fmt/ranges.h>

#ifdef _WIN32
#undef min
#undef max
#endif


MidiSortableMessageInterface &
DummyMidiPort::PROC_get_event_reference(uint32_t idx) {
    try {
        if (!m_queued_msgs.empty()) {
            auto &m =  m_queued_msgs.at(idx);
            uint32_t time = m.get_time();
            ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Read queued midi message @ {}", time);
            return m;
        } else {
            auto &m =  m_buffer_data.at(idx);
            uint32_t time = m.get_time();
            ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Read buffer midi message @ {}", time);
            return m;
        }
    } catch (std::out_of_range &e) {
        throw std::runtime_error("Dummy midi port read error");
    }
}

void DummyMidiPort::PROC_write_event_value(uint32_t size, uint32_t time,
                                           const uint8_t *data)
{
    ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Write midi message value to internal buffer @ {}", time);
    m_buffer_data.push_back(StoredMessage(time, size, std::vector<uint8_t>(data, data + size)));
}

void DummyMidiPort::PROC_write_event_reference(
    MidiSortableMessageInterface const &m)
{
    uint32_t t = m.get_time();
    ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Write midi message reference @ {}", t);
    PROC_write_event_value(m.get_size(), m.get_time(), m.get_data());
}

bool DummyMidiPort::write_by_reference_supported() const { return true; }

bool DummyMidiPort::write_by_value_supported() const { return true; }

DummyMidiPort::DummyMidiPort(std::string name, shoop_port_direction_t direction, shoop_weak_ptr<DummyExternalConnections> external_connections)
    : MidiPort(true, true, true),
      DummyPort(name, direction, PortDataType::Midi, external_connections),
      WithCommandQueue(100) {}

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
        m_queued_msgs.push_back(StoredMessage(time, size, std::vector<uint8_t>(data, data + size)));
        std::stable_sort(this->m_queued_msgs.begin(), this->m_queued_msgs.end(), [](StoredMessage const& a, StoredMessage const& b) {
            return a.time < b.time;
        });
    });
}

bool DummyMidiPort::get_queue_empty() {
    bool is_empty = false;
    exec_process_thread_command([=,&is_empty]() {
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
        std::erase_if(m_queued_msgs, [&, this](StoredMessage const& msg) {
            auto rval = msg.time < progress_by;
            if (rval) {
                this->ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("msg dropped from MIDI dummy input queue");
            }
            return rval;
        });
        std::for_each(m_queued_msgs.begin(), m_queued_msgs.end(), [&](StoredMessage &msg) {
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
        std::stable_sort(m_buffer_data.begin(), m_buffer_data.end(), [](StoredMessage const& a, StoredMessage const& b) {
            return a.time < b.time;
        });
        if (!get_muted()) {
            for (auto &msg: m_buffer_data) {
                if (msg.time < n_requested_frames) {
                    uint32_t new_time = msg.time + (n_original_requested_frames - n_requested_frames);
                    ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Write midi message value to external queue @ {} -> {} {} {}", msg.time, new_time, n_original_requested_frames.load(), n_requested_frames.load());
                    m_written_requested_msgs.push_back(StoredMessage(new_time, msg.size, msg.data));
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

MidiReadableBufferInterface *
DummyMidiPort::PROC_get_read_output_data_buffer(uint32_t n_frames) {
    return (static_cast<MidiReadableBufferInterface *>(this));
}

MidiWriteableBufferInterface *
DummyMidiPort::PROC_get_write_data_into_port_buffer(uint32_t n_frames) {
    current_buf_frames = n_frames;
    m_buffer_data.clear();
    return (static_cast<MidiWriteableBufferInterface *>(this));
}

uint32_t DummyMidiPort::PROC_get_n_events() const {
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

bool DummyMidiPort::read_by_reference_supported() const { return true; }

void DummyMidiPort::PROC_get_event_value(uint32_t idx,
                                         uint32_t &size_out,
                                         uint32_t &time_out,
                                         const uint8_t *&data_out) {
    auto &msg = PROC_get_event_reference(idx);
    size_out = msg.get_size();
    time_out = msg.get_time();
    data_out = msg.get_data();
}

std::vector<DummyMidiPort::StoredMessage> DummyMidiPort::get_written_requested_msgs() {
    std::vector<DummyMidiPort::StoredMessage> result;
    exec_process_thread_command([=,&result]() {
        ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Dequeue (have {} msgs)", this->m_written_requested_msgs.size());
        result = this->m_written_requested_msgs;
        this->m_written_requested_msgs.clear();
    });
    return result;
}

DummyMidiPort::~DummyMidiPort() { DummyPort::close(); }