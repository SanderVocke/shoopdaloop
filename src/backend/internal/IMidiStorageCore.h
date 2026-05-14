#pragma once
#include "MidiStorageElem.h"
#include <cstdint>
#include <functional>
#include <optional>
#include <vector>

/**
 * IMidiStorageCore - Pure virtual interface for basic storage state access.
 * 
 * This interface exposes the fundamental state of the ringbuffer storage
 * without any mutation operations. It allows Rust implementations to
 * provide storage while C++ consumers can access the state.
 */
class IMidiStorageCore {
public:
    virtual ~IMidiStorageCore() = default;

    // Basic state queries
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
    virtual MidiStorageElem* get_elem(uint32_t idx) = 0;
    virtual const MidiStorageElem* get_elem(uint32_t idx) const = 0;
    virtual std::vector<MidiStorageElem>& data() = 0;
    virtual const std::vector<MidiStorageElem>& data() const = 0;
};

/**
 * TruncateSide - Enum for truncate direction
 */
enum class MidiStorageTruncateSide {
    TruncateHead, // Remove all messages with time > t from the head
    TruncateTail, // Remove all messages with time < t from the tail
};

/**
 * IMidiStorageOperations - Pure virtual interface for storage mutation operations.
 * 
 * This interface exposes all the mutating operations on the storage.
 * Dropped messages are reported via callback.
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
 * 
 * This is the main interface that C++ consumers and Rust implementations
 * will use for MIDI storage operations.
 */
class IMidiStorage : public IMidiStorageCore, public IMidiStorageOperations {
public:
    virtual ~IMidiStorage() = default;
};