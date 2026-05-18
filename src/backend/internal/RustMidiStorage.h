#pragma once

#include "MidiStorageElem.h"
#include "IMidiStorageCore.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/midi_storage_cxx.rs.h"
#include <cstdint>
#include <memory>
#include <functional>
#include <vector>

class MidiStorageCursor;
class MidiRingbuffer;  // Full definition in MidiStorage.h (included by this header's includer)

/**
 * RustMidiStorage - A wrapper that uses Rust for basic queries
 */
class RustMidiStorage : public shoop_enable_shared_from_this<RustMidiStorage>,
                        public IMidiStorage {
public:
    using Elem = MidiStorageElem;
    using Cursor = MidiStorageCursor;  // Alias for backward compatibility
    using TruncateSide = MidiStorageTruncateSide;  // For compatibility with MidiStorage

    RustMidiStorage(uint32_t data_size);
    virtual ~RustMidiStorage();

    // IMidiStorageCore - uses C++ storage state (synced with mutations)
    uint32_t n_events() const override { return m_n_events; }
    uint32_t capacity_elems() const override { return m_data.size(); }
    uint32_t bytes_capacity() const override { return capacity_elems() * sizeof(Elem); }
    uint32_t bytes_occupied() const { return m_n_events * sizeof(Elem); }
    uint32_t bytes_free() const { return (m_data.size() - m_n_events) * sizeof(Elem); }
    bool full() const override { return m_n_events == m_data.size(); }
    bool empty() const override { return m_n_events == 0; }

    uint32_t raw_tail() const override { return m_tail; }
    uint32_t raw_head() const override { return m_head; }
    uint32_t raw_capacity() const override { return m_data.size(); }
    bool raw_full() const override { return m_n_events == m_data.size(); }

    // Also sync Rust core state for any Rust-side queries
    void sync_rust_state();

    // Element access - uses C++ storage
    // Physical offset access (raw array index 0 to capacity-1)
    MidiStorageElem* get_elem_physical(uint32_t idx) override;
    const MidiStorageElem* get_elem_physical(uint32_t idx) const override;
    
    // Logical index access (0 = oldest, increasing toward newest)
    MidiStorageElem* get_elem_logical(uint32_t idx) override;
    const MidiStorageElem* get_elem_logical(uint32_t idx) const override;
    
    std::vector<MidiStorageElem>& data() override { return m_data; }
    const std::vector<MidiStorageElem>& data() const override { return m_data; }

    // IMidiStorageOperations - C++ implementation
    bool append(uint32_t time, uint16_t size, const uint8_t* data,
                bool allow_replace = false,
                DroppedMsgCallback dropped_msg_cb = nullptr) override;
    bool prepend(uint32_t time, uint16_t size, const uint8_t* data,
                 DroppedMsgCallback dropped_msg_cb = nullptr) override;
    void clear() override;
    void copy(IMidiStorageCore& target) const override;
    void copy_from(const IMidiStorageCore& from) override;
    void truncate(uint32_t time, MidiStorageTruncateSide side,
                  DroppedMsgCallback dropped_msg_cb = nullptr) override;
    void truncate_fn(std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
                     MidiStorageTruncateSide side,
                     DroppedMsgCallback dropped_msg_cb = nullptr) override;
    void for_each_msg_modify(std::function<void(uint32_t&, uint16_t&, uint8_t*)> cb) override;
    void for_each_msg(std::function<void(uint32_t, uint16_t, uint8_t*)> cb) override;

    // Cursor creation using MidiStorageCursor (which works with IMidiStorage)
    shoop_shared_ptr<MidiStorageCursor> create_cursor();
    shoop_shared_ptr<void> create_cursor_shared() override;

    // Time window operations (MidiRingbufferCore functionality)
    void set_n_samples(uint32_t n);
    uint32_t get_n_samples() const;
    uint32_t get_current_start_time() const;
    uint32_t get_current_end_time() const;
    void next_buffer(uint32_t n_frames, DroppedMsgCallback dropped_msg_cb = nullptr);
    bool put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data,
            DroppedMsgCallback dropped_msg_cb = nullptr);
    void snapshot(RustMidiStorage& target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const;

private:
    friend class MidiStorage;  // For copy operations
    rust::Box<backend_rust::MidiStorageCore> m_rust_core;
    rust::Box<backend_rust::MidiTimeWindow> m_time_window;
    std::vector<Elem> m_data;
    uint32_t m_tail = 0;
    uint32_t m_head = 0;
    uint32_t m_n_events = 0;
};