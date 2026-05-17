#pragma once
#include <stdint.h>
#include "PortInterface.h"
#include "IAudioReadableBuffer.h"
#include "IAudioWriteableBuffer.h"
#include "IAudioStateTracking.h"
#include "IAudioRingbuffer.h"
#include <atomic>
#include "BufferQueue.h"
#include "BufferPool.h"
#include "shoop_shared_ptr.h"
#include "shoop_globals.h"

// Forward declaration
class AudioPortBase;

template<typename SampleT>
class AudioPort : public virtual PortInterface,
                  public virtual IAudioReadableBuffer,
                  public virtual IAudioWriteableBuffer,
                  public virtual IAudioStateTracking,
                  public virtual IAudioRingbuffer<SampleT> {
    std::atomic<float> ma_input_peak = 0.0f;
    std::atomic<float> ma_output_peak = 0.0f;
    std::atomic<float> ma_gain = 1.0f;
    std::atomic<bool> ma_muted = false;

    // Always-active FIFO queue acting as a ring buffer of sorts: data coming
    // in is always recorded until dropped from the queue.
    // Can be used for retroactive recording.
    BufferQueue<SampleT> mp_always_record_ringbuffer;

public:
    using RingbufferSnapshot = typename BufferQueue<SampleT>::Snapshot;
    using UsedBufferPool = BufferPool<SampleT>;

    AudioPort(shoop_shared_ptr<UsedBufferPool> maybe_ringbuffer_buffer_pool);
    virtual ~AudioPort();

    virtual SampleT* PROC_get_buffer(uint32_t n_frames) = 0;

    PortDataType type() const override;

    void PROC_process(uint32_t nframes) override;

    // IAudioStateTracking implementation
    IAudioStateTracking* get_state_tracking() override { return this; }
    float get_input_peak() const override;
    void reset_input_peak() override;
    float get_output_peak() const override;
    void reset_output_peak() override;
    float get_gain() const override;
    void set_gain(float gain) override;
    bool get_muted() const override;
    void set_muted(bool muted) override;

    // IAudioRingbuffer implementation
    IAudioRingbuffer<SampleT>* get_ringbuffer() override { return this; }
    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
    typename BufferQueue<SampleT>::Snapshot get_ringbuffer_contents() override;

    // IAudioReadableBuffer implementation
    IAudioReadableBuffer* get_readable_buffer() override { return this; }
    audio_sample_t* get_read_ptr() override;
    uint32_t n_samples() const override;
    void get_peak(float& in_peak, float& out_peak) override;

    // IAudioWriteableBuffer implementation
    IAudioWriteableBuffer* get_writeable_buffer() override { return this; }
    audio_sample_t* get_write_ptr() override;
    uint32_t capacity() const override;
};

#ifndef IMPLEMENT_AUDIOPORT_H
extern template class AudioPort<float>;
extern template class AudioPort<int>;
#endif