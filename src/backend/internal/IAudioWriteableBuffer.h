#pragma once
#include "shoop_globals.h"

/**
 * IAudioWriteableBuffer - Pure virtual interface for writeable audio buffers.
 * 
 * This interface defines the contract for writing audio samples to a buffer.
 * Implementations must provide methods to access write pointers and configure
 * buffer properties.
 * 
 * Note: This interface is designed for audio_sample_t (float) which is the
 * primary sample type used throughout the codebase.
 */
class IAudioWriteableBuffer {
public:
    virtual ~IAudioWriteableBuffer() {}
    
    /// Returns a pointer to this audio port as an IAudioWriteableBuffer.
    virtual IAudioWriteableBuffer* get_writeable_buffer() = 0;
    
    /// Returns a pointer to the audio sample data for writing.
    virtual audio_sample_t* get_write_ptr() = 0;
    
    /// Returns the capacity (number of samples) of the buffer.
    virtual uint32_t capacity() const = 0;
    
    /// Sets the gain multiplier for samples in this buffer.
    /// @param gain The gain value to apply (1.0 = no change)
    virtual void set_gain(float gain) = 0;
};