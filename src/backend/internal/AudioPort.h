#pragma once
#include <stdint.h>
#include "PortInterface.h"
#include <atomic>
#include "BufferQueue.h"
#include "BufferPool.h"
#include "shoop_shared_ptr.h"
#include "TracyPlotter.h"

template<typename SampleT>
class AudioPort : public virtual PortInterface {
    std::atomic<float> ma_input_peak = 0.0f;
    std::atomic<float> ma_output_peak = 0.0f;
    std::atomic<float> ma_gain = 1.0f;
    std::atomic<bool> ma_muted = false;

    // Always-active FIFO queue acting as a ring buffer of sorts: data coming
    // in is always recorded until dropped from the queue.
    // Can be used for retroactive recording.
    BufferQueue<SampleT> mp_always_record_ringbuffer;

    // Tracy plotters for audio port debugging
    TracyPlotter m_plot_input_peak{"AudioPort/input_peak"};
    TracyPlotter m_plot_output_peak{"AudioPort/output_peak"};
    TracyPlotter m_plot_frames_processed{"AudioPort/frames_processed"};
    TracyPlotter m_plot_muted{"AudioPort/muted"};
    TracyPlotter m_plot_gain{"AudioPort/gain"};

public:
    using RingbufferSnapshot = typename BufferQueue<SampleT>::Snapshot;
    using UsedBufferPool = BufferPool<SampleT>;

    AudioPort(shoop_shared_ptr<UsedBufferPool> maybe_ringbuffer_buffer_pool);
    explicit AudioPort(shoop_shared_ptr<UsedBufferPool> maybe_ringbuffer_buffer_pool,
                       const char* plot_prefix);
    virtual ~AudioPort();

    virtual SampleT *PROC_get_buffer(uint32_t n_frames) = 0;

    PortDataType type() const override;

    void PROC_process(uint32_t nframes) override;

    void set_gain(float gain);
    float get_gain() const;

    void set_muted(bool muted) override;
    bool get_muted() const override;

    float get_input_peak() const;
    void reset_input_peak();
    float get_output_peak() const;
    void reset_output_peak();

    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
    
    typename BufferQueue<SampleT>::Snapshot PROC_get_ringbuffer_contents();
};

#ifndef IMPLEMENT_AUDIOPORT_H
extern template class AudioPort<float>;
extern template class AudioPort<int>;
#endif