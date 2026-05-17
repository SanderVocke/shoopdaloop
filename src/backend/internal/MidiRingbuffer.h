#pragma once

#include "LoggingEnabled.h"
#include "shoop_shared_ptr.h"
#include "RustMidiStorage.h"  // Need full definition of RustMidiStorage
#include <memory>
#include <optional>

// Forward declarations
class MidiStorageCursor;
class IMidiStorage;

/**
 * MidiRingbuffer: A thin C++ wrapper around Rust MIDI storage.
 */
class MidiRingbuffer : public ModuleLoggingEnabled<"Backend.MidiStorage"> {
public:
    using Storage = RustMidiStorage;

private:
    shoop_shared_ptr<RustMidiStorage> m_storage;

public:
    MidiRingbuffer(uint32_t data_size);

    void set_n_samples(uint32_t n);

    uint32_t get_n_samples() const;
    uint32_t get_current_n_samples() const { return get_n_samples(); }
    uint32_t get_current_start_time() const;
    uint32_t get_current_end_time() const;

    void next_buffer(uint32_t n_frames, DroppedMsgCallback dropped_msg_cb = nullptr);
    bool put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data, DroppedMsgCallback dropped_msg_cb = nullptr);
    void snapshot(IMidiStorage &target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const;

    IMidiStorage& storage();
    const IMidiStorage& storage() const;

    uint32_t n_events() const { return m_storage->n_events(); }
    uint32_t bytes_capacity() const { return m_storage->bytes_capacity(); }
    bool full() const { return m_storage->full(); }

    shoop_shared_ptr<MidiStorageCursor> create_cursor();

private:
    shoop_shared_ptr<MidiRingbuffer> shared_from_this() {
        return shoop_shared_ptr<MidiRingbuffer>(this, [](MidiRingbuffer*){});
    }
};