#include "AudioMidiDriver.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include "WithCommandQueue.h"
#include "fmt/format.h"
#include <fmt/ranges.h>
#include "types.h"
#include <chrono>
#include <cstdint>
#include <string>
#include <thread>
#include "DummyAudioMidiDriver.h"
#include <map>
#include <algorithm>
#include <regex>

const std::map<DummyAudioMidiDriverMode, const char*> mode_names = {
    {DummyAudioMidiDriverMode::Automatic, "Automatic"},
    {DummyAudioMidiDriverMode::Controlled, "Controlled"}
};

DummyPort::DummyPort(
    std::string name,
    shoop_port_direction_t direction,
    PortDataType type,
    std::weak_ptr<DummyExternalConnections> external_connections
) : m_name(name), m_direction(direction), m_external_connections(external_connections) {}

const char* DummyPort::name() const { return m_name.c_str(); }

void DummyPort::close() {}

PortExternalConnectionStatus DummyPort::get_external_connection_status() const {
    if (auto e = m_external_connections.lock()) {
        return e->connection_status_of(this);
    }
    return PortExternalConnectionStatus();
}

void DummyPort::connect_external(std::string name) {
    if (auto e = m_external_connections.lock()) {
        e->connect(this, name);
    }
}

void DummyPort::disconnect_external(std::string name) {
    if (auto e = m_external_connections.lock()) {
        e->disconnect(this, name);
    }
}

void *DummyPort::maybe_driver_handle() const {
    return (void*)this;
}

DummyAudioPort::DummyAudioPort(std::string name, shoop_port_direction_t direction, std::weak_ptr<DummyExternalConnections> external_connections)
    : AudioPort<audio_sample_t>(), m_name(name),
      DummyPort(name, direction, PortDataType::Audio, external_connections),
      m_direction(direction),
      m_queued_data(128) { }

float *DummyAudioPort::PROC_get_buffer(uint32_t n_frames) {
    size_t new_size = std::max({m_buffer_data.size(), (size_t)n_frames, (size_t)1});
    if (new_size > m_buffer_data.size()) {
        m_buffer_data.resize(new_size);
    }
    auto rval = m_buffer_data.data();
    return rval;
}

void DummyAudioPort::queue_data(uint32_t n_frames, audio_sample_t const *data) {
    auto s = m_queued_data.read_available();
    auto v = std::vector<audio_sample_t>(data, data + n_frames);
    log<log_level_debug>("Queueing {} samples, {} sets queued total", n_frames, s);
    if (should_log<log_level_debug_trace>()) {
        log<log_level_debug_trace>("--> Queued samples: {}", v);
    }
    m_queued_data.push(v);
}

bool DummyAudioPort::get_queue_empty() {
    return m_queued_data.empty();
}

DummyAudioPort::~DummyAudioPort() { DummyPort::close(); }

void DummyAudioPort::PROC_process(uint32_t n_frames) {
    AudioPort<audio_sample_t>::PROC_process(n_frames);
    
    auto buf = PROC_get_buffer(n_frames);
    uint32_t to_store = std::min(n_frames, m_n_requested_samples.load());
    if (to_store > 0) {
        log<log_level_debug>("Buffering {} samples ({} total)", to_store, m_retained_samples.size() + to_store);
        if (should_log<log_level_debug_trace>()) {
            std::array<audio_sample_t, 16> arr;
            std::copy(buf, buf+std::min(to_store, (uint32_t)16), arr.begin());
            log<log_level_debug_trace>("--> first 16 buffered samples: {}", arr);
        }
        m_retained_samples.insert(m_retained_samples.end(), buf, buf+to_store);
        m_n_requested_samples -= to_store;
    }
}

void DummyAudioPort::PROC_prepare(uint32_t n_frames) {
    auto buf = PROC_get_buffer(n_frames);
    uint32_t filled = 0;
    while (!m_queued_data.empty() && filled < n_frames) {
        auto &front = m_queued_data.front();
        uint32_t to_copy = std::min((size_t)(n_frames - filled), front.size());
        uint32_t total_copyable = m_queued_data.front().size();
        log<log_level_debug>("Dequeueing {} of {} input samples", to_copy, total_copyable);
        if (should_log<log_level_debug_trace>()) {
            log<log_level_debug_trace>("--> Dequeued input samples: {}", front);
        }
        memcpy((void *)(buf + filled), (void *)front.data(),
               sizeof(audio_sample_t) * to_copy);
        filled += to_copy;
        front.erase(front.begin(), front.begin() + to_copy);
        if (front.size() == 0) {
            m_queued_data.pop();
            bool another = !m_queued_data.empty();
            log<log_level_debug>("Pop queue item. Another: {}", another);
        }
    }
    memset((void *)(buf+filled), 0, sizeof(audio_sample_t) * (n_frames - filled));
}

