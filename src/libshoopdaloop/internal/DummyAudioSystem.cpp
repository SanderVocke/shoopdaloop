#include "types.h"
#define IMPLEMENT_DUMMYAUDIOSYSTEM_H
#include "DummyAudioSystem.h"
#include <map>

template class DummyAudioSystem<uint32_t, uint16_t>;
template class DummyAudioSystem<uint32_t, uint32_t>;
template class DummyAudioSystem<uint16_t, uint16_t>;
template class DummyAudioSystem<uint16_t, uint32_t>;
template class DummyAudioSystem<uint32_t, uint64_t>;

const std::map<DummyAudioSystemMode, const char*> mode_names = {
    {DummyAudioSystemMode::Automatic, "Automatic"},
    {DummyAudioSystemMode::Controlled, "Controlled"}
};

DummyAudioPort::DummyAudioPort(std::string name, PortDirection direction)
    : AudioPortInterface<audio_sample_t>(name, direction), m_name(name),
      m_direction(direction),
      m_queued_data(256) {}

float *DummyAudioPort::PROC_get_buffer(size_t n_frames) {
    auto rval = (audio_sample_t *)malloc(n_frames * sizeof(audio_sample_t));
    size_t filled = 0;
    while (!m_queued_data.empty() && filled < n_frames) {
        auto &front = m_queued_data.front();
        size_t to_copy = std::min(n_frames - filled, front.size());
        memcpy((void *)(rval + filled), (void *)front.data(),
               sizeof(audio_sample_t) * to_copy);
        filled += to_copy;
        if (front.size() == 0) {
            m_queued_data.pop();
        } else {
            front.erase(front.begin(), front.begin() + to_copy);
        }
    }
    memset((void *)(rval+filled), 0, sizeof(audio_sample_t) * (n_frames - filled));
    return rval;
}

const char *DummyAudioPort::name() const { return m_name.c_str(); }

PortDirection DummyAudioPort::direction() const { return m_direction; }

void DummyAudioPort::close() {}

void DummyAudioPort::queue_data(size_t n_frames, audio_sample_t const *data) {
    m_queued_data.push(
        std::vector<audio_sample_t>(data, data + n_frames)
    );
}

bool DummyAudioPort::get_queue_empty() {
    return m_queued_data.empty();
}

DummyAudioPort::~DummyAudioPort() { close(); }

size_t DummyMidiPort::PROC_get_n_events() const { return 0; }

MidiSortableMessageInterface &
DummyMidiPort::PROC_get_event_reference(size_t idx) {
    throw std::runtime_error("Dummy midi port cannot read messages");
}

void DummyMidiPort::PROC_write_event_value(uint32_t size, uint32_t time,
                                           const uint8_t *data) {}

void DummyMidiPort::PROC_write_event_reference(
    MidiSortableMessageInterface const &m) {}

bool DummyMidiPort::write_by_reference_supported() const { return true; }

bool DummyMidiPort::write_by_value_supported() const { return true; }

DummyMidiPort::DummyMidiPort(std::string name, PortDirection direction)
    : MidiPortInterface(name, direction), m_direction(direction), m_name(name) {
}

const char *DummyMidiPort::name() const { return m_name.c_str(); }

PortDirection DummyMidiPort::direction() const { return m_direction; }

void DummyMidiPort::close() {}

MidiReadableBufferInterface &
DummyMidiPort::PROC_get_read_buffer(size_t n_frames) {
    return *(static_cast<MidiReadableBufferInterface *>(this));
}

MidiWriteableBufferInterface &
DummyMidiPort::PROC_get_write_buffer(size_t n_frames) {
    return *(static_cast<MidiWriteableBufferInterface *>(this));
}

DummyMidiPort::~DummyMidiPort() { close(); }

