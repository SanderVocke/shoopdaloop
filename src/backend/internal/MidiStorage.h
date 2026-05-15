#pragma once
#include "LoggingEnabled.h"
#include "IMidiStorageCore.h"
#include "IMidiStorageCursor.h"
#include "IMidiRingbufferTimeWindow.h"
#include "shoop_shared_ptr.h"
#include <memory>
#include <optional>

// Forward declarations
class MidiStorageCore;
class MidiStorageCursor;
class RustMidiStorage;

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

    // Physical offset access (raw array index)
    Elem* get_elem_at_physical_offset(uint32_t idx) override { return &m_data.at(idx); }
    const Elem* get_elem_at_physical_offset(uint32_t idx) const override { return &m_data.at(idx); }
    
    // Logical index access (0 = oldest, increasing toward newest)
    Elem* get_elem_logical(uint32_t idx) override {
        if (idx >= m_n_events) return nullptr;
        uint32_t phys_idx = (m_tail + idx) % m_data.size();
        return &m_data[phys_idx];
    }
    const Elem* get_elem_logical(uint32_t idx) const override {
        if (idx >= m_n_events) return nullptr;
        uint32_t phys_idx = (m_tail + idx) % m_data.size();
        return &m_data[phys_idx];
    }
    
    // Legacy alias
    Elem* get_elem(uint32_t idx) override { return get_elem_at_physical_offset(idx); }
    const Elem* get_elem(uint32_t idx) const override { return get_elem_at_physical_offset(idx); }
    
    std::vector<Elem>& data() override { return m_data; }
    const std::vector<Elem>& data() const override { return m_data; }

    // IMidiStorageOperations implementation
    bool append(uint32_t time, uint16_t size, const uint8_t* data,
                bool allow_replace=false,
                DroppedMsgCallback dropped_msg_cb=nullptr) override;
    bool prepend(uint32_t time, uint16_t size, const uint8_t* data,
                 DroppedMsgCallback dropped_msg_cb=nullptr) override;
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
// Implements IMidiStorage for full interface support
class MidiStorage : public shoop_enable_shared_from_this<MidiStorage>,
                    public ModuleLoggingEnabled<"Backend.MidiStorage">,
                    public IMidiStorage {
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

    // All methods implement IMidiStorage interface
    // Note: bytes_occupied/bytes_free are convenience methods
    uint32_t bytes_occupied() const { return m_core->n_events() * sizeof(Elem); }
    uint32_t bytes_free() const { return (m_core->capacity_elems() - m_core->n_events()) * sizeof(Elem); }

    uint32_t n_events() const override { return m_core->n_events(); }
    uint32_t capacity_elems() const override { return m_core->capacity_elems(); }
    uint32_t bytes_capacity() const override { return m_core->bytes_capacity(); }
    bool full() const override { return m_core->full(); }
    bool empty() const override { return m_core->empty(); }
    uint32_t raw_tail() const override { return m_core->raw_tail(); }
    uint32_t raw_head() const override { return m_core->raw_head(); }
    uint32_t raw_capacity() const override { return m_core->raw_capacity(); }
    bool raw_full() const override { return m_core->raw_full(); }
    
    // Physical offset access (delegates to core)
    MidiStorageElem* get_elem_at_physical_offset(uint32_t idx) override { return m_core->get_elem_at_physical_offset(idx); }
    const MidiStorageElem* get_elem_at_physical_offset(uint32_t idx) const override { return m_core->get_elem_at_physical_offset(idx); }
    
    // Logical index access (delegates to core)
    MidiStorageElem* get_elem_logical(uint32_t idx) override { return m_core->get_elem_logical(idx); }
    const MidiStorageElem* get_elem_logical(uint32_t idx) const override { return m_core->get_elem_logical(idx); }
    
    // Legacy alias
    MidiStorageElem* get_elem(uint32_t idx) override { return m_core->get_elem(idx); }
    const MidiStorageElem* get_elem(uint32_t idx) const override { return m_core->get_elem(idx); }
    
    std::vector<MidiStorageElem>& data() override { return m_core->data(); }
    const std::vector<MidiStorageElem>& data() const override { return m_core->data(); }

    // IMidiStorageOperations interface
    bool append(uint32_t time, uint16_t size, const uint8_t* data,
                bool allow_replace = false,
                DroppedMsgCallback dropped_msg_cb = nullptr) override;
    bool prepend(uint32_t time, uint16_t size, const uint8_t* data,
                 DroppedMsgCallback dropped_msg_cb = nullptr) override { return m_core->prepend(time, size, data, dropped_msg_cb); }
    void clear() override;
    void copy(IMidiStorageCore& to) const override;
    void copy_from(const IMidiStorageCore& from) override { m_core->copy_from(from); }
    void truncate(uint32_t time, MidiStorageTruncateSide side,
                  DroppedMsgCallback dropped_msg_cb = nullptr) override;
    void truncate_fn(std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
                    MidiStorageTruncateSide side,
                    DroppedMsgCallback dropped_msg_cb = nullptr) override;
    void for_each_msg_modify(std::function<void(uint32_t&, uint16_t&, uint8_t*)> cb) override;
    void for_each_msg(std::function<void(uint32_t, uint16_t, uint8_t*)> cb) override;

    // Copy to/from MidiStorage (convenience)
    void copy(MidiStorage &to) const { m_core->copy(*to.m_core); }
    void copy_from(const MidiStorage &from) { m_core->copy_from(*from.m_core); }

    // Cursor management (not part of IMidiStorage)
    SharedCursor create_cursor();
    void clear_cursors();
    
    // IMidiStorage interface implementation
    shoop_shared_ptr<void> create_cursor_shared() override;

    // Access to core for time-window operations in MidiRingbuffer
    MidiStorageCore* core() { return m_core.get(); }
    const MidiStorageCore* core() const { return m_core.get(); }

    // Backward-compatible truncate using TruncateSide alias
    void truncate_compat(uint32_t time, TruncateSide side, DroppedMsgCallback dropped_msg_cb=nullptr);
    void truncate_fn_compat(std::function<bool(uint32_t time, uint16_t size, const uint8_t* data)> should_truncate_fn,
                     TruncateSide side, DroppedMsgCallback dropped_msg_cb=nullptr);
    void for_each_msg_modify_compat(std::function<void(uint32_t &t, uint16_t &s, uint8_t* data)> cb);
    void for_each_msg_compat(std::function<void(uint32_t t, uint16_t s, uint8_t* data)> cb);
};

