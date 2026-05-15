#pragma once
#include "MidiStorageElem.h"
#include "shoop_shared_ptr.h"
#include <cstdint>
#include <functional>
#include <optional>
#include <vector>

/**
 * IMidiStorageCore - Pure virtual interface for MIDI storage state access.
 */
class IMidiStorageCore {
public:
    virtual ~IMidiStorageCore() = default;

    // state queries
    virtual uint32_t n_events() const = 0;
    virtual uint32_t capacity_elems() const = 0;
    virtual uint32_t bytes_capacity() const = 0;
    virtual bool full() const = 0;
    virtual bool empty() const = 0;

    // Raw ringbuffer indices
    virtual uint32_t raw_tail() const = 0;
    virtual uint32_t raw_head() const = 0;
    virtual uint32_t raw_capacity() const = 0;
    virtual bool raw_full() const = 0;

    // Element access
    // Physical offset: raw index into underlying array (0 to capacity-1)
    virtual MidiStorageElem* get_elem_physical(uint32_t idx) = 0;
    virtual const MidiStorageElem* get_elem_physical(uint32_t idx) const = 0;
    
    // Logical index: 0 = oldest message, 1 = next oldest, etc.
    virtual MidiStorageElem* get_elem_logical(uint32_t idx) = 0;
    virtual const MidiStorageElem* get_elem_logical(uint32_t idx) const = 0;
    
    // Legacy alias for backward compatibility
    virtual MidiStorageElem* get_elem(uint32_t idx) = 0;
    virtual const MidiStorageElem* get_elem(uint32_t idx) const = 0;
    
    virtual std::vector<MidiStorageElem>& data() = 0;
    virtual const std::vector<MidiStorageElem>& data() const = 0;
};

enum class MidiStorageTruncateSide {
    TruncateHead, // Remove all messages with time > t from the head
    TruncateTail, // Remove all messages with time < t from the tail
};

/**
 * IMidiStorageOperations - Pure virtual interface for storage mutation operations.
 */
class IMidiStorageOperations {
public:
    virtual ~IMidiStorageOperations() = default;

    // Mutation operations
    virtual bool append(uint32_t time, uint16_t size, const uint8_t* data,
                        bool allow_replace = false,
                        DroppedMsgCallback dropped_msg_cb = nullptr) = 0;
    virtual bool prepend(uint32_t time, uint16_t size, const uint8_t* data,
                         DroppedMsgCallback dropped_msg_cb = nullptr) = 0;
    virtual void clear() = 0;

    // Copy operations
    virtual void copy(IMidiStorageCore& target) const = 0;
    virtual void copy_from(const IMidiStorageCore& from) = 0;

    // Truncate operations
    virtual void truncate(uint32_t time, MidiStorageTruncateSide side, 
                         DroppedMsgCallback dropped_msg_cb = nullptr) = 0;
    virtual void truncate_fn(std::function<bool(uint32_t time, uint16_t size, const uint8_t* data)> should_truncate_fn,
                             MidiStorageTruncateSide side, 
                             DroppedMsgCallback dropped_msg_cb = nullptr) = 0;

    // Iteration/modification
    virtual void for_each_msg_modify(std::function<void(uint32_t &t, uint16_t &s, uint8_t* data)> cb) = 0;
    virtual void for_each_msg(std::function<void(uint32_t t, uint16_t s, uint8_t* data)> cb) = 0;
};

/**
 * IMidiStorage - Combined interface for storage access and operations.
 */
class IMidiStorage : public IMidiStorageCore, public IMidiStorageOperations {
public:
    virtual ~IMidiStorage() = default;

    // Cursor management - using shared_ptr of the concrete cursor type
    // Subclasses should define their own SharedCursor type and implement this
    virtual shoop_shared_ptr<void> create_cursor_shared() = 0;
};

// Template for cursor creation in MidiStorage
template<typename StorageType>
shoop_shared_ptr<typename StorageType::Cursor>
create_storage_cursor(shoop_shared_ptr<StorageType> storage) {
    return storage->create_cursor();
}