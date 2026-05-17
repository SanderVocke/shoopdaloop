#pragma once
#include "shoop_globals.h"
#include "BufferQueue.h"

/**
 * IAudioRingbuffer - Pure virtual interface for audio ringbuffer operations.
 * 
 * This interface defines the contract for managing the audio ringbuffer,
 * which handles always-on recording functionality.
 * 
 * Note: This interface is designed for audio_sample_t (float) which is the
 * primary sample type used throughout the codebase.
 */
template<typename SampleT>
class IAudioRingbuffer {
public:
    virtual ~IAudioRingbuffer() {}
    
    /// Returns a pointer to this audio port as an IAudioRingbuffer.
    virtual IAudioRingbuffer<SampleT>* get_ringbuffer() = 0;
    
    /// Sets the number of samples in the ringbuffer.
    /// @param n The number of samples
    virtual void set_ringbuffer_n_samples(unsigned n) = 0;
    
    /// Gets the number of samples in the ringbuffer.
    /// @return The number of samples
    virtual unsigned get_ringbuffer_n_samples() const = 0;
    
    /// Gets the current contents of the ringbuffer.
    /// @return A snapshot of the ringbuffer contents
    virtual typename BufferQueue<SampleT>::Snapshot get_ringbuffer_contents() = 0;
};