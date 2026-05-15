#include "MidiStorage.h"
#include "RustMidiStorage.h"
#include "types.h"
#include <atomic>
#include <cstdint>
#include <cstring>
#include <functional>
#include <iostream>
#include <memory>
#include <optional>
#include <stdexcept>
#include <stdio.h>
#include <vector>
#include <fmt/format.h>

using namespace logging;

// MidiStorageCore implementation

MidiStorageCore::MidiStorageCore(uint32_t data_size)
    : m_data(data_size / sizeof(Elem)) {
}

uint32_t MidiStorageCore::bytes_capacity() const {
    return m_data.size() * sizeof(Elem);
}

uint32_t MidiStorageCore::capacity_elems() const {
    return m_data.size();
}

bool MidiStorageCore::full() const {
    return m_n_events == m_data.size();
}

bool MidiStorageCore::empty() const {
    return m_n_events == 0;
}

void MidiStorageCore::clear() {
    m_head = m_tail = m_n_events = 0;
}

uint32_t MidiStorageCore::n_events() const {
    return m_n_events;
}

bool MidiStorageCore::append(uint32_t time, uint16_t size,
                             const uint8_t *data, bool allow_replace,
                             DroppedMsgCallback dropped_msg_cb) {
    log<log_level_debug_trace>("MidiStorageCore::append: time {}, size {}", time, size);

    if (full() && !allow_replace) {
        log<log_level_warning>("Ignoring store of MIDI message: buffer full.");
        return false;
    }
    if (m_n_events > 0) {
        uint32_t newest_idx = (m_head + m_data.size() - 1) % m_data.size();
        if (m_data[newest_idx].time > time) {
            // Don't store out-of-order messages
            log<log_level_warning>("Ignoring store of out-of-order MIDI message.");
            return false;
        }
    }

    uint32_t n_removed = 0;
    if (full()) {
        if (dropped_msg_cb) {
            auto &dropped_msg = m_data[m_tail];
            dropped_msg_cb(dropped_msg.time, dropped_msg.size, dropped_msg.data());
        }
        m_tail = (m_tail + 1) % m_data.size();
        n_removed++;
    }

    auto &elem = m_data[m_head];
    elem.time = time;
    elem.size = size;
    memcpy(elem.bytes, data, size);

    m_head = (m_head + 1) % m_data.size();
    if (m_n_events < m_data.size()) {
        m_n_events++;
    }

    if (n_removed > 0) {
        log<log_level_debug_trace>("MidiStorageCore::append: removed {} messages to make space", n_removed);
    }
    return true;
}

bool MidiStorageCore::prepend(uint32_t time, uint16_t size,
                              const uint8_t *data,
                              DroppedMsgCallback dropped_msg_cb) {
    if (full()) {
        if (dropped_msg_cb) {
            dropped_msg_cb(time, size, data);
        }
        log<log_level_debug_trace>("MidiStorageCore::prepend: buffer full");
        return false;
    }
    if (m_n_events > 0 && m_data[m_tail].time < time) {
        // Don't store out-of-order messages
        log<log_level_warning>("Ignoring store of out-of-order MIDI message.");
        return false;
    }

    uint32_t new_tail = (m_tail + m_data.size() - 1) % m_data.size();
    m_tail = new_tail;
    m_n_events++;

    auto &elem = m_data[m_tail];
    elem.time = time;
    elem.size = size;
    memcpy(elem.bytes, data, size);

    return true;
}

void MidiStorageCore::copy(IMidiStorageCore &to) const {
    auto* target = dynamic_cast<MidiStorageCore*>(&to);
    if (!target) {
        throw std::runtime_error("copy target must be MidiStorageCore or implement protected members");
    }
    target->m_data.resize(m_data.size());
    target->m_tail = 0;
    target->m_n_events = m_n_events;

    if (m_n_events == 0) {
        target->m_head = 0;
        return;
    }

    uint32_t count = 0;
    uint32_t idx = m_tail;
    while (count < m_n_events) {
        target->m_data[count] = m_data[idx];
        idx = (idx + 1) % m_data.size();
        count++;
    }
    target->m_head = count % m_data.size();
}

