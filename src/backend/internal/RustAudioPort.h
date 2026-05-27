#pragma once

/**
 * RustAudioPortF32 - C++ wrapper around Rust AudioPort for f32 samples
 * 
 * This provides a drop-in replacement for the C++ AudioPort<float> template,
 * delegating all operations to the Rust implementation via the CXX bridge.
 * 
 * Features:
 * - Atomic peak meters (input/output)
 * - Atomic gain control
 * - Atomic mute state
 * - Ringbuffer (AudioBufferQueue) for retroactive recording
 * - Zero-allocation on audio thread
 */

#include "PortInterface.h"
#include "AudioBuffer.h"
#include "BufferPool.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/audio_port_cxx.rs.h"

#include <cstdint>
#include <cstring>
#include <optional>
#include <vector>
#include <algorithm>
#include <stdexcept>

// Forward declaration of Rust audio port type
class RustAudioPortF32;

// Buffer pointer info for ringbuffer snapshots
struct RustAudioPortBufferInfo {
    const float* data_ptr;
    size_t len;
};

// Snapshot structure matching AudioPort's RingbufferSnapshot
struct RustAudioPortSnapshot {
    shoop_shared_ptr<std::vector<shoop_shared_ptr<AudioBuffer<float>>>> data;
    uint32_t n_samples;
    uint32_t buffer_size;
};

/**
 * RustAudioPortF32 - Audio port backed by Rust implementation
 * 
 * This wraps the Rust AudioPort and provides:
 * - Peak metering
 * - Gain/mute control
 * - Ringbuffer access
 * 
 * Note: This is currently only implemented for float samples.
 */
class RustAudioPortF32 : public virtual PortInterface {
public:
    using BufferObj = AudioBuffer<float>;
    using UsedBufferPool = BufferPool<float>;
    using RingbufferSnapshot = RustAudioPortSnapshot;

protected:
    // Rust implementation - wrapped in optional since rust::Box has no default constructor
    std::optional<rust::Box<backend_rust::AudioPort>> m_rust;
    
    // Buffer for PROC_get_buffer (used by subclasses)
    std::vector<float> m_buffer;
    uint32_t m_buffer_size = 0;

public:
    /**
     * Create a new RustAudioPortF32
     * 
     * @param buffer_pool Pool for ringbuffer allocations (can be nullptr for no ringbuffer)
     * @param max_buffers Maximum number of ringbuffer buffers
     */
    RustAudioPortF32(shoop_shared_ptr<UsedBufferPool> buffer_pool, uint32_t max_buffers = 32);
    
    /**
     * Default constructor - creates port with default ringbuffer settings
     */
    RustAudioPortF32();
    
    virtual ~RustAudioPortF32();

    // PortInterface implementation
    PortDataType type() const override { return PortDataType::Audio; }

    /**
     * Get the audio buffer for processing.
     * Returns pointer to internal buffer, which is resized as needed.
     * 
     * @param n_frames Number of frames needed
     * @return Pointer to buffer (never nullptr)
     */
    virtual float* PROC_get_buffer(uint32_t n_frames);

    /**
     * Process audio: applies gain/mute, tracks peaks, records to ringbuffer.
     * Should be called after PROC_get_buffer() and after the buffer is filled.
     * 
     * @param n_frames Number of frames to process
     */
    virtual void PROC_process(uint32_t n_frames);

    // Gain control
    void set_gain(float gain);
    float get_gain() const;

    // Mute control
    void set_muted(bool muted) override;
    bool get_muted() const override;

    // Peak meters
    float get_input_peak() const;
    void reset_input_peak();
    float get_output_peak() const;
    void reset_output_peak();

    // Ringbuffer configuration
    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;

    // Ringbuffer access
    RingbufferSnapshot PROC_get_ringbuffer_contents();

    // Name/driver - not used in base class
    const char* name() const override { return ""; }
    void close() override {}
    void* maybe_driver_handle() const override { return nullptr; }
    PortExternalConnectionStatus get_external_connection_status() const override { return PortExternalConnectionStatus(); }
    void connect_external(std::string name) override { (void)name; }
    void disconnect_external(std::string name) override { (void)name; }
    bool has_internal_read_access() const override { return true; }
    bool has_internal_write_access() const override { return true; }
    bool has_implicit_input_source() const override { return false; }
    bool has_implicit_output_sink() const override { return false; }
};

// Type alias for convenience - matches AudioPort<float>
using _RustAudioPort = RustAudioPortF32;