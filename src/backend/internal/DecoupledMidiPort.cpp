#include "DecoupledMidiPort.h"
#include "AudioMidiDriver.h"
#include "PortInterface.h"
#include "types.h"
#include <optional>
#include <cstring>
#include <iostream>
#include <array>

DecoupledMidiPort::DecoupledMidiPort(
    std::shared_ptr<MidiPort> port,
    std::weak_ptr<AudioMidiDriver> driver,
    uint32_t queue_size,
    shoop_port_direction_t direction)
    : port(port), direction(direction), m_rust(backend_rust::new_decoupled_midi_port(queue_size, static_cast<uint32_t>(direction))), maybe_driver(driver) {};

std::shared_ptr<MidiPort> const& DecoupledMidiPort::get_port() {
    return port;
}

void DecoupledMidiPort::PROC_process(uint32_t n_frames) {
    if (direction == shoop_port_direction_t::ShoopPortDirection_Input) {
        port->PROC_prepare(n_frames);
        port->PROC_process(n_frames);
        auto buf = port->PROC_get_read_output_data_buffer(n_frames);
        auto n = buf->n_events();
        if (n>0) {
            log<log_level_debug>("Got {} MIDI events", n);
        }
        for (uint32_t idx = 0; idx < n; idx++) {
            Message m = buf->get_event(idx);
            std::array<uint8_t, 4> data = {m.bytes[0], m.bytes[1], m.bytes[2], m.bytes[3]};
            backend_rust::decoupled_push(*m_rust, m.time, m.size, data);
        }
    } else if (direction == shoop_port_direction_t::ShoopPortDirection_Output) {
        port->PROC_prepare(n_frames);
        auto buf = port->PROC_get_write_data_into_port_buffer(n_frames);
        uint32_t time = 0;
        uint16_t size = 0;
        Message m;
        while (backend_rust::decoupled_pop(*m_rust, time, size, m.bytes)) {
            m.time = time;
            m.size = size;
            buf->write_event(m);
        }
        port->PROC_process(n_frames);
    }
}

const char* DecoupledMidiPort::name() const {
    return static_cast<PortInterface*>(port.get())->name();
}

std::optional<DecoupledMidiPort::Message>
DecoupledMidiPort::pop_incoming() {
    if (direction != shoop_port_direction_t::ShoopPortDirection_Input) {
        throw std::runtime_error(
            "Attempt to pop input message from output port");
    }
    uint32_t time = 0;
    uint16_t size = 0;
    Message m;
    if (backend_rust::decoupled_pop(*m_rust, time, size, m.bytes)) {
        m.time = time;
        m.size = size;
        return m;
    }
    return std::nullopt;
}

void DecoupledMidiPort::push_outgoing(
    DecoupledMidiPort::Message m) {
    std::array<uint8_t, 4> data = {m.bytes[0], m.bytes[1], m.bytes[2], m.bytes[3]};
    backend_rust::decoupled_push(*m_rust, m.time, m.size, data);
}

void DecoupledMidiPort::close() {
    static_cast<PortInterface*>(port.get())->close();
}

std::shared_ptr<AudioMidiDriver> DecoupledMidiPort::
    get_maybe_driver() const {
    return maybe_driver.lock();
}

void DecoupledMidiPort::forget_driver() {
    maybe_driver.reset();
}

void DecoupledMidiPort::set_registry_handle(uint64_t handle) {
    m_registry_handle = handle;
}

uint64_t DecoupledMidiPort::registry_handle() const {
    return m_registry_handle;
}