void MidiStorageCore::copy_from(const IMidiStorageCore &from) {
    // Cast from IMidiStorageCore to MidiStorageCore for internal access
    auto* source = dynamic_cast<const MidiStorageCore*>(&from);
    if (!source) {
        throw std::runtime_error("copy_from source must be MidiStorageCore or implement protected members");
    }
    m_data.resize(source->m_data.size());
    m_tail = source->m_tail;
    m_head = source->m_head;
    m_n_events = source->m_n_events;

    if (m_n_events == 0) {
        return;
    }

    uint32_t count = 0;
    uint32_t idx = source->m_tail;
    while (count < m_n_events) {
        m_data[idx] = source->m_data[idx];
        idx = (idx + 1) % m_data.size();
        count++;
    }
}

void MidiStorageCore::truncate(uint32_t time, MidiStorageTruncateSide type, DroppedMsgCallback dropped_msg_cb) {
    log<log_level_debug_trace>("MidiStorageCore::truncate to {}", time);
    if (type == MidiStorageTruncateSide::TruncateTail) {
        return truncate_fn([time](uint32_t t, uint16_t size, const uint8_t* data){ return t < time; }, type, dropped_msg_cb);
    } else if (type == MidiStorageTruncateSide::TruncateHead) {
        return truncate_fn([time](uint32_t t, uint16_t size, const uint8_t* data){ return t > time; }, type, dropped_msg_cb);
    }
}

void MidiStorageCore::truncate_fn(std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
                  MidiStorageTruncateSide type, DroppedMsgCallback dropped_msg_cb) {
    log<log_level_debug_trace>("MidiStorageCore::truncate to function");
    uint32_t cap = m_data.size();
    if (cap == 0 || m_n_events == 0) {
        return;
    }

    if (type == MidiStorageTruncateSide::TruncateHead) {
        uint32_t newest_idx = (m_head + cap - 1) % cap;
        auto &e = m_data[newest_idx];
        if (!should_truncate_fn(e.time, e.size, e.data())) {
            log<log_level_debug_trace>("MidiStorageCore::truncate: head unchanged");
            return;
        }

        uint32_t idx = m_tail;
        uint32_t kept = 0;
        for (uint32_t i = 0; i < m_n_events; ++i) {
            auto &elem = m_data[idx];
            if (should_truncate_fn(elem.time, elem.size, elem.data())) {
                break;
            }
            kept++;
            idx = (idx + 1) % cap;
        }

        if (dropped_msg_cb) {
            uint32_t drop_idx = idx;
            for (uint32_t i = kept; i < m_n_events; ++i) {
                auto &elem = m_data[drop_idx];
                dropped_msg_cb(elem.time, elem.size, elem.data());
                drop_idx = (drop_idx + 1) % cap;
            }
        }

        log<log_level_debug_trace>("MidiStorageCore::truncate head: {} -> {}, n msgs {} -> {}", m_head, idx, m_n_events, kept);
        m_head = idx;
        m_n_events = kept;

    } else if (type == MidiStorageTruncateSide::TruncateTail) {
        auto &e = m_data[m_tail];
        if (!should_truncate_fn(e.time, e.size, e.data())) {
            log<log_level_debug_trace>("MidiStorageCore::truncate: tail unchanged");
            return;
        }

        uint32_t idx = m_tail;
        uint32_t dropped = 0;
        for (uint32_t i = 0; i < m_n_events; ++i) {
            auto &elem = m_data[idx];
            if (!should_truncate_fn(elem.time, elem.size, elem.data())) {
                break;
            }
            if (dropped_msg_cb) {
                dropped_msg_cb(elem.time, elem.size, elem.data());
            }
            dropped++;
            idx = (idx + 1) % cap;
        }

        log<log_level_debug_trace>("MidiStorageCore::truncate tail: {} -> {}, n msgs {} -> {}", m_tail, idx, m_n_events, m_n_events - dropped);
        m_tail = idx;
        m_n_events -= dropped;
        if (m_n_events == 0) {
            m_head = m_tail;
        }
    }

    log<log_level_debug_trace>("MidiStorageCore::truncate: done");
}

