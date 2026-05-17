#pragma once
#include "shoop_globals.h"

/**
 * IAudioReadableBuffer - Pure virtual interface for readable audio buffers.
 * 
 * This interface defines the contract for reading audio samples from a buffer.
 * Implementations must provide methods to query the buffer and retrieve samples.
 * 
 * Note: This interface is designed for audio_sample_t (float) which is the
 * primary sample type used throughout the codebase.
 */
class IAudioReadableBuffer {
public:
    virtual ~IAudioReadableBuffer() {}
    
    /// Returns a pointer to this audio port as an IAudioReadableBuffer.
    virtual IAudioReadableBuffer* get_readable_buffer() = 0;
    
    /// Returns a pointer to the audio sample data for reading.
    virtual audio_sample_t* get_read_ptr() = 0;
    
    /// Returns the number of samples in the buffer.
    virtual uint32_t n_samples() const = 0;
    
    /// Gets the current peak values (input and output) from the buffer.
    /// @param in_peak Output parameter for input peak value
    /// @param out_peak Output parameter for output peak value
    virtual void get_peak(float& in_peak, float& out_peak) = 0;
};