#pragma once
#include <string>
#include "AudioPortInterface.h"
#include "PortInterface.h"
#include <stdexcept>
#include <cstring>
#include <iostream>
#include <vector>

template<typename SampleT>
class InternalAudioPort : public AudioPortInterface<SampleT> {
    std::string m_name;
    PortDirection m_direction;
    std::vector<SampleT> m_buffer;

public:
    // In ShoopDaLoop, complex internal processing graphs are not supported.
    // Internal ports have a direction, which means:
    // - input ports are meant to receive data from external sources
    // - output ports are meant to provide data for outputting to external sinks.
    // This helps guide the assumptions about which external ports to connect them
    // to internally.
    InternalAudioPort(
        std::string name,
        PortDirection direction,
        size_t n_frames
    ) : AudioPortInterface<SampleT>(name, direction),
        m_name(name),
        m_direction(direction),
        m_buffer(n_frames) {}
    
    float *PROC_get_buffer(size_t n_frames) override {
        if(n_frames > m_buffer.size()) {
            throw std::runtime_error("Requesting oversized buffer from internal port");
        }
        return m_buffer.data();
    }

    const char* name() const override {
        return m_name.c_str();
    }

    PortDirection direction() const override {
        return m_direction;
    }

    void reallocate_buffer(size_t n_frames) {
        m_buffer = std::vector<SampleT>(n_frames);
    }

    size_t buffer_size() const { return m_buffer.size(); }

    void close() override {}
};