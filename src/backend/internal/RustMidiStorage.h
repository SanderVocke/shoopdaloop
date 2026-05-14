#pragma once

#include "MidiStorageElem.h"
#include "IMidiStorageCore.h"
#include "backend_rust/src/midi_storage_cxx.rs.h"
#include <cstdint>
#include <memory>
#include <functional>
#include <vector>

/**
 * RustMidiStorage - A wrapper that uses Rust for basic queries
 * 
 * This class wraps the Rust MidiStorageCore via CXX bridge.
 * Rust provides: n_events, capacity, full, empty, raw_tail, raw_head, raw_full
 * C++ provides: append, prepend, clear, copy, truncate, iteration
 */
class RustMidiStorage : public IMidiStorage {
public:
    using Elem = MidiStorageElem;

    RustMidiStorage(uint32_t data_size);
    virtual ~RustMidiStorage();

    // IMidiStorageCore - uses Rust
    uint32_t n_events() const override { return m_rust_core->n_events(); }
    uint32_t capacity_elems() const override { return m_rust_core->capacity(); }
    uint32_t bytes_capacity() const override { return capacity_elems() * sizeof(Elem); }
    bool full() const override { return m_rust_core->full(); }
    bool empty() const override { return m_rust_core->empty(); }

    uint32_t raw_tail() const override { return m_rust_core->raw_tail(); }
    uint32_t raw_head() const override { return m_rust_core->raw_head(); }
    uint32_t raw_capacity() const override { return m_rust_core->capacity(); }
    bool raw_full() const override { return m_rust_core->raw_full(); }

    // Element access - uses C++ storage
    MidiStorageElem* get_elem(uint32_t idx) override;
    const MidiStorageElem* get_elem(uint32_t idx) const override;
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

private:
    rust::Box<backend_rust::MidiStorageCore> m_rust_core;
    std::vector<Elem> m_data;
    uint32_t m_tail = 0;
    uint32_t m_head = 0;
    uint32_t m_n_events = 0;
};