void MidiStorageCore::for_each_msg_modify(
    std::function<void(uint32_t &t, uint16_t &s, uint8_t *data)> cb) {
    if (m_data.empty()) return;
    uint32_t idx = m_tail;
    for (uint32_t i = 0; i < m_n_events; ++i) {
        auto &elem = m_data[idx];
        cb(elem.time, elem.size, elem.bytes);
        idx = (idx + 1) % m_data.size();
    }
}

void MidiStorageCore::for_each_msg(
    std::function<void(uint32_t t, uint16_t s, uint8_t *data)> cb)
{
    for_each_msg_modify([cb](uint32_t &t, uint16_t &s, uint8_t *data) {
        cb(t, s, data);
    });
}

// MidiStorage implementation

MidiStorage::MidiStorage(uint32_t data_size)
    : m_core(std::make_unique<MidiStorageCore>(data_size)) {
    m_cursors.reserve(n_starting_cursors);
}

bool MidiStorage::append(uint32_t time, uint16_t size,
                         const uint8_t *data, bool allow_replace,
                         DroppedMsgCallback dropped_msg_cb) {
    auto tail = m_core->m_tail;
    auto rval = m_core->append(time, size, data, allow_replace, dropped_msg_cb);
    if (rval && m_core->m_tail != tail) {
        for (auto &c: m_cursors) {
            if(auto cc = c.lock()) {
                if (cc->offset() == tail) { cc->reset(); }
            }
        }
    }
    return rval;
}

void MidiStorage::clear() {
    for (auto &elem : m_cursors) {
        if (auto c = elem.lock()) {
            c->invalidate();
        }
    }
    m_cursors.clear();
    m_core->clear();
}

void MidiStorage::copy(IMidiStorageCore& to) const {
    // Cast to MidiStorage to get access to its m_core
    auto* midi_storage = dynamic_cast<MidiStorage*>(&to);
    if (midi_storage) {
        m_core->copy(*midi_storage->m_core);
    } else {
        // Try RustMidiStorage - iterate and copy elements using ringbuffer semantics
        auto* rust_storage = dynamic_cast<RustMidiStorage*>(&to);
        if (rust_storage) {
            uint32_t cap = m_core->raw_capacity();
            
            // Resize RustMidiStorage data to match capacity
            rust_storage->m_data.resize(cap);
            
            // Copy n_events from source to target, preserving ringbuffer positions
            uint32_t n = m_core->n_events();
            rust_storage->m_n_events = n;
            
            // Copy elements preserving their physical ringbuffer positions
            for (uint32_t i = 0; i < n; ++i) {
                uint32_t src_phys = (m_core->m_tail + i) % cap;
                auto* src_elem = m_core->get_elem_physical(src_phys);
                if (src_elem) {
                    // Store at same physical position in target
                    rust_storage->m_data[src_phys] = *src_elem;
                }
            }
            
            // Copy the ringbuffer state: head at (tail + n) % cap
            rust_storage->m_tail = m_core->m_tail;
            rust_storage->m_head = (m_core->m_tail + n) % cap;
        } else {
            throw std::runtime_error("copy target must be MidiStorage or RustMidiStorage");
        }
    }
}

shoop_shared_ptr<void> 
MidiStorage::create_cursor_shared() {
    return create_cursor();
}

typename MidiStorage::SharedCursor
MidiStorage::create_cursor() {
    auto self = shared_from_this();
    if (self) {
        auto rval = shoop_make_shared<Cursor>(self);
        m_cursors.push_back(rval);
        rval->reset();
        return rval;
    } else {
        throw std::runtime_error(
            "Attempting to create cursor for destructed storage");
    }
}

void MidiStorage::clear_cursors() {
    m_cursors.clear();
}

