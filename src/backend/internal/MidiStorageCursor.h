#pragma once
#include "IMidiStorageCore.h"
#include "IMidiStorageCursor.h"
#include "MidiStorageElem.h"
#include "LoggingEnabled.h"
#include "shoop_shared_ptr.h"
#include <cstdint>
#include <functional>
#include <optional>

/**
 * MidiStorageCursor - Cursor for iterating over MIDI storage.
 * Works with any IMidiStorage implementation (RustMidiStorage, etc.)
 * Implements IMidiStorageCursor.
 */
class MidiStorageCursor : public ModuleLoggingEnabled<"Backend.MidiStorage">,
                          public IMidiStorageCursor {
public:
    using Elem = MidiStorageElem;
    using SharedCursor = shoop_shared_ptr<MidiStorageCursor>;  // Typedef for cursor type

    // Sentinel value for invalid offset (matches Rust's INVALID_OFFSET)
    static constexpr uint32_t INVALID_OFFSET = 0xFFFFFFFF;

private:
    std::optional<uint32_t> m_offset = std::nullopt;
    std::optional<uint32_t> m_prev_offset = std::nullopt;
    shoop_shared_ptr<IMidiStorage> m_storage = nullptr;

public:
    // Get current offset value (INVALID_OFFSET if not valid)
    uint32_t get_raw_offset() const { 
        return m_offset.value_or(INVALID_OFFSET); 
    }
    // Get current prev_offset value (INVALID_OFFSET if not valid)
    uint32_t get_raw_prev_offset() const { 
        return m_prev_offset.value_or(INVALID_OFFSET); 
    }
    // Check if offset is valid
    bool is_offset_valid() const { 
        return m_offset.has_value() && m_offset.value() != INVALID_OFFSET; 
    }

public:
    MidiStorageCursor(shoop_shared_ptr<IMidiStorage> _storage);
    
    // For backward compatibility with MidiStorage
    template<typename StorageType>
    MidiStorageCursor(shoop_shared_ptr<StorageType> _storage) 
        : m_storage(std::move(_storage)) {}

    // IMidiStorageCursor implementation
    // Cursor state
    bool valid() const override;
    std::optional<uint32_t> offset() const override;
    std::optional<uint32_t> prev_offset() const override;

    // Navigation
    void invalidate() override;
    bool is_at_start() const override;
    void reset() override;
    void overwrite(uint32_t offset, uint32_t prev_offset) override;
    void next() override;

    // Element access
    Elem* get() override { return valid() ? get(m_offset.value()) : nullptr; }
    const Elem* get() const override { return valid() ? get(m_offset.value()) : nullptr; }
    Elem* get(uint32_t raw_offset) override { return m_storage->get_elem_physical(raw_offset); }
    const Elem* get(uint32_t raw_offset) const override { return m_storage->get_elem_physical(raw_offset); }
    Elem* get_prev() override { return m_prev_offset.has_value() ? get(m_prev_offset.value()) : nullptr; }
    const Elem* get_prev() const override { return m_prev_offset.has_value() ? get(m_prev_offset.value()) : nullptr; }

    // State queries
    bool wrapped() const override;

    // FindResult is now in IMidiStorageCursor
    using FindResult = IMidiStorageCursor::FindResult;

    // Iteration with predicate
    FindResult find_time_forward(uint32_t time, 
        std::function<void(Elem*)> maybe_skip_msg_callback = nullptr) override;
    FindResult find_fn_forward(std::function<bool(Elem*)> fn,
        std::function<void(Elem*)> maybe_skip_msg_callback = nullptr) override;
};