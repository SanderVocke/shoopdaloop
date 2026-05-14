#pragma once
#include "LoggingEnabled.h"
#include "IMidiStorageCore.h"
#include "shoop_shared_ptr.h"
#include <memory>
#include <optional>

// Forward declarations
class MidiStorageCore;
class MidiStorageCursor;

// Cursor find result
struct CursorFindResult {
    uint32_t n_processed;
    bool found_valid_elem;
};

// MidiStorageCore: Contains the raw ringbuffer data and basic operations.
// This is the core storage that can be easily bridged to Rust.
// Implements IMidiStorageCore and IMidiStorageOperations for FFI compatibility.
class MidiStorageCore : public ModuleLoggingEnabled<"Backend.MidiStorage">,
                        public IMidiStorageCore,
                        public IMidiStorageOperations {
public:
    using Elem = MidiStorageElem;

protected:
    std::vector<Elem> m_data;
    uint32_t m_tail = 0;
    uint32_t m_head = 0;
    uint32_t m_n_events = 0;

    friend class MidiStorage;

public:
    MidiStorageCore(uint32_t data_size);

    // IMidiStorageCore implementation
    uint32_t bytes_capacity() const override;
    uint32_t capacity_elems() const override;
    bool full() const override;
    bool empty() const override;
    uint32_t n_events() const override;

    // Raw accessors for cursor operations
    uint32_t raw_tail() const override { return m_tail; }
    uint32_t raw_head() const override { return m_head; }
    uint32_t raw_capacity() const override { return m_data.size(); }
    bool raw_full() const override { return m_n_events == m_data.size(); }

    Elem* get_elem(uint32_t idx) override { return &m_data.at(idx); }
    const Elem* get_elem(uint32_t idx) const override { return &m_data.at(idx); }
    std::vector<Elem>& data() override { return m_data; }
    const std::vector<Elem>& data() const override { return m_data; }

    // IMidiStorageOperations implementation
    bool append(uint32_t time, uint16_t size, const uint8_t* data,
                bool allow_replace=false,
                DroppedMsgCallback dropped_msg_cb=nullptr) override;
    bool prepend(uint32_t time, uint16_t size, const uint8_t* data) override;
    void clear() override;
    void copy(IMidiStorageCore &to) const override;
    void copy_from(const IMidiStorageCore &from) override;

    void truncate(uint32_t time, MidiStorageTruncateSide side, 
                  DroppedMsgCallback dropped_msg_cb=nullptr) override;
    void truncate_fn(std::function<bool(uint32_t time, uint16_t size, const uint8_t* data)> should_truncate_fn,
                     MidiStorageTruncateSide side, 
                     DroppedMsgCallback dropped_msg_cb=nullptr) override;
    void for_each_msg_modify(std::function<void(uint32_t &t, uint16_t &s, uint8_t* data)> cb) override;
    void for_each_msg(std::function<void(uint32_t t, uint16_t s, uint8_t* data)> cb) override;
};

// MidiStorage: Contains a MidiStorageCore and adds cursor management
class MidiStorage : public shoop_enable_shared_from_this<MidiStorage>,
                    public ModuleLoggingEnabled<"Backend.MidiStorage"> {
public:
    using Elem = MidiStorageElem;
    using Cursor = MidiStorageCursor;
    using SharedCursor = shoop_shared_ptr<Cursor>;

    using TruncateSide = MidiStorageTruncateSide;

private:
    std::unique_ptr<MidiStorageCore> m_core;
    std::vector<shoop_weak_ptr<Cursor>> m_cursors;
    static constexpr uint32_t n_starting_cursors = 10;

public:
    MidiStorage(uint32_t data_size);

    // Delegating methods to MidiStorageCore
    uint32_t bytes_capacity() const { return m_core->bytes_capacity(); }
    uint32_t capacity_elems() const { return m_core->capacity_elems(); }
    bool full() const { return m_core->full(); }
    void clear();
    uint32_t bytes_occupied() const { return m_core->n_events() * sizeof(Elem); }
    uint32_t bytes_free() const { return (m_core->capacity_elems() - m_core->n_events()) * sizeof(Elem); }
    uint32_t n_events() const { return m_core->n_events(); }

    uint32_t raw_tail() const { return m_core->raw_tail(); }
    uint32_t raw_head() const { return m_core->raw_head(); }
    uint32_t raw_capacity() const { return m_core->raw_capacity(); }
    bool raw_full() const { return m_core->raw_full(); }

    Elem* get_elem(uint32_t idx) { return m_core->get_elem(idx); }
    const Elem* get_elem(uint32_t idx) const { return m_core->get_elem(idx); }

    bool append(uint32_t time, uint16_t size, const uint8_t* data,
                bool allow_replace=false,
                DroppedMsgCallback dropped_msg_cb=nullptr);
    bool prepend(uint32_t time, uint16_t size, const uint8_t* data) {
        return m_core->prepend(time, size, data);
    }
    void copy(MidiStorage &to) const { m_core->copy(*to.m_core); }
    void copy_from(const MidiStorage &from) { m_core->copy_from(*from.m_core); }

    SharedCursor create_cursor();
    void clear_cursors();

    void truncate(uint32_t time, TruncateSide side, DroppedMsgCallback dropped_msg_cb=nullptr);
    void truncate_fn(std::function<bool(uint32_t time, uint16_t size, const uint8_t* data)> should_truncate_fn,
                     TruncateSide side, DroppedMsgCallback dropped_msg_cb=nullptr);
    void for_each_msg_modify(std::function<void(uint32_t &t, uint16_t &s, uint8_t* data)> cb);
    void for_each_msg(std::function<void(uint32_t t, uint16_t s, uint8_t* data)> cb);

    // Access to core for time-window operations in MidiRingbuffer
    MidiStorageCore* core() { return m_core.get(); }
    const MidiStorageCore* core() const { return m_core.get(); }

    std::vector<Elem>& data() { return m_core->data(); }
    const std::vector<Elem>& data() const { return m_core->data(); }
};

