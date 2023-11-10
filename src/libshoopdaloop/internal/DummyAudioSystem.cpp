#include "PortInterface.h"
#include "WithCommandQueue.h"
#include "types.h"
#include <chrono>
#include <cstdint>
#include <thread>
#include "DummyAudioSystem.h"
#include <map>
#include <algorithm>

const std::map<DummyAudioSystemMode, const char*> mode_names = {
    {DummyAudioSystemMode::Automatic, "Automatic"},
    {DummyAudioSystemMode::Controlled, "Controlled"}
};

DummyPort::DummyPort(
    std::string name,
    PortDirection direction,
    PortType type
) : m_name(name), m_direction(direction), m_type(type) {}

const char* DummyPort::name() const { return m_name.c_str(); }

PortDirection DummyPort::direction() const { return m_direction; }

PortType DummyPort::type() const { return m_type; }

void DummyPort::close() {}

PortExternalConnectionStatus DummyPort::get_external_connection_status() const { return PortExternalConnectionStatus(); }

void DummyPort::connect_external(std::string name) {}

void DummyPort::disconnect_external(std::string name) {}

DummyAudioPort::DummyAudioPort(std::string name, PortDirection direction)
    : AudioPortInterface<audio_sample_t>(name, direction), m_name(name),
      DummyPort(name, direction, PortType::Audio),
      m_direction(direction),
      m_queued_data(128) { }

float *DummyAudioPort::PROC_get_buffer(uint32_t n_frames, bool do_zero) {
    m_buffer_data.resize(std::max(m_buffer_data.size(), (size_t)n_frames));
    auto rval = m_buffer_data.data();
    uint32_t filled = 0;
    while (!m_queued_data.empty() && filled < n_frames) {
        auto &front = m_queued_data.front();
        uint32_t to_copy = std::min((size_t)(n_frames - filled), front.size());
        uint32_t total_copyable = m_queued_data.front().size();
        log<debug>("Dequeueing {} of {} samples", to_copy, total_copyable);
        memcpy((void *)(rval + filled), (void *)front.data(),
               sizeof(audio_sample_t) * to_copy);
        filled += to_copy;
        front.erase(front.begin(), front.begin() + to_copy);
        if (front.size() == 0) {
            m_queued_data.pop();
            bool another = !m_queued_data.empty();
            log<debug>("Pop queue item. Another: {}", another);
        }
    }
    memset((void *)(rval+filled), 0, sizeof(audio_sample_t) * (n_frames - filled));

    if (do_zero) {
        memset((void *)rval, 0, sizeof(audio_sample_t) * n_frames);
    }

    return rval;
}

void DummyAudioPort::queue_data(uint32_t n_frames, audio_sample_t const *data) {
    m_queued_data.push(
        std::vector<audio_sample_t>(data, data + n_frames)
    );
}

bool DummyAudioPort::get_queue_empty() {
    return m_queued_data.empty();
}

DummyAudioPort::~DummyAudioPort() { DummyPort::close(); }

void DummyAudioPort::PROC_post_process(float* buf, uint32_t n_frames) {
    uint32_t to_store = std::min(n_frames, m_n_requested_samples.load());
    if (to_store > 0) {
        log<debug>("Storing {} samples", to_store);
        m_retained_samples.insert(m_retained_samples.end(), buf, buf+to_store);
    }
}

void DummyAudioPort::request_data(uint32_t n_frames) {
    m_n_requested_samples += n_frames;
}

std::vector<audio_sample_t> DummyAudioPort::dequeue_data(uint32_t n) {
    if (n > m_retained_samples.size()) {
        throw_error<std::runtime_error>("Not enough retained samples");
    }
    std::vector<audio_sample_t> rval(m_retained_samples.begin(), m_retained_samples.begin()+n);
    m_retained_samples.erase(m_retained_samples.begin(), m_retained_samples.begin()+n);
    return rval;
}

MidiSortableMessageInterface &
DummyMidiPort::PROC_get_event_reference(uint32_t idx) {
    try {
        if (!m_queued_msgs.empty()) {
            auto &m =  m_queued_msgs.at(idx);
            uint32_t time = m.get_time();
            log<debug>("Read queued midi message @ {}", time);
            return m;
        } else {
            auto &m =  m_buffer_data.at(idx);
            uint32_t time = m.get_time();
            log<debug>("Read buffer midi message @ {}", time);
            return m;
        }
    } catch (std::out_of_range &e) {
        throw std::runtime_error("Dummy midi port read error");
    }
}

