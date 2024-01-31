#include "DecoupledMidiPort.h"
#include "AudioMidiDriver.h"
#include "PortInterface.h"
#include "types.h"
#include <optional>
#include <cstring>
#include <iostream>

template <typename TimeType, typename SizeType>
DecoupledMidiPort<TimeType, SizeType>::DecoupledMidiPort(
    std::shared_ptr<MidiPort> port,
    std::weak_ptr<AudioMidiDriver> driver,
    uint32_t queue_size,
    shoop_port_direction_t direction)
    : port(port), ma_queue(queue_size), direction(direction), maybe_driver(driver) {};

template <typename TimeType, typename SizeType>
std::shared_ptr<MidiPort> const& DecoupledMidiPort<TimeType, SizeType>::get_port() {
    return port;
}

template <typename TimeType, typename SizeType>
void DecoupledMidiPort<TimeType, SizeType>::PROC_process(uint32_t n_frames) {
    if (direction == shoop_port_direction_t::Input) {
        port->PROC_prepare(n_frames);
        port->PROC_process(n_frames);
        auto buf = port->PROC_get_read_output_data_buffer(n_frames);
        auto n = buf->PROC_get_n_events();
        if (n>0) {
            log<log_level_debug>("Got {} MIDI events", n);
        }
        for (uint32_t idx = 0; idx < n; idx++) {
            Message m;
            const uint8_t *data;
            uint32_t time;
            uint32_t size;
            buf->PROC_get_event_reference(idx).get(size, time, data);
            m.data = std::vector<uint8_t>(size);
            memcpy((void *)m.data.data(), (void *)data, size);
            ma_queue.push(m);
        }
    } else {
        port->PROC_prepare(n_frames);
        auto buf = port->PROC_get_write_data_into_port_buffer(n_frames);
        Message m;
        while (ma_queue.pop(m)) {
            buf->PROC_write_event_value(m.data.size(), 0, m.data.data());
        }
        port->PROC_process(n_frames);
    }
}

template <typename TimeType, typename SizeType>
const char* DecoupledMidiPort<TimeType, SizeType>::name() const {
    return port->name();
}

template <typename TimeType, typename SizeType>
std::optional<typename DecoupledMidiPort<TimeType, SizeType>::Message>
DecoupledMidiPort<TimeType, SizeType>::pop_incoming() {
    if (direction != shoop_port_direction_t::Input) {
        throw std::runtime_error(
            "Attempt to pop input message from output port");
    }
    Message m;
    if (ma_queue.pop(m)) {
        return m;
    }
    return std::nullopt;
}

template <typename TimeType, typename SizeType>
void DecoupledMidiPort<TimeType, SizeType>::push_outgoing(
    typename DecoupledMidiPort<TimeType, SizeType>::Message m) {
    ma_queue.push(m);
}

template <typename TimeType, typename SizeType>
void DecoupledMidiPort<TimeType, SizeType>::close() {
    port->close();
}

template <typename TimeType, typename SizeType>
std::shared_ptr<AudioMidiDriver> DecoupledMidiPort<TimeType, SizeType>::
    get_maybe_driver() const {
    return maybe_driver.lock();
}

template <typename TimeType, typename SizeType>
void DecoupledMidiPort<TimeType, SizeType>::forget_driver() {
    maybe_driver.reset();
}

template class DecoupledMidiPort<uint32_t, uint16_t>;
template class DecoupledMidiPort<uint32_t, uint32_t>;
template class DecoupledMidiPort<uint16_t, uint16_t>;
template class DecoupledMidiPort<uint16_t, uint32_t>;