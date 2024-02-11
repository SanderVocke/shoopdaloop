#pragma once
#include <string>
#include <stdint.h>
#include "PortInterface.h"
#include <atomic>
#include <cstring>
#include <stdexcept>

template<typename SampleT>
class AudioPort : public virtual PortInterface {
    std::atomic<float> ma_input_peak = 0.0f;
    std::atomic<float> ma_output_peak = 0.0f;
    std::atomic<float> ma_gain = 1.0f;
    std::atomic<bool> ma_muted = false;

public:
    AudioPort() : PortInterface(),
      ma_muted(false),
      ma_gain(1.0f),
      ma_input_peak(0.0f),
      ma_output_peak(0.0f) {}
    virtual ~AudioPort() {}

    virtual SampleT *PROC_get_buffer(uint32_t n_frames) = 0;

    PortDataType type() const override { return PortDataType::Audio; }

    void PROC_process(uint32_t nframes) override {
        auto buf = PROC_get_buffer(nframes);
        auto muted = ma_muted.load();
        
        if (!buf) {
            throw std::runtime_error("PROC_get_buffer returned nullptr");
        }

        // Process input peak and buffer
        SampleT input_peak = ma_input_peak.load();
        auto gain = ma_gain.load();
        for(size_t i=0; i<nframes; i++) {
            input_peak = std::max(input_peak, std::abs(buf[i]));
            if (muted) {
                buf[i] = 0.0f;
            } else {
                buf[i] *= gain;
            } 
        }
        ma_input_peak = input_peak;
        ma_output_peak = std::max(
            ma_output_peak.load(),
            muted ?
                0.0f : input_peak * gain
        );
    }

    void set_gain(float gain) { ma_gain = gain; }
    float get_gain() const { return ma_gain.load(); }

    void set_muted(bool muted) override { ma_muted = muted; }
    bool get_muted() const override { return ma_muted; }

    float get_input_peak() const { return ma_input_peak.load(); }
    void reset_input_peak() { ma_input_peak = 0.0f; }
    float get_output_peak() const { return ma_output_peak.load(); }
    void reset_output_peak() { ma_output_peak = 0.0f; }
};
