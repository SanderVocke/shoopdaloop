#define IMPLEMENT_DUMMYAUDIOSYSTEM_H
#include "DummyAudioSystem.h"
template class DummyAudioSystem<uint32_t, uint16_t>;
template class DummyAudioSystem<uint32_t, uint32_t>;
template class DummyAudioSystem<uint16_t, uint16_t>;
template class DummyAudioSystem<uint16_t, uint32_t>;
template class DummyAudioSystem<uint32_t, uint64_t>;

DummyAudioPort::DummyAudioPort(std::string name, PortDirection direction)
    : AudioPortInterface<audio_sample_t>(name, direction), m_name(name),
      m_direction(direction) {}

float *DummyAudioPort::PROC_get_buffer(size_t n_frames) {
    auto rval = (audio_sample_t *)malloc(n_frames * sizeof(audio_sample_t));
    memset((void *)rval, 0, sizeof(audio_sample_t) * n_frames);
    return rval;
}

const char *DummyAudioPort::name() const { return m_name.c_str(); }

PortDirection DummyAudioPort::direction() const { return m_direction; }

void DummyAudioPort::close() {}

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
DummyAudioSystem<Time, Size>::DummyAudioSystem(
    std::string client_name, std::function<void(size_t)> process_cb)
    : AudioSystemInterface<Time, Size>(client_name, process_cb),
      m_process_cb(process_cb), mc_buffer_size(256), mc_sample_rate(48000),
      m_finish(false), m_client_name(client_name),
      m_audio_port_closed_cb(nullptr), m_audio_port_opened_cb(nullptr),
      m_midi_port_closed_cb(nullptr), m_midi_port_opened_cb(nullptr) {
    m_audio_ports.clear();
    m_midi_ports.clear();
    log_init();

    log<logging::LogLevel::debug>("DummyAudioSystem: constructed");
}

template <typename Time, typename Size>
void DummyAudioSystem<Time, Size>::start() {
    m_proc_thread = std::thread([this] {
        log<logging::LogLevel::debug>(
            "DummyAudioSystem: starting process thread");
        auto bufs_per_second = mc_sample_rate / mc_buffer_size;
        auto interval = 1.0f / ((float)bufs_per_second);
        auto micros = size_t(interval * 1000000.0f);
        while (!this->m_finish) {
            log<logging::LogLevel::trace>(
                "DummyAudioSystem: process iteration");
            std::this_thread::sleep_for(std::chrono::microseconds(micros));
            m_process_cb(mc_buffer_size);
        }
        log<logging::LogLevel::debug>(
            "DummyAudioSystem: ending process thread");
    });
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
void DummyAudioSystem<Time, Size>::reset_xruns(){

};