void DummyAudioPort::request_data(uint32_t n_frames) {
    m_n_requested_samples += n_frames;
}

std::vector<audio_sample_t> DummyAudioPort::dequeue_data(uint32_t n) {
    auto s = m_retained_samples.size();
    if (n > s) {
        throw_error<std::runtime_error>("Not enough retained samples");
    }
    log<log_level_debug>("Yielding {} of {} output samples", n, s);
    std::vector<audio_sample_t> rval(m_retained_samples.begin(), m_retained_samples.begin()+n);
    m_retained_samples.erase(m_retained_samples.begin(), m_retained_samples.begin()+n);
    if (should_log<log_level_debug_trace>()) {
        log<log_level_debug_trace>("--> Yielded samples: {}", rval);
    }
    return rval;
}

unsigned DummyAudioPort::input_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? External : Internal;
}

unsigned DummyAudioPort::output_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? Internal : External;
}

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

DummyMidiPort::DummyMidiPort(std::string name, shoop_port_direction_t direction, std::weak_ptr<DummyExternalConnections> external_connections)
    : MidiPort(true, false, false), DummyPort(name, direction, PortDataType::Midi, external_connections) {
}

unsigned DummyMidiPort::input_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? External : Internal;
}

unsigned DummyMidiPort::output_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? Internal : External;
}

void DummyMidiPort::clear_queues() {
    m_queued_msgs.clear();
    m_written_requested_msgs.clear();
    n_original_requested_frames = 0;
    n_requested_frames = 0;
}

void DummyMidiPort::queue_msg(uint32_t size, uint32_t time, uint8_t const *data) {
    ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Queueing midi message @ {}", time);
    m_queued_msgs.push_back(StoredMessage(time, size, std::vector<uint8_t>(data, data + size)));
    std::stable_sort(m_queued_msgs.begin(), m_queued_msgs.end(), [](StoredMessage const& a, StoredMessage const& b) {
        return a.time < b.time;
    });
}

bool DummyMidiPort::get_queue_empty() {
    return m_queued_msgs.empty();
}

void DummyMidiPort::request_data(uint32_t n_frames) {
    if (n_requested_frames > 0) {
        throw std::runtime_error("Previous request not yet completed");
    }
    ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("request {} frames", n_frames);
    n_requested_frames = n_frames;
    n_original_requested_frames = n_frames;
}

