#pragma once

#include "MidiStorageElem.h"
#include "IMidiStorageCursor.h"
#include "LoggingEnabled.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/midi_storage_cxx.rs.h"
#include <cstdint>
#include <optional>
#include <functional>
#include <memory>

class IMidiStorage;
class RustMidiStorage;

/**
 * MidiStorageCursor - Cursor for iterating over MIDI storage.
 * 
 * This is a C++ wrapper around the Rust MidiCursor implementation.
 * All iteration logic is delegated to Rust via the CXX bridge.
 */
class MidiStorageCursor : public ModuleLoggingEnabled<"Backend.MidiStorage">,
                       public IMidiStorageCursor {
public:
    using SharedCursor = shoop_shared_ptr<MidiStorageCursor>;

    // Sentinel value for invalid offset (matches Rust's INVALID_OFFSET)
    static constexpr uint32_t INVALID_OFFSET = 0xFFFFFFFF;

private:
    // The Rust cursor we're wrapping
    rust::Box<backend_rust::MidiCursor> m_rust_cursor;
    // Weak reference to the storage (needed for some operations like is_at_start)
    // Note: storage lifetime must outlive the cursor
    uint64_t m_storage_ptr_key = 0;  // Used for debugging/identification

public:
    MidiStorageCursor(shoop_shared_ptr<IMidiStorage> storage);
    
    // For backward compatibility with specific storage types
    template<typename StorageType>
    MidiStorageCursor(shoop_shared_ptr<StorageType> storage);

    // IMidiStorageCursor implementation - delegates to Rust
    bool valid() const;
    std::optional<uint32_t> offset() const;
    std::optional<uint32_t> prev_offset() const;
    void invalidate();
    bool is_at_start() const;
    void reset();
    void overwrite(uint32_t offset, uint32_t prev_offset);
    void next();
    bool wrapped() const;

    // Element access - delegates to storage
    ::MidiStorageElem* get();
    const ::MidiStorageElem* get() const;
    ::MidiStorageElem* get(uint32_t raw_offset);
    const ::MidiStorageElem* get(uint32_t raw_offset) const;
    ::MidiStorageElem* get_prev();
    const ::MidiStorageElem* get_prev() const;

    // Iteration with predicate
    FindResult find_time_forward(uint32_t time,
        std::function<void(::MidiStorageElem*)> maybe_skip_msg_callback = nullptr);
    FindResult find_fn_forward(std::function<bool(::MidiStorageElem*)> fn,
        std::function<void(::MidiStorageElem*)> maybe_skip_msg_callback = nullptr);

    // Storage access for element lookups
    void set_storage(shoop_shared_ptr<IMidiStorage> storage);

private:
    shoop_shared_ptr<IMidiStorage> m_storage;
};