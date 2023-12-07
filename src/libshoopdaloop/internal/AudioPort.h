#pragma once
#include <string>
#include <stdint.h>
#include "PortInterface.h"
#include <atomic>
#include <cstring>

template<typename SampleT>
class AudioPort : public virtual PortInterface {
    std::atomic<float> ma_peak;
    std::atomic<float> ma_gain;
    std::atomic<bool> ma_muted;

public:
    AudioPort() : PortInterface() {}
    virtual ~AudioPort() {}

    virtual SampleT *PROC_get_buffer(uint32_t n_frames) = 0;

    PortDataType type() const override { return PortDataType::Audio; }

    void PROC_process(uint32_t nframes) override {
        auto buf = PROC_get_buffer(nframes);
        auto muted = ma_muted.load();
        
        if (muted) {
            memset((void*)buf, 0, nframes * sizeof(SampleT));
        } else {
            auto gain = ma_gain.load();
            SampleT peak = 0.0f;
            for(size_t i=0; i<nframes; i++) {
                buf[i] *= gain;
                peak = std::max(peak, std::abs(buf[i]));
            }
            ma_peak = peak;
        }
    }

    void set_gain(float gain) { ma_gain = gain; }
    float get_gain() const { return ma_gain.load(); }

    void set_muted(bool muted) override { ma_muted = muted; }
    bool get_muted() const override { return ma_muted; }
};