void MidiStorage::truncate(uint32_t time, MidiStorageTruncateSide side, DroppedMsgCallback dropped_msg_cb) {
    m_core->truncate(time, side, dropped_msg_cb);
    
    // Invalidate cursors that no longer point to valid elements
    auto is_valid_idx = [this](uint32_t idx) -> bool {
        auto n_events = m_core->m_n_events;
        auto m_tail = m_core->m_tail;
        auto m_head = m_core->m_head;
        if (n_events == 0) return false;
        if (m_head > m_tail) return idx >= m_tail && idx < m_head;
        if (m_head < m_tail) return idx >= m_tail || idx < m_head;
        return true; // full
    };
    for (auto &c : m_cursors) {
        if (auto cc = c.lock()) {
            auto off = cc->offset();
            if (!off.has_value()) continue;
            if (!is_valid_idx(off.value())) {
                cc->invalidate();
            }
        }
    }
}

void MidiStorage::truncate_fn(std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
                  MidiStorageTruncateSide side, DroppedMsgCallback dropped_msg_cb) {
    m_core->truncate_fn(should_truncate_fn, side, dropped_msg_cb);
    
    // Invalidate cursors that no longer point to valid elements
    auto is_valid_idx = [this](uint32_t idx) -> bool {
        auto n_events = m_core->m_n_events;
        auto m_tail = m_core->m_tail;
        auto m_head = m_core->m_head;
        if (n_events == 0) return false;
        if (m_head > m_tail) return idx >= m_tail && idx < m_head;
        if (m_head < m_tail) return idx >= m_tail || idx < m_head;
        return true; // full
    };
    for (auto &c : m_cursors) {
        if (auto cc = c.lock()) {
            auto off = cc->offset();
            if (!off.has_value()) continue;
            if (!is_valid_idx(off.value())) {
                cc->invalidate();
            }
        }
    }
}

void MidiStorage::for_each_msg_modify(
    std::function<void(uint32_t &t, uint16_t &s, uint8_t *data)> cb) {
    m_core->for_each_msg_modify(cb);
}

void MidiStorage::for_each_msg(
    std::function<void(uint32_t t, uint16_t s, uint8_t *data)> cb) {
    m_core->for_each_msg(cb);
}

// Backward-compatible versions (using TruncateSide alias)
void MidiStorage::truncate_compat(uint32_t time, TruncateSide side, DroppedMsgCallback dropped_msg_cb) {
    truncate(time, static_cast<MidiStorageTruncateSide>(side), dropped_msg_cb);
}

void MidiStorage::truncate_fn_compat(std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
                    TruncateSide side, DroppedMsgCallback dropped_msg_cb) {
    truncate_fn(should_truncate_fn, static_cast<MidiStorageTruncateSide>(side), dropped_msg_cb);
}

void MidiStorage::for_each_msg_modify_compat(
    std::function<void(uint32_t &t, uint16_t &s, uint8_t *data)> cb) {
    m_core->for_each_msg_modify(cb);
}

void MidiStorage::for_each_msg_compat(
    std::function<void(uint32_t t, uint16_t s, uint8_t *data)> cb) {
    m_core->for_each_msg(cb);
}

// MidiStorageCursor implementation

MidiStorageCursor::MidiStorageCursor(
    shoop_shared_ptr<IMidiStorage> _storage)
    : m_storage(_storage) {
}

bool MidiStorageCursor::valid() const {
    return m_offset.has_value() && m_offset != INVALID_OFFSET;
}

std::optional<uint32_t> MidiStorageCursor::offset() const {
    if (m_offset.has_value() && m_offset != INVALID_OFFSET) {
        return m_offset;
    }
    return std::nullopt;
}

std::optional<uint32_t>
MidiStorageCursor::prev_offset() const {
    if (m_prev_offset.has_value() && m_prev_offset != INVALID_OFFSET) {
        return m_prev_offset;
    }
    return std::nullopt;
}

void MidiStorageCursor::invalidate() {
    m_offset = std::nullopt;
    m_prev_offset = std::nullopt;
}

bool MidiStorageCursor::is_at_start() const {
    return offset() == m_storage->raw_tail();
}

