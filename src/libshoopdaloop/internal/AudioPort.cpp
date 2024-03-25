#include "AudioPort.h"

#include <cstring>
#include <stdexcept>
#include <stdint.h>
#include "PortInterface.h"
#include <atomic>
#include <cstring>
#include <stdexcept>
#include "BufferQueue.h"

template<typename SampleT>
AudioPort<SampleT>::AudioPort(std::shared_ptr<BufferPool> buffer_pool)
    : PortInterface(),
      ma_muted(false),
      ma_gain(1.0f),
      ma_input_peak(0.0f),
      ma_output_peak(0.0f),
      mp_always_record_ringbuffer(buffer_pool, buffer_pool ? 32 : 0) {}

template<typename SampleT>
AudioPort<SampleT>::~AudioPort() {}

template<typename SampleT>
PortDataType AudioPort<SampleT>::type() const { return PortDataType::Audio; }

template<typename SampleT>
void AudioPort<SampleT>::PROC_process(uint32_t nframes) {
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

    // Process ringbuffer
    if (mp_always_record_ringbuffer.n_samples() > 0) {
        mp_always_record_ringbuffer.PROC_put(buf, nframes);
    }
}

template<typename SampleT>
void AudioPort<SampleT>::set_gain(float gain) { ma_gain = gain; }

template<typename SampleT>
float AudioPort<SampleT>::get_gain() const { return ma_gain.load(); }

template<typename SampleT>
void AudioPort<SampleT>::set_muted(bool muted) { ma_muted = muted; }

template<typename SampleT>
bool AudioPort<SampleT>::get_muted() const { return ma_muted; }

template<typename SampleT>
float AudioPort<SampleT>::get_input_peak() const { return ma_input_peak.load(); }

template<typename SampleT>
void AudioPort<SampleT>::reset_input_peak() { ma_input_peak = 0.0f; }

template<typename SampleT>
float AudioPort<SampleT>::get_output_peak() const { return ma_output_peak.load(); }

template<typename SampleT>
void AudioPort<SampleT>::reset_output_peak() { ma_output_peak = 0.0f; }

template<typename SampleT>
void AudioPort<SampleT>::set_ringbuffer_n_samples(unsigned n) {
    mp_always_record_ringbuffer.set_min_n_samples(n);
}

template<typename SampleT>
unsigned AudioPort<SampleT>::get_ringbuffer_n_samples() const {
    return mp_always_record_ringbuffer.n_samples();
}

template<typename SampleT>
typename BufferQueue<SampleT>::Snapshot AudioPort<SampleT>::PROC_get_ringbuffer_contents() {
    return mp_always_record_ringbuffer.PROC_get();
}

template class AudioPort<float>;
template class AudioPort<int>;