#pragma once
#include "IMidiStorageCore.h"
#include "MidiStorageElem.h"
#include <cstdint>
#include <functional>
#include <optional>

// Forward declarations
class IMidiStorageCursor;

/**
 * IMidiStorageCursor - Pure virtual interface for MIDI storage cursor operations.
 * 
 * This interface provides cursor-based iteration over MIDI storage elements.
 * It allows Rust implementations to provide cursor functionality while
 * C++ consumers can use the cursor API.
 */
class IMidiStorageCursor {
public:
    virtual ~IMidiStorageCursor() = default;

    // Cursor state
    virtual bool valid() const = 0;
    virtual std::optional<uint32_t> offset() const = 0;
    virtual std::optional<uint32_t> prev_offset() const = 0;

    // Navigation
    virtual void invalidate() = 0;
    virtual bool is_at_start() const = 0;
    virtual void reset() = 0;
    virtual void overwrite(uint32_t offset, uint32_t prev_offset) = 0;
    virtual void next() = 0;

    // Element access
    virtual MidiStorageElem* get() = 0;
    virtual const MidiStorageElem* get() const = 0;
    virtual MidiStorageElem* get(uint32_t raw_offset) = 0;
    virtual const MidiStorageElem* get(uint32_t raw_offset) const = 0;
    virtual MidiStorageElem* get_prev() = 0;
    virtual const MidiStorageElem* get_prev() const = 0;

    // State queries
    virtual bool wrapped() const = 0;

    // Iteration results
    struct FindResult {
        uint32_t n_processed;
        bool found_valid_elem;
    };

    // Iteration with predicate
    virtual FindResult find_time_forward(uint32_t time, 
        std::function<void(MidiStorageElem*)> maybe_skip_msg_callback = nullptr) = 0;
    virtual FindResult find_fn_forward(std::function<bool(MidiStorageElem*)> fn,
        std::function<void(MidiStorageElem*)> maybe_skip_msg_callback = nullptr) = 0;
};

/**
 * CursorFreeFunctions - Free functions for cursor operations.
 * 
 * These functions work with IMidiStorageCursor and IMidiStorageCore
 * to provide cursor-based iteration functionality.
 */
class CursorFreeFunctions {
public:
    // Create a cursor for the given storage
    static IMidiStorageCursor* create_cursor(IMidiStorageCore* storage);

    // Find operations
    static IMidiStorageCursor::FindResult cursor_find_time_forward(
        IMidiStorageCursor* cursor,
        uint32_t time,
        std::function<void(MidiStorageElem*)> maybe_skip_msg_callback = nullptr);

    static IMidiStorageCursor::FindResult cursor_find_fn_forward(
        IMidiStorageCursor* cursor,
        std::function<bool(MidiStorageElem*)> fn,
        std::function<void(MidiStorageElem*)> maybe_skip_msg_callback = nullptr);
};