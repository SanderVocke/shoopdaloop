#include "AudioPortBase.h"

#include <cstring>
#include <stdexcept>
#include <stdint.h>
#include "PortInterface.h"
#include <cmath>

#ifdef _WIN32
#undef min
#undef max
#endif

AudioPortBase::AudioPortBase(shoop_shared_ptr<UsedBufferPool> buffer_pool)
    : PortInterface(),
      ma_muted(false),
      ma_gain(1.0f),
      ma_input_peak(0.0f),
      ma_output_peak(0.0f),
      mp_always_record_ringbuffer(buffer_pool, buffer_pool ? 32 : 0)
{
}

AudioPortBase::~AudioPortBase() {}

PortDataType AudioPortBase::type() const { return PortDataType::Audio; }

void AudioPortBase::process_samples(audio_sample_t* buf, uint32_t nframes) {
    if (!buf) {
        throw std::runtime_error("process_samples received nullptr buffer");
    }

    auto muted = ma_muted.load();
    audio_sample_t input_peak = ma_input_peak.load();
    auto gain = ma_gain.load();

    for(size_t i = 0; i < nframes; i++) {
        input_peak = std::max(input_peak, std::abs(buf[i]));
        if (muted) {
            buf[i] = 0.0f;
        } else {
            buf[i] = static_cast<audio_sample_t>(buf[i] * gain);
        }
    }

    ma_input_peak = input_peak;
    ma_output_peak = std::max(
        ma_output_peak.load(),
        muted ? audio_sample_t(0.0f) : audio_sample_t(input_peak * gain)
    );

    // Process ringbuffer
    if (mp_always_record_ringbuffer.single_buffer_size() > 0) {
        mp_always_record_ringbuffer.PROC_put(buf, nframes);
    }
}

// IAudioStateTracking implementation
float AudioPortBase::get_input_peak() const { return ma_input_peak.load(); }

void AudioPortBase::reset_input_peak() { ma_input_peak = 0.0f; }

float AudioPortBase::get_output_peak() const { return ma_output_peak.load(); }

void AudioPortBase::reset_output_peak() { ma_output_peak = 0.0f; }

float AudioPortBase::get_gain() const { return ma_gain.load(); }

void AudioPortBase::set_gain(float gain) { ma_gain = gain; }

bool AudioPortBase::get_muted() const { return ma_muted.load(); }

void AudioPortBase::set_muted(bool muted) { ma_muted = muted; }

// IAudioRingbuffer implementation
void AudioPortBase::set_ringbuffer_n_samples(unsigned n) {
    mp_always_record_ringbuffer.set_min_n_samples(n);
}

unsigned AudioPortBase::get_ringbuffer_n_samples() const {
    return mp_always_record_ringbuffer.n_samples();
}

AudioPortBase::RingbufferSnapshot AudioPortBase::get_ringbuffer_contents() {
    return mp_always_record_ringbuffer.PROC_get();
}