void MidiStorageCursor::overwrite(uint32_t offset,
                                                      uint32_t prev_offset) {
    m_offset = offset;
    m_prev_offset = prev_offset;
}

void MidiStorageCursor::reset() {
    auto n_events = m_storage->n_events();
    if (n_events == 0) {
        invalidate();
    } else {
        m_offset = m_storage->raw_tail();
        m_prev_offset = std::nullopt;
    }
}

void MidiStorageCursor::next() {
    if (!m_offset.has_value()) { return; }
    auto storage_ptr = m_storage.get();
    uint32_t cap = storage_ptr->raw_capacity();
    uint32_t n_events = storage_ptr->n_events();
    if (cap == 0) { invalidate(); return; }

    uint32_t next = (m_offset.value() + 1) % cap;

    auto raw_full = storage_ptr->raw_full();
    auto head = storage_ptr->raw_head();
    auto tail = storage_ptr->raw_tail();

    if (cap == 1) {
        invalidate();
        return;
    }

    if (!raw_full && next == head) {
        invalidate();
        return;
    }

    if (raw_full && next == tail && m_prev_offset.has_value()) {
        invalidate();
        return;
    }

    m_prev_offset = m_offset;
    m_offset = next;
}

bool MidiStorageCursor::wrapped() const {
    if (!m_prev_offset.has_value() || !m_offset.has_value()) {
        return false;
    }
    return m_prev_offset.value() > m_offset.value();
}

MidiStorageCursor::FindResult MidiStorageCursor::find_time_forward(
    uint32_t time, std::function<void(Elem *)> maybe_skip_msg_callback)
{
    auto print_offset = m_offset.has_value() ? (int)m_offset.value() : (int)-1;
    auto storage_ptr = m_storage.get();
    log<log_level_debug_trace>("find_time_forward (storage {}, cursor {}, target time {})", fmt::ptr(storage_ptr), print_offset, time);
    return find_fn_forward([time](Elem *e) {
        return e->time >= time;
    }, maybe_skip_msg_callback);
}

MidiStorageCursor::FindResult MidiStorageCursor::find_fn_forward(
    std::function<bool(Elem *)> fn, std::function<void(Elem *)> maybe_skip_msg_callback)
{
    FindResult rval;
    rval.found_valid_elem = false;
    rval.n_processed = 0;

    auto print_offset = m_offset.has_value() ? (int)m_offset.value() : (int)-1;
    log<log_level_debug_trace>("find_fn_forward (storage {}, cursor {})", fmt::ptr(m_storage.get()), print_offset);
    if (!valid()) {
        log<log_level_debug_trace>("find_fn_forward: not valid, returning");
        return rval;
    }

    auto storage_ptr = m_storage.get();
    uint32_t cap = storage_ptr->raw_capacity();
    uint32_t n_events = storage_ptr->n_events();
    if (cap == 0) {
        invalidate();
        return rval;
    }

    uint32_t head = storage_ptr->raw_head();
    uint32_t tail = storage_ptr->raw_tail();
    bool raw_full = storage_ptr->raw_full();

    uint32_t idx = m_offset.value();
    uint32_t prev_idx = idx;
    for (uint32_t step = 0; step < n_events; ++step) {
        if (step > 0) {
            if (!raw_full && idx == head) { break; }
            if (raw_full && idx == tail) { break; }
        }

    Elem *elem = storage_ptr->get_elem_physical(idx);
        if (fn(elem)) {
            if (step > 0) {
                m_prev_offset = prev_idx;
                m_offset = idx;
            }
            rval.found_valid_elem = true;
            return rval;
        }

        if (maybe_skip_msg_callback) {
            log<log_level_debug_trace>("Skip event @ {}", elem->time);
            maybe_skip_msg_callback(elem);
        }

        prev_idx = idx;
        idx = (idx + 1) % cap;
        rval.n_processed++;
    }

    log<log_level_debug_trace>("find_fn_forward: none found");
    invalidate();
    return rval;
}

