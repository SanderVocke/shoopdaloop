#include "DecoupledMidiPort.h"
#include "AudioMidiDriver.h"
#include "PortInterface.h"
#include "types.h"
#include "TracyPlotter.h"
#include "Checksum.h"
#include <optional>
#include <cstring>
#include <iostream>

template <typename TimeType, typename SizeType>
DecoupledMidiPort<TimeType, SizeType>::DecoupledMidiPort(
    shoop_shared_ptr<MidiPort> port,
    shoop_weak_ptr<AudioMidiDriver> driver,
    uint32_t queue_size,
    shoop_port_direction_t direction)
    : port(port),
      ma_queue(queue_size),
      direction(direction),
      maybe_driver(driver),
      m_plot_incoming_queue_size("incoming_queue_size"),
      m_plot_outgoing_queue_size("outgoing_queue_size"),
      m_plot_input_checksum("input_checksum"),
      m_plot_output_checksum("output_checksum") {};

template <typename TimeType, typename SizeType>
shoop_shared_ptr<MidiPort> const& DecoupledMidiPort<TimeType, SizeType>::get_port() {
    return port;
}

template <typename TimeType, typename SizeType>
void DecoupledMidiPort<TimeType, SizeType>::PROC_process(uint32_t n_frames) {
    if (direction == shoop_port_direction_t::ShoopPortDirection_Input) {
        port->PROC_prepare(n_frames);
        port->PROC_process(n_frames);
        auto buf = port->PROC_get_read_output_data_buffer(n_frames);
        auto n = buf->n_events();
        if (n>0) {
            log<log_level_debug>("Got {} MIDI events", n);
        }

        // Compute input checksum from received messages
        double input_checksum = 0.0;
        for (uint32_t idx = 0; idx < n; idx++) {
            Message m = buf->get_event(idx);
            ma_queue.push(m);
            input_checksum += checksum::compute_midi_message_checksum(m.time, m.size, m.data());
        }
        ma_input_checksum = input_checksum;
        const char* port_name = port->name();
        m_plot_incoming_queue_size.plot(static_cast<double>(ma_queue.read_available()), port_name);
        m_plot_input_checksum.plot(input_checksum, port_name);
    } else if (direction == shoop_port_direction_t::ShoopPortDirection_Output) {
        port->PROC_prepare(n_frames);
        auto buf = port->PROC_get_write_data_into_port_buffer(n_frames);

        // Compute output checksum from outgoing messages
        double output_checksum = 0.0;
        Message m;
        while (ma_queue.pop(m)) {
            buf->write_event(m);
            output_checksum += checksum::compute_midi_message_checksum(m.time, m.size, m.data());
        }
        ma_output_checksum = output_checksum;
        const char* port_name = port->name();
        port->PROC_process(n_frames);
        m_plot_outgoing_queue_size.plot(static_cast<double>(ma_queue.read_available()), port_name);
        m_plot_output_checksum.plot(output_checksum, port_name);
    }
}

template <typename TimeType, typename SizeType>
const char* DecoupledMidiPort<TimeType, SizeType>::name() const {
    return port->name();
}

template <typename TimeType, typename SizeType>
std::optional<typename DecoupledMidiPort<TimeType, SizeType>::Message>
DecoupledMidiPort<TimeType, SizeType>::pop_incoming() {
    if (direction != shoop_port_direction_t::ShoopPortDirection_Input) {
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
shoop_shared_ptr<AudioMidiDriver> DecoupledMidiPort<TimeType, SizeType>::
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