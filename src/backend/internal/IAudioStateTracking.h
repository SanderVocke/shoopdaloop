#pragma once
#include "shoop_globals.h"

/**
 * IAudioStateTracking - Pure virtual interface for audio state tracking.
 * 
 * This interface defines the contract for tracking audio port state including
 * peak levels, gain, and mute status.
 * 
 * Note: This interface is designed for audio_sample_t (float) which is the
 * primary sample type used throughout the codebase.
 */
class IAudioStateTracking {
public:
    virtual ~IAudioStateTracking() {}
    
    /// Returns a pointer to this audio port as an IAudioStateTracking.
    virtual IAudioStateTracking* get_state_tracking() = 0;
    
    /// Gets the current input peak value.
    /// @return The input peak value
    virtual float get_input_peak() const = 0;
    
    /// Resets the input peak value to zero.
    virtual void reset_input_peak() = 0;
    
    /// Gets the current output peak value.
    /// @return The output peak value
    virtual float get_output_peak() const = 0;
    
    /// Resets the output peak value to zero.
    virtual void reset_output_peak() = 0;
    
    /// Gets the current gain value.
    /// @return The gain value (1.0 = no change)
    virtual float get_gain() const = 0;
    
    /// Sets the gain value.
    /// @param gain The gain value to apply (1.0 = no change)
    virtual void set_gain(float gain) = 0;
    
    /// Gets the mute state.
    /// @return true if muted, false otherwise
    virtual bool get_muted() const = 0;
    
    /// Sets the mute state.
    /// @param muted true to mute, false to unmute
    virtual void set_muted(bool muted) = 0;
};