void DummyMidiPort::PROC_prepare(uint32_t nframes) {
    m_buffer_data.clear();
    auto progress_by = n_processed_last_round.load();
    progress_by -= std::min(n_requested_frames.load(), progress_by);
    if (progress_by > 0) {
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
}

void DummyMidiPort::PROC_process(uint32_t nframes) {
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
    auto rval = m_written_requested_msgs;
    m_written_requested_msgs.clear();
    return rval;
}

DummyMidiPort::~DummyMidiPort() { DummyPort::close(); }

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::enter_mode(DummyAudioMidiDriverMode mode) {
    if (m_mode.load() != mode) {
        Log::log<log_level_debug>("DummyAudioMidiDriver: mode -> {}", mode_names.at(mode));
        m_mode = mode;
        m_controlled_mode_samples_to_process = 0;

        // Ensure we finish any processing we were doing
        wait_process();
    }
}

template <typename Time, typename Size>
DummyAudioMidiDriverMode DummyAudioMidiDriver<Time, Size>::get_mode() const {
    return m_mode;
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::controlled_mode_request_samples(uint32_t samples) {
    m_controlled_mode_samples_to_process += samples;
    uint32_t requested = m_controlled_mode_samples_to_process.load();
    Log::log<log_level_debug>("DummyAudioMidiDriver: request {} samples ({} total)", samples, requested);
}

template <typename Time, typename Size>
uint32_t DummyAudioMidiDriver<Time, Size>::get_controlled_mode_samples_to_process() const {
    return m_controlled_mode_samples_to_process;
}


template <typename Time, typename Size>
DummyAudioMidiDriver<Time, Size>::DummyAudioMidiDriver()
    : AudioMidiDriver(),
      m_finish(false),
      m_paused(false),
      m_mode(DummyAudioMidiDriverMode::Automatic),
      m_controlled_mode_samples_to_process(0),
      m_audio_port_closed_cb(nullptr), m_audio_port_opened_cb(nullptr),
      m_midi_port_closed_cb(nullptr), m_midi_port_opened_cb(nullptr),
      m_external_connections(std::make_shared<DummyExternalConnections>())
{
    m_audio_ports.clear();
    m_midi_ports.clear();

    Log::log<log_level_debug>("DummyAudioMidiDriver: constructed");
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::start(
    AudioMidiDriverSettingsInterface &settings
) {
    auto p_settings = (DummyAudioMidiDriverSettings*)&settings;
    auto &_settings = *p_settings;

    AudioMidiDriver::set_sample_rate(_settings.sample_rate);
    AudioMidiDriver::set_buffer_size(_settings.buffer_size);
    m_client_name_str = _settings.client_name;
    AudioMidiDriver::set_client_name(m_client_name_str.c_str());
    AudioMidiDriver::set_dsp_load(0.0f);
    AudioMidiDriver::set_maybe_client_handle(nullptr);

    Log::log<log_level_debug>("Starting (sample rate {}, buf size {})", _settings.sample_rate, _settings.buffer_size);

    // Processing the command queue once will ensure that it knows processing is active.
    // That way commands added from now on will be executed on the process thread.
    ma_queue.PROC_exec_all();

    m_proc_thread = std::thread([this] {
        Log::log<log_level_debug>("Starting process thread - {}", mode_names.at(m_mode));
        auto bufs_per_second = AudioMidiDriver::get_sample_rate() / AudioMidiDriver::get_buffer_size();
        auto interval = 1.0f / ((float)bufs_per_second);
        auto micros = uint32_t(interval * 1000000.0f);
        float time_taken = 0.0f;
        while (!this->m_finish) {
            std::this_thread::sleep_for(std::chrono::microseconds((uint32_t)std::ceil(std::max(0.0f, micros - time_taken))));
            PROC_handle_command_queue();
            if (!m_paused) {
                auto start = std::chrono::high_resolution_clock::now();
                auto mode = m_mode.load();
                auto samples_to_process = m_controlled_mode_samples_to_process.load();
                uint32_t to_process = mode == DummyAudioMidiDriverMode::Controlled ?
                    std::min(samples_to_process, AudioMidiDriver::get_buffer_size()) :
                    AudioMidiDriver::get_buffer_size();
                Log::log<log_level_debug_trace>("Process {}", to_process);
                AudioMidiDriver::PROC_process(to_process);
                if (mode == DummyAudioMidiDriverMode::Controlled) {
                    m_controlled_mode_samples_to_process -= to_process;
                }
                auto end = std::chrono::high_resolution_clock::now();
                time_taken = duration_cast<std::chrono::microseconds>(end - start).count();
            }
        }
        Log::log<log_level_debug>("Ending process thread");
    });
    AudioMidiDriver::set_active(true);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::pause() {
    Log::log<log_level_debug>("DummyAudioMidiDriver: pause");
    m_paused = true;
    wait_process();
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::resume() {
    Log::log<log_level_debug>("DummyAudioMidiDriver: resume");
    m_paused = false;
}

template <typename Time, typename Size>
DummyAudioMidiDriver<Time, Size>::~DummyAudioMidiDriver() {
    close();
}

template <typename Time, typename Size>
std::shared_ptr<AudioPort<audio_sample_t>>
DummyAudioMidiDriver<Time, Size>::open_audio_port(std::string name,
                                              shoop_port_direction_t direction) {
    Log::log<log_level_debug>("DummyAudioMidiDriver : add audio port");
    auto rval = std::make_shared<DummyAudioPort>(name, direction, m_external_connections);
    m_audio_ports.insert(rval);
    return rval;
}

template <typename Time, typename Size>
std::shared_ptr<MidiPort>
DummyAudioMidiDriver<Time, Size>::open_midi_port(std::string name,
                                             shoop_port_direction_t direction) {
    Log::log<log_level_debug>("DummyAudioMidiDriver: add midi port");
    auto rval = std::make_shared<DummyMidiPort>(name, direction, m_external_connections);
    m_midi_ports.insert(rval);
    return rval;
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::close() {
    m_finish = true;
    if (m_proc_thread.joinable()) {
        if (std::this_thread::get_id() != m_proc_thread.get_id()) {
            m_proc_thread.join();
        } else {
            m_proc_thread.detach();
        }
    }
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::controlled_mode_run_request(uint32_t timeout) {
    Log::log<log_level_debug>("DummyAudioMidiDriver: run request");
    auto s = std::chrono::high_resolution_clock::now();
    auto timed_out = [this, &timeout, &s]() {
        return std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::high_resolution_clock::now() - s
        ).count() >= timeout;
    };
    while(
        m_mode == DummyAudioMidiDriverMode::Controlled &&
        m_controlled_mode_samples_to_process > 0 &&
        !timed_out()
    ) {
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }

    wait_process();

    if (m_controlled_mode_samples_to_process > 0) {
        Log::log<log_level_error>("DummyAudioMidiDriver: run request timed out");
    }
}

std::vector<ExternalPortDescriptor> DummyExternalConnections::find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    )
{
    std::vector<ExternalPortDescriptor> rval;
    for(auto &p : m_external_mock_ports) {
        bool name_matched = (!maybe_name_regex || std::regex_match(p.name, std::regex(std::string(maybe_name_regex))));
        bool direction_matched = (maybe_direction_filter == ShoopPortDirection_Any) || maybe_direction_filter == p.direction;
        bool data_type_matched = (maybe_data_type_filter == ShoopPortDataType_Any) || maybe_data_type_filter == p.data_type;
        if (name_matched && direction_matched && data_type_matched) {
            rval.push_back(p);
        }
    }

    return rval;
}

template<typename Time, typename Size>
std::vector<ExternalPortDescriptor> DummyAudioMidiDriver<Time, Size>::find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    )
{
    return m_external_connections->find_external_ports(maybe_name_regex, maybe_direction_filter, maybe_data_type_filter);
}

void DummyExternalConnections::add_external_mock_port(std::string name, shoop_port_direction_t direction, shoop_port_data_type_t data_type) {
    if (std::find_if(m_external_mock_ports.begin(), m_external_mock_ports.end(), [name](auto &a) { return a.name == name; }) == m_external_mock_ports.end()) {
        m_external_mock_ports.push_back(ExternalPortDescriptor {
            .name = name,
            .direction = direction,
            .data_type = data_type
        });
    }
}

void DummyExternalConnections::remove_external_mock_port(std::string name) {
    auto new_end = std::remove_if(m_external_mock_ports.begin(), m_external_mock_ports.end(), [name](auto &a) { return a.name == name; });
    
    if(!(new_end == m_external_mock_ports.end())) {
        m_external_mock_ports.erase(new_end, m_external_mock_ports.end());
        // Remove connections also
        auto new_conns_end = std::remove_if(m_external_connections.begin(), m_external_connections.end(), [name](auto &a) { return a.second == name; });
        m_external_connections.erase(
            new_conns_end,
            m_external_connections.end()
        );
    }
}

void DummyExternalConnections::remove_all_external_mock_ports() {
    m_external_mock_ports.clear();
    m_external_connections.clear();
}

ExternalPortDescriptor &DummyExternalConnections::get_port(std::string name) {
    auto it = std::find_if(m_external_mock_ports.begin(), m_external_mock_ports.end(), [name](auto &a) { return a.name == name; });
    if (it == m_external_mock_ports.end()) {
        throw std::runtime_error("Port not found");
    }
    return *it;
}

void DummyExternalConnections::connect(DummyPort* port, std::string external_port_name) {
    auto pname = port->name();
    log<log_level_debug>("connect {} to {}", pname, external_port_name);
    auto &desc = get_port(external_port_name);
    auto conn = std::make_pair(port, desc.name);
    if (std::find(m_external_connections.begin(), m_external_connections.end(), conn) == m_external_connections.end()) {
        m_external_connections.push_back(conn);
    }
}

void DummyExternalConnections::disconnect(DummyPort* port, std::string external_port_name) {
    auto pname = port->name();
    log<log_level_debug>("disconnect {} from {}", pname, external_port_name);
    auto &desc = get_port(external_port_name);
    auto conn = std::make_pair(port, desc.name);
    auto new_end = std::remove(m_external_connections.begin(), m_external_connections.end(), conn);
    m_external_connections.erase(new_end, m_external_connections.end());
}

PortExternalConnectionStatus DummyExternalConnections::connection_status_of(const DummyPort* port) {
    PortExternalConnectionStatus rval;
    for (auto &conn : m_external_connections) {
        rval[conn.second] = conn.first == port;
    }
    return rval;
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::add_external_mock_port(std::string name, shoop_port_direction_t direction, shoop_port_data_type_t data_type) {
    Log::log<log_level_debug>("add external mock port {}", name);
    m_external_connections->add_external_mock_port(name, direction, data_type);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::remove_external_mock_port(std::string name) {
    Log::log<log_level_debug>("remove external mock port {}", name);
    m_external_connections->remove_external_mock_port(name);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::remove_all_external_mock_ports() {
    Log::log<log_level_debug>("remove all external mock ports");
    m_external_connections->remove_all_external_mock_ports();
}

template class DummyAudioMidiDriver<uint32_t, uint16_t>;
template class DummyAudioMidiDriver<uint32_t, uint32_t>;
template class DummyAudioMidiDriver<uint16_t, uint16_t>;
template class DummyAudioMidiDriver<uint16_t, uint32_t>;
template class DummyAudioMidiDriver<uint32_t, uint64_t>;
template class DummyAudioMidiDriver<uint64_t, uint64_t>;