void DummyMidiPort::PROC_write_event_value(uint32_t size, uint32_t time,
                                           const uint8_t *data)
{
    if (time < n_requested_frames) {
        uint32_t new_time = time + (n_original_requested_frames - n_requested_frames);
        log<debug>("Write midi message value to external queue @ {} -> {}", time, new_time);
        m_written_requested_msgs.push_back(StoredMessage(new_time, size, std::vector<uint8_t>(data, data + size)));
    }
    log<debug>("Write midi message value to internal buffer @ {}", time);
    m_buffer_data.push_back(StoredMessage(time, size, std::vector<uint8_t>(data, data + size)));
}

void DummyMidiPort::PROC_write_event_reference(
    MidiSortableMessageInterface const &m)
{
    uint32_t t = m.get_time();
    log<debug>("Write midi message reference @ {}", t);
    PROC_write_event_value(m.get_size(), m.get_time(), m.get_data());    
}

bool DummyMidiPort::write_by_reference_supported() const { return true; }

bool DummyMidiPort::write_by_value_supported() const { return true; }

DummyMidiPort::DummyMidiPort(std::string name, PortDirection direction)
    : MidiPortInterface(name, direction), DummyPort(name, direction, PortType::Midi){
}

void DummyMidiPort::clear_queues() {
    m_queued_msgs.clear();
    m_written_requested_msgs.clear();
    n_original_requested_frames = 0;
    n_requested_frames = 0;
}

void DummyMidiPort::queue_msg(uint32_t size, uint32_t time, uint8_t const *data) {
    log<debug>("Queueing midi message @ {}", time);
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
    n_requested_frames = n_frames;
    n_original_requested_frames = n_frames;
}

MidiReadableBufferInterface &
DummyMidiPort::PROC_get_read_buffer(uint32_t n_frames) {
    current_buf_frames = n_frames;

    uint32_t update_queue_by = m_update_queue_by_frames_pending;
    if (update_queue_by > 0) {
        // The queue was used last pass and needs to be truncated now for the current pass.
        // (first erase msgs that will end up having a negative timestamp)
        std::erase_if(m_queued_msgs, [&](StoredMessage const& msg) {
            return msg.time < update_queue_by;
        });
        std::for_each(m_queued_msgs.begin(), m_queued_msgs.end(), [&](StoredMessage &msg) {
            msg.time -= update_queue_by;
        });
        m_update_queue_by_frames_pending = 0;
    }

    return *(static_cast<MidiReadableBufferInterface *>(this));
}

MidiWriteableBufferInterface &
DummyMidiPort::PROC_get_write_buffer(uint32_t n_frames) {
    current_buf_frames = n_frames;
    m_buffer_data.clear();
    return *(static_cast<MidiWriteableBufferInterface *>(this));
}