template <typename Time, typename Size>
std::string DummyAudioSystem<Time, Size>::log_module_name() const {
    return "Backend.DummyAudioSystem";
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::enter_mode(DummyAudioSystemMode mode) {
    if (m_mode.load() != mode) {
        log<logging::LogLevel::debug>(fmt::runtime(
            std::string("DummyAudioSystem: mode -> ") +
            std::string(mode_names.at(mode))
        ));
        m_mode = mode;
        m_controlled_mode_samples_to_process = 0;
    }
}

template <typename Time, typename Size>
DummyAudioSystemMode DummyAudioSystem<Time, Size>::get_mode() const {
    return m_mode;
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::controlled_mode_request_samples(size_t samples) {
    m_controlled_mode_samples_to_process += samples;
}

template <typename Time, typename Size>
size_t DummyAudioSystem<Time, Size>::get_controlled_mode_samples_to_process() const {
    return m_controlled_mode_samples_to_process;
}


template <typename Time, typename Size>
DummyAudioSystem<Time, Size>::DummyAudioSystem(
    std::string client_name, std::function<void(size_t)> process_cb,
    DummyAudioSystemMode mode,
    size_t sample_rate,
    size_t buffer_size)
    : AudioSystemInterface<Time, Size>(client_name, process_cb),
      m_process_cb(process_cb), mc_buffer_size(buffer_size), mc_sample_rate(sample_rate),
      m_finish(false), m_client_name(client_name),
      m_audio_port_closed_cb(nullptr), m_audio_port_opened_cb(nullptr),
      m_midi_port_closed_cb(nullptr), m_midi_port_opened_cb(nullptr),
      m_controlled_mode_samples_to_process(0),
      m_mode(mode),
      m_paused(false),
      m_post_process_cb(nullptr) {
    m_audio_ports.clear();
    m_midi_ports.clear();
    log_init();

    log<logging::LogLevel::debug>("DummyAudioSystem: constructed");
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::start() {
    m_proc_thread = std::thread([this] {
        // TODO: use fmt properly
        log<logging::LogLevel::debug>(fmt::runtime(
            std::string("DummyAudioSystem: starting process thread - ") +
            std::string(mode_names.at(m_mode))
        ));
        auto bufs_per_second = mc_sample_rate / mc_buffer_size;
        auto interval = 1.0f / ((float)bufs_per_second);
        auto micros = size_t(interval * 1000000.0f);
        while (!this->m_finish) {
            std::this_thread::sleep_for(std::chrono::microseconds(micros));
            if (!m_paused) {
                auto mode = m_mode.load();
                size_t to_process = mode == DummyAudioSystemMode::Controlled ?
                    std::min(m_controlled_mode_samples_to_process.load(), mc_buffer_size) :
                    mc_buffer_size;
                log<logging::LogLevel::trace>("DummyAudioSystem: process {}", to_process);
                m_process_cb(to_process);
                if (m_post_process_cb) {
                    m_post_process_cb(to_process);
                }
                if (mode == DummyAudioSystemMode::Controlled) {
                    m_controlled_mode_samples_to_process -= to_process;
                }
            }
        }
        log<logging::LogLevel::debug>(
            "DummyAudioSystem: ending process thread");
    });
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::pause() {
    m_paused = true;
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::resume() {
    m_paused = false;
}

template <typename Time, typename Size>
DummyAudioSystem<Time, Size>::~DummyAudioSystem() {
    close();
    log<logging::LogLevel::debug>("DummyAudioSystem: destructed");
}

template <typename Time, typename Size>
std::shared_ptr<AudioPortInterface<audio_sample_t>>
DummyAudioSystem<Time, Size>::open_audio_port(std::string name,
                                              PortDirection direction) {
    log<logging::LogLevel::debug>("DummyAudioSystem : add audio port");
    auto rval = std::make_shared<DummyAudioPort>(name, direction);
    m_audio_ports.insert(rval);
    return rval;
}

template <typename Time, typename Size>
std::shared_ptr<MidiPortInterface>
DummyAudioSystem<Time, Size>::open_midi_port(std::string name,
                                             PortDirection direction) {
    log<logging::LogLevel::debug>("DummyAudioSystem: add midi port");
    auto rval = std::make_shared<DummyMidiPort>(name, direction);
    m_midi_ports.insert(rval);
    return rval;
}

template <typename Time, typename Size>
size_t DummyAudioSystem<Time, Size>::get_sample_rate() const {
    return mc_sample_rate;
}

template <typename Time, typename Size>
size_t DummyAudioSystem<Time, Size>::get_buffer_size() const {
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
    log<logging::LogLevel::debug>("DummyAudioSystem: closing");
    if (m_proc_thread.joinable()) {
        log<logging::LogLevel::debug>(
            "DummyAudioSystem: joining process thread");
        m_proc_thread.join();
    }
}

template <typename Time, typename Size>
size_t DummyAudioSystem<Time, Size>::get_xruns() const {
    return 0;
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::reset_xruns(){};

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::install_post_process_handler(std::function<void (size_t)> cb) {
    m_post_process_cb = cb;
}

