#pragma once

#include <cstdint>
#include <cstring>

/// Utility for computing running checksums of audio/MIDI data.
/// Used for debugging/integration tests - allows comparing data flow
/// between test runs by tracking checksums at each process iteration.

namespace checksum {

/// Compute checksum of audio samples by summing absolute values.
/// This produces a stable checksum that can be used to verify
/// data consistency across test runs.
template<typename SampleT>
inline double compute_audio_checksum(SampleT const* data, uint32_t n_samples) {
    double sum = 0.0;
    for (uint32_t i = 0; i < n_samples; i++) {
        // Use absolute value to handle sign differences consistently
        sum += static_cast<double>(std::abs(data[i]));
    }
    return sum;
}

/// Compute checksum of MIDI message data by summing byte values.
/// Each message contributes its time + all byte values.
inline double compute_midi_message_checksum(uint32_t time, uint16_t size, const uint8_t* data) {
    double sum = static_cast<double>(time);
    for (uint16_t i = 0; i < size; i++) {
        sum += static_cast<double>(data[i]);
    }
    return sum;
}

/// Reset a checksum to zero (used when clearing buffers).
inline constexpr double zero_checksum() {
    return 0.0;
}

} // namespace checksum