void DummyMidiPort::PROC_post_process(uint32_t n_frames) {
    // Decrement timestamps of all midi messages.

    n_requested_frames -= std::min(n_requested_frames.load(), n_frames);
    m_update_queue_by_frames_pending = n_frames;
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

std::vector<DummyMidiPort::StoredMessage> DummyMidiPort::get_written_requested_msgs() {
    auto rval = m_written_requested_msgs;
    m_written_requested_msgs.clear();
    return rval;
}

DummyMidiPort::~DummyMidiPort() { DummyPort::close(); }

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::enter_mode(DummyAudioSystemMode mode) {
    if (m_mode.load() != mode) {
        log<debug>("DummyAudioSystem: mode -> {}", mode_names.at(mode));
        m_mode = mode;
        m_controlled_mode_samples_to_process = 0;

        // Ensure we finish any processing we were doing
        wait_process();
    }
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::wait_process() {
    // To ensure a complete process cycle was done, execute two commands with
    // a small delay in-between. Each command will end up in a separate process
    // iteration.
    log<trace>("DummyAudioSystem: wait process");
    exec_process_thread_command([]() { ; });
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
    exec_process_thread_command([]() { ; });
    log<trace>("DummyAudioSystem: wait process done");
}

template <typename Time, typename Size>
DummyAudioSystemMode DummyAudioSystem<Time, Size>::get_mode() const {
    return m_mode;
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::controlled_mode_request_samples(uint32_t samples) {
    m_controlled_mode_samples_to_process += samples;
    uint32_t requested = m_controlled_mode_samples_to_process.load();
    log<debug>("DummyAudioSystem: request {} samples ({} total)", samples, requested);
}

template <typename Time, typename Size>
uint32_t DummyAudioSystem<Time, Size>::get_controlled_mode_samples_to_process() const {
    return m_controlled_mode_samples_to_process;
}


template <typename Time, typename Size>
DummyAudioSystem<Time, Size>::DummyAudioSystem(
    std::string client_name, std::function<void(uint32_t)> process_cb,
    DummyAudioSystemMode mode,
    uint32_t sample_rate,
    uint32_t buffer_size)
    : AudioSystemInterface(client_name, process_cb),
      WithCommandQueue<20, 1000, 1000>(),
      m_process_cb(process_cb), mc_buffer_size(buffer_size), mc_sample_rate(sample_rate),
      m_finish(false), m_client_name(client_name),
      m_audio_port_closed_cb(nullptr), m_audio_port_opened_cb(nullptr),
      m_midi_port_closed_cb(nullptr), m_midi_port_opened_cb(nullptr),
      m_controlled_mode_samples_to_process(0),
      m_mode(mode),
      m_paused(false) {
    m_audio_ports.clear();
    m_midi_ports.clear();

    log<debug>("DummyAudioSystem: constructed");
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::start() {
    m_proc_thread = std::thread([this] {
        // TODO: use fmt properly
        log<debug>("DummyAudioSystem: starting process thread - {}", mode_names.at(m_mode));
        auto bufs_per_second = mc_sample_rate / mc_buffer_size;
        auto interval = 1.0f / ((float)bufs_per_second);
        auto micros = uint32_t(interval * 1000000.0f);
        while (!this->m_finish) {
            std::this_thread::sleep_for(std::chrono::microseconds(micros));
            PROC_handle_command_queue();
            if (!m_paused) {
                auto mode = m_mode.load();
                auto samples_to_process = m_controlled_mode_samples_to_process.load();
                uint32_t to_process = mode == DummyAudioSystemMode::Controlled ?
                    std::min(samples_to_process, mc_buffer_size) :
                    mc_buffer_size;
                log<trace>("DummyAudioSystem: process {}", to_process);
                m_process_cb(to_process);
                if (mode == DummyAudioSystemMode::Controlled) {
                    m_controlled_mode_samples_to_process -= to_process;
                }
            }
        }
        log<debug>(
            "DummyAudioSystem: ending process thread");
    });
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::pause() {
    log<debug>("DummyAudioSystem: pause");
    m_paused = true;
    wait_process();
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::resume() {
    log<debug>("DummyAudioSystem: resume");
    m_paused = false;
}

template <typename Time, typename Size>
DummyAudioSystem<Time, Size>::~DummyAudioSystem() {
    close();
}

template <typename Time, typename Size>
std::shared_ptr<AudioPortInterface<audio_sample_t>>
DummyAudioSystem<Time, Size>::open_audio_port(std::string name,
                                              PortDirection direction) {
    log<debug>("DummyAudioSystem : add audio port");
    auto rval = std::make_shared<DummyAudioPort>(name, direction);
    m_audio_ports.insert(rval);
    return rval;
}

template <typename Time, typename Size>
std::shared_ptr<MidiPortInterface>
DummyAudioSystem<Time, Size>::open_midi_port(std::string name,
                                             PortDirection direction) {
    log<debug>("DummyAudioSystem: add midi port");
    auto rval = std::make_shared<DummyMidiPort>(name, direction);
    m_midi_ports.insert(rval);
    return rval;
}

template <typename Time, typename Size>
uint32_t DummyAudioSystem<Time, Size>::get_sample_rate() const {
    return mc_sample_rate;
}

template <typename Time, typename Size>
uint32_t DummyAudioSystem<Time, Size>::get_buffer_size() const {
    return mc_buffer_size;
}

template <typename Time, typename Size>
void *DummyAudioSystem<Time, Size>::maybe_client_handle() const {
    return (void *)this;
}

template <typename Time, typename Size>
const char *DummyAudioSystem<Time, Size>::client_name() const {
    return m_client_name.c_str();
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::close() {
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
uint32_t DummyAudioSystem<Time, Size>::get_xruns() const {
    return 0;
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::reset_xruns(){};

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::controlled_mode_run_request(uint32_t timeout) {
    log<debug>("DummyAudioSystem: run request");
    auto s = std::chrono::high_resolution_clock::now();
    auto timed_out = [this, &timeout, &s]() {
        return std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::high_resolution_clock::now() - s
        ).count() >= timeout;
    };
    while(
        m_mode == DummyAudioSystemMode::Controlled &&
        m_controlled_mode_samples_to_process > 0 &&
        !timed_out()
    ) {
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }

    wait_process();

    if (m_controlled_mode_samples_to_process > 0) {
        log<error>("DummyAudioSystem: run request timed out");
    }
}

template class DummyAudioSystem<uint32_t, uint16_t>;
template class DummyAudioSystem<uint32_t, uint32_t>;
template class DummyAudioSystem<uint16_t, uint16_t>;
template class DummyAudioSystem<uint16_t, uint32_t>;
template class DummyAudioSystem<uint32_t, uint64_t>;
template class DummyAudioSystem<uint64_t, uint64_t>;