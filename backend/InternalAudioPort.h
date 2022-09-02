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
    // Note that the port direction for internal ports are defined w.r.t. ShoopDaLoop.
    // That is to say, the inputs of a plugin effect are regarded as output ports to
    // ShoopDaLoop.
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
        if (m_direction == PortDirection::Output) {
            memset((void*)m_buffer.data(), 0, sizeof(float) * n_frames);
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

    void zero() {
        memset((void*)m_buffer.data(), 0, sizeof(float) * m_buffer.size());
    }
};