#pragma once

#include "MidiStorageElem.h"
#include "IMidiStorageCore.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/midi_storage_cxx.rs.h"
#include <cstdint>
#include <memory>
#include <functional>
#include <vector>
#include <optional>

class MidiStorageCursor;
class MidiRingbuffer;

/**
 * RustMidiStorage - Pure Rust-backed MIDI storage.
 * 
 * All data lives in Rust MidiStorageCore. C++ is just a thin wrapper
 * that queries Rust via the CXX FFI bridge. No data is duplicated between
 * C++ and Rust.
 */
class RustMidiStorage : public shoop_enable_shared_from_this<RustMidiStorage>,
                        public IMidiStorage {
public:
    using Elem = MidiStorageElem;
    using Cursor = MidiStorageCursor;
    using TruncateSide = MidiStorageTruncateSide;

    RustMidiStorage(uint32_t data_size);
    virtual ~RustMidiStorage();

    // IMidiStorageCore - all queries go through FFI to Rust
    uint32_t n_events() const override { return m_rust_core->n_events(); }
    uint32_t capacity_elems() const override { return m_rust_core->capacity(); }
    uint32_t bytes_capacity() const override { return capacity_elems() * sizeof(Elem); }
    uint32_t bytes_occupied() const { return n_events() * sizeof(Elem); }
    uint32_t bytes_free() const { return bytes_capacity() - bytes_occupied(); }
    bool full() const override { return m_rust_core->full(); }
    bool empty() const override { return m_rust_core->empty(); }

    uint32_t raw_tail() const override { return m_rust_core->raw_tail(); }
    uint32_t raw_head() const override { return m_rust_core->raw_head(); }
    uint32_t raw_capacity() const override { return m_rust_core->capacity(); }
    bool raw_full() const override { return m_rust_core->raw_full(); }

    // Element access - queries Rust via FFI
    // Returns pointer to m_elem_buffer which is populated from Rust on each call
    MidiStorageElem* get_elem_physical(uint32_t idx) override;
    const MidiStorageElem* get_elem_physical(uint32_t idx) const override;
    
    // Logical index access (0 = oldest, increasing toward newest)
    MidiStorageElem* get_elem_logical(uint32_t idx) override;
    const MidiStorageElem* get_elem_logical(uint32_t idx) const override;
    
    // Note: data() returns empty vector - all data is in Rust
    std::vector<MidiStorageElem>& data() override { return m_empty_data; }
    const std::vector<MidiStorageElem>& data() const override { return m_empty_data; }

    // IMidiStorageOperations - delegates to Rust via FFI
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

    // Cursor creation
    shoop_shared_ptr<MidiStorageCursor> create_cursor();
    shoop_shared_ptr<void> create_cursor_shared() override;

    // Time window operations
    void set_n_samples(uint32_t n);
    uint32_t get_n_samples() const;
    uint32_t get_current_start_time() const;
    uint32_t get_current_end_time() const;
    void next_buffer(uint32_t n_frames, DroppedMsgCallback dropped_msg_cb = nullptr);
    bool put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data,
            DroppedMsgCallback dropped_msg_cb = nullptr);
    void snapshot(RustMidiStorage& target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const;

private:
    friend class MidiStorageCursor;
    rust::Box<backend_rust::MidiStorageCore> m_rust_core;
    rust::Box<backend_rust::MidiTimeWindow> m_time_window;
    
    // Mutable buffer used to return elements through FFI
    // This is populated on each get_elem_physical/logical call
    mutable MidiStorageElem m_elem_buffer;
    
    // Friend accessor for MidiStorageCursor to get element at raw offset
    // Returns true if element exists, fills out_time/size/bytes
    bool get_elem_at_physical_offset_raw(uint32_t idx, uint32_t& out_time, uint16_t& out_size, std::array<uint8_t, 4>& out_bytes) const;
    
    // Empty vector returned by data() for interface compliance
    std::vector<Elem> m_empty_data;
};

// Dropped message callback trampoline for FFI
static void dropped_cb_trampoline(uint32_t time, uint16_t size, const uint8_t* data, uintptr_t ctx) {
    auto* cb = reinterpret_cast<DroppedMsgCallback*>(ctx);
    if (*cb) {
        (*cb)(time, size, data);
    }
}