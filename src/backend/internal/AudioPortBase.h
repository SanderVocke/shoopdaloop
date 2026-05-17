#pragma once
#include <stdint.h>
#include <atomic>
#include "PortInterface.h"
#include "IAudioStateTracking.h"
#include "IAudioRingbuffer.h"
#include "BufferQueue.h"
#include "BufferPool.h"
#include "shoop_shared_ptr.h"
#include "shoop_globals.h"

/**
 * AudioPortBase - Non-template base class for audio ports with float samples.
 * 
 * This class contains the core audio port logic for the primary sample type
 * (audio_sample_t = float). It holds atomic state for peak tracking, gain,
 * and mute status, as well as the always-record ringbuffer.
 * 
 * This matches the MIDI port pattern where MidiPortBase provides the concrete
 * implementation that MidiPort templates delegate to.
 * 
 * Note: For float samples only - if int support is ever needed, create a
 * separate AudioPortBaseInt class.
 */
class AudioPortBase : public virtual PortInterface,
                      public virtual IAudioStateTracking,
                      public virtual IAudioRingbuffer<audio_sample_t> {
    std::atomic<float> ma_input_peak = 0.0f;
    std::atomic<float> ma_output_peak = 0.0f;
    std::atomic<float> ma_gain = 1.0f;
    std::atomic<bool> ma_muted = false;

    // Always-active FIFO queue acting as a ring buffer of sorts: data coming
    // in is always recorded until dropped from the queue.
    // Can be used for retroactive recording.
    BufferQueue<audio_sample_t> mp_always_record_ringbuffer;

public:
    using RingbufferSnapshot = typename BufferQueue<audio_sample_t>::Snapshot;
    using UsedBufferPool = BufferPool<audio_sample_t>;

    AudioPortBase(shoop_shared_ptr<UsedBufferPool> maybe_ringbuffer_buffer_pool);
    virtual ~AudioPortBase();

    PortDataType type() const override;

    /// Process samples: applies gain and mute, tracks peak levels.
    /// @param buf The audio buffer to process
    /// @param nframes Number of frames in the buffer
    virtual void process_samples(audio_sample_t* buf, uint32_t nframes);

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
    IAudioRingbuffer<audio_sample_t>* get_ringbuffer() override { return this; }
    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
    RingbufferSnapshot get_ringbuffer_contents() override;

    /// Get the ringbuffer for external use.
    BufferQueue<audio_sample_t>& ringbuffer() { return mp_always_record_ringbuffer; }
    const BufferQueue<audio_sample_t>& ringbuffer() const { return mp_always_record_ringbuffer; }
};