// MidiStorageCursor: Works with MidiStorage
class MidiStorageCursor : public ModuleLoggingEnabled<"Backend.MidiStorage"> {
public:
    using Storage = MidiStorage;
    using Elem = MidiStorageElem;

private:
    std::optional<uint32_t> m_offset = std::nullopt;
    std::optional<uint32_t> m_prev_offset = std::nullopt;
    shoop_shared_ptr<Storage> m_storage = nullptr;

public:
    MidiStorageCursor(shoop_shared_ptr<Storage> _storage);

    bool valid() const;

    std::optional<uint32_t> offset() const;
    std::optional<uint32_t> prev_offset() const;

    void invalidate();
    bool is_at_start() const;
    void overwrite(uint32_t offset, uint32_t prev_offset);

    void reset();

    Elem *get(uint32_t raw_offset) const;
    Elem *get() const;
    Elem *get_prev() const;
    void next();

    // True if previous elem is valid, current elem is valid,
    // but stepping between them steps over the ringbuffer
    // boundary
    bool wrapped() const;

    // Iterate to the next message until the given time (or later) or end of buffer is reached.
    CursorFindResult find_time_forward(uint32_t time, std::function<void(Elem *)> maybe_skip_msg_callback = nullptr);

    // Iterate to the next message until the given function returns true or end of buffer is reached.
    CursorFindResult find_fn_forward(std::function<bool(Elem *)> fn, std::function<void(Elem *)> maybe_skip_msg_callback = nullptr);
};

// MidiRingbuffer: Contains a MidiStorage instead of inheriting from it
class MidiRingbuffer : public ModuleLoggingEnabled<"Backend.MidiStorage"> {
public:
    using Storage = MidiStorage;

private:
    shoop_shared_ptr<MidiStorage> m_storage;
    std::atomic<uint32_t> n_samples = 0;
    std::atomic<uint32_t> current_buffer_start_time = 0;
    std::atomic<uint32_t> current_buffer_end_time = 0;

public:
    MidiRingbuffer(uint32_t data_size);

    // Set N samples. Also truncates the tail such that older data is erased.
    void set_n_samples(uint32_t n);

    uint32_t get_n_samples() const;
    uint32_t get_current_start_time() const;
    uint32_t get_current_end_time() const;

    // Increment the current time. Also truncates the tail such that out-of-range data is erased.
    void next_buffer(uint32_t n_frames, DroppedMsgCallback dropped_msg_cb = nullptr);

    // Put a message at the head of the ringbuffer.
    bool put(uint32_t frame_in_current_buffer, uint16_t size,  const uint8_t* data, DroppedMsgCallback dropped_msg_cb = nullptr);

    // Copy the current state of the ringbuffer to the target storage.
    // The timestamps on the messages in "target" are set such that
    // the time at "start_offset_from_end" before the current buffer end
    // is considered zero. If not given, the buffer length is used.
    // All messages before that point are truncated away.
    void snapshot(MidiStorage &target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const;

    // Access underlying storage
    MidiStorage& storage() { return *m_storage; }
    const MidiStorage& storage() const { return *m_storage; }

    // Delegating methods for backward compatibility with consumers that use MidiStorage methods
    uint32_t n_events() const { return m_storage->n_events(); }
    uint32_t bytes_capacity() const { return m_storage->bytes_capacity(); }
    bool full() const { return m_storage->full(); }

    // Cursor creation delegates to the underlying storage
    MidiStorage::SharedCursor create_cursor() { return m_storage->create_cursor(); }

private:
    // Helper to create a shared_ptr from MidiRingbuffer itself for cursor creation
    shoop_shared_ptr<MidiRingbuffer> shared_from_this() {
        return shoop_shared_ptr<MidiRingbuffer>(this, [](MidiRingbuffer*){});
    }

public:
};