// MidiStorageCursor: Works with any IMidiStorage implementation (MidiStorage or RustMidiStorage)
// Implements IMidiStorageCursor
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
    Elem* get(uint32_t raw_offset) override { return m_storage->get_elem(raw_offset); }
    const Elem* get(uint32_t raw_offset) const override { return m_storage->get_elem(raw_offset); }
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

// MidiRingbufferCore: Implements time-window logic for MIDI ringbuffer
class MidiRingbufferCore : public ModuleLoggingEnabled<"Backend.MidiStorage">,
                           public IMidiRingbufferTimeWindow {
public:
    MidiRingbufferCore(shoop_shared_ptr<IMidiStorage> storage);
    virtual ~MidiRingbufferCore() = default;

    // IMidiRingbufferTimeWindow implementation
    void set_n_samples(uint32_t n) override;
    uint32_t get_n_samples() const override;
    uint32_t get_current_start_time() const override;
    uint32_t get_current_end_time() const override;
    void next_buffer(uint32_t n_frames, 
                    std::function<void(uint32_t, uint16_t, const uint8_t*)> dropped_msg_cb = nullptr) override;
    bool put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data,
            std::function<void(uint32_t, uint16_t, const uint8_t*)> dropped_msg_cb = nullptr) override;
    void snapshot(IMidiStorage& target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const override;

    // Access to underlying storage for cursor creation
    IMidiStorage& storage() { return *m_storage; }
    const IMidiStorage& storage() const { return *m_storage; }

private:
    shoop_shared_ptr<IMidiStorage> m_storage;
    std::atomic<uint32_t> n_samples = 0;
    std::atomic<uint32_t> current_buffer_start_time = 0;
    std::atomic<uint32_t> current_buffer_end_time = 0;
};

// MidiRingbuffer: Contains a MidiStorage and MidiRingbufferCore
class MidiRingbuffer : public ModuleLoggingEnabled<"Backend.MidiStorage"> {
public:
    using Storage = MidiStorage;

private:
    shoop_shared_ptr<IMidiStorage> m_storage;
    std::unique_ptr<MidiRingbufferCore> m_time_window;

public:
    MidiRingbuffer(uint32_t data_size);

    // Set N samples. Also truncates the tail such that older data is erased.
    void set_n_samples(uint32_t n) { m_time_window->set_n_samples(n); }

    uint32_t get_n_samples() const { return m_time_window->get_n_samples(); }
    uint32_t get_current_start_time() const { return m_time_window->get_current_start_time(); }
    uint32_t get_current_end_time() const { return m_time_window->get_current_end_time(); }

    // Increment the current time. Also truncates the tail such that out-of-range data is erased.
    void next_buffer(uint32_t n_frames, DroppedMsgCallback dropped_msg_cb = nullptr) {
        m_time_window->next_buffer(n_frames, dropped_msg_cb);
    }

    // Put a message at the head of the ringbuffer.
    bool put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data, DroppedMsgCallback dropped_msg_cb = nullptr) {
        return m_time_window->put(frame_in_current_buffer, size, data, dropped_msg_cb);
    }

    // Copy the current state of the ringbuffer to the target storage.
    // The timestamps on the messages in "target" are set such that
    // the time at "start_offset_from_end" before the current buffer end
    // is considered zero. If not given, the buffer length is used.
    // All messages before that point are truncated away.
    void snapshot(IMidiStorage &target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const {
        m_time_window->snapshot(target, start_offset_from_end);
    }

    // Access underlying storage (via IMidiStorage interface)
    IMidiStorage& storage() { return *m_storage; }
    const IMidiStorage& storage() const { return *m_storage; }

    // Delegating methods for backward compatibility with consumers that use MidiStorage methods
    uint32_t n_events() const { return m_storage->n_events(); }
    uint32_t bytes_capacity() const { return m_storage->bytes_capacity(); }
    bool full() const { return m_storage->full(); }

    // Cursor creation - returns MidiStorageCursor compatible cursor
    // Note: m_storage can be MidiStorage or RustMidiStorage, both create MidiStorageCursor
    shoop_shared_ptr<MidiStorageCursor> create_cursor();

private:
    // Helper to create a shared_ptr from MidiRingbuffer itself for cursor creation
    shoop_shared_ptr<MidiRingbuffer> shared_from_this() {
        return shoop_shared_ptr<MidiRingbuffer>(this, [](MidiRingbuffer*){});
    }

public:
};