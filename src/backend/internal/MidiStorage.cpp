#include "MidiStorage.h"
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

uint8_t *MidiStorageElem::data() {
    return bytes;
}

const uint8_t *MidiStorageElem::data() const {
    return bytes;
}

const uint8_t *MidiStorageElem::get_data() const {
    return bytes;
}

uint32_t MidiStorageElem::get_time() const {
    return proc_time;
}

uint32_t MidiStorageElem::get_size() const {
    return size;
}

void MidiStorageElem::get(uint32_t &size_out,
                                              uint32_t &time_out,
                                              const uint8_t *&data_out) const {
    size_out = size;
    time_out = proc_time;
    data_out = data();
}

MidiStorageBase::MidiStorageBase(uint32_t data_size)
    : m_data(data_size / sizeof(Elem)) {
}

uint32_t MidiStorageBase::bytes_capacity() const {
    return m_data.size() * sizeof(Elem);
}

uint32_t MidiStorageBase::capacity_elems() const {
    return m_data.size();
}

bool MidiStorageBase::full() const {
    return m_n_events == m_data.size();
}

void MidiStorageBase::clear() {
    m_head = m_tail = m_n_events = 0;
}

uint32_t MidiStorageBase::bytes_occupied() const {
    return m_n_events * sizeof(Elem);
}

uint32_t MidiStorageBase::bytes_free() const {
    return (m_data.size() - m_n_events) * sizeof(Elem);
}

uint32_t MidiStorageBase::n_events() const {
    return m_n_events;
}

bool MidiStorageBase::append(uint32_t time, uint16_t size,
                             const uint8_t *data, bool allow_replace,
                             DroppedMsgCallback dropped_msg_cb) {
    log<log_level_debug_trace>("append: time {}, size {}", time, size);

    if (full() && !allow_replace) {
        log<log_level_warning>("Ignoring store of MIDI message: buffer full.");
        return false;
    }
    if (m_n_events > 0) {
        uint32_t newest_idx = (m_head + m_data.size() - 1) % m_data.size();
        if (m_data[newest_idx].storage_time > time) {
            // Don't store out-of-order messages
            log<log_level_warning>("Ignoring store of out-of-order MIDI message.");
            return false;
        }
    }

    uint32_t n_removed = 0;
    if (full()) {
        if (dropped_msg_cb) {
            auto &dropped_msg = m_data[m_tail];
            dropped_msg_cb(dropped_msg.storage_time, dropped_msg.size, dropped_msg.data());
        }
        m_tail = (m_tail + 1) % m_data.size();
        n_removed++;
    }

    auto &elem = m_data[m_head];
    elem.size = size;
    elem.proc_time = time;
    elem.storage_time = time;
    memcpy(elem.bytes, data, size);

    m_head = (m_head + 1) % m_data.size();
    if (m_n_events < m_data.size()) {
        m_n_events++;
    }

    if (n_removed > 0) {
        log<log_level_debug_trace>("append: removed {} messages to make space", n_removed);
    }
    return true;
}

bool MidiStorage::append(uint32_t time, uint16_t size,
                         const uint8_t *data, bool allow_replace,
                         DroppedMsgCallback dropped_msg_cb)
{
    auto tail = MidiStorageBase::m_tail;
    auto rval = MidiStorageBase::append(time, size, data, allow_replace, dropped_msg_cb);
    if (rval && MidiStorageBase::m_tail != tail) {
        for (auto &c: m_cursors) {
            if(auto cc = c.lock()) {
                if (cc->offset() == tail) { cc->reset(); }
            }
        }
    }
    return rval;
}

bool MidiStorageBase::prepend(uint32_t time, uint16_t size,
                              const uint8_t *data) {
    if (full()) {
        log<log_level_debug_trace>("Prepend: buffer full");
        return false;
    }
    if (m_n_events > 0 && m_data[m_tail].get_time() < time) {
        // Don't store out-of-order messages
        log<log_level_warning>("Ignoring store of out-of-order MIDI message.");
        return false;
    }

    uint32_t new_tail = (m_tail + m_data.size() - 1) % m_data.size();
    m_tail = new_tail;
    m_n_events++;

    auto &elem = m_data[m_tail];
    elem.size = size;
    elem.proc_time = time;
    elem.storage_time = time;
    memcpy(elem.bytes, data, size);

    return true;
}

void MidiStorageBase::copy(MidiStorageBase &to) const {
    to.m_data.resize(m_data.size());
    to.m_tail = 0;
    to.m_n_events = m_n_events;

    if (m_n_events == 0) {
        to.m_head = 0;
        return;
    }

    uint32_t count = 0;
    uint32_t idx = m_tail;
    while (count < m_n_events) {
        to.m_data[count] = m_data[idx];
        idx = (idx + 1) % m_data.size();
        count++;
    }
    to.m_head = count % m_data.size();
}

MidiStorageCursor::MidiStorageCursor(
    shoop_shared_ptr<const Storage> _storage)
    : m_storage(_storage) {}

bool MidiStorageCursor::valid() const {
    return m_offset.has_value();
}

std::optional<uint32_t> MidiStorageCursor::offset() const {
    return m_offset;
}

std::optional<uint32_t>
MidiStorageCursor::prev_offset() const {
    return m_prev_offset;
}

void MidiStorageCursor::invalidate() {
    m_offset.reset();
    m_prev_offset.reset();
}

bool MidiStorageCursor::is_at_start() const {
    return offset() == m_storage->m_tail;
}

void MidiStorageCursor::overwrite(uint32_t offset,
                                                      uint32_t prev_offset) {
    m_offset = offset;
    m_prev_offset = prev_offset;
}

void MidiStorageCursor::reset() {
    if (m_storage->m_n_events == 0) {
        log<log_level_debug_trace>("reset: no events, invalidating");
        invalidate();
    } else {
        log<log_level_debug_trace>("reset: resetting to tail {}", m_storage->m_tail);
        m_offset = m_storage->m_tail;
        m_prev_offset = std::nullopt;
    }
}

typename MidiStorageCursor::Elem *
MidiStorageCursor::get(uint32_t raw_offset) const {
    return &m_storage->m_data.at(raw_offset);
}

typename MidiStorageCursor::Elem *
MidiStorageCursor::get() const {
    return m_offset.has_value() ? get(m_offset.value()) : nullptr;
}

typename MidiStorageCursor::Elem *
MidiStorageCursor::get_prev() const {
    return m_prev_offset.has_value() ? get(m_prev_offset.value()) : nullptr;
}

void MidiStorageCursor::next() {
    if (!m_offset.has_value()) { return; }
    auto const& storage = *m_storage;
    uint32_t cap = storage.m_data.size();
    if (cap == 0) { invalidate(); return; }

    uint32_t next = (m_offset.value() + 1) % cap;

    if (cap == 1) {
        invalidate();
        return;
    }

    if (!storage.full() && next == storage.m_head) {
        invalidate();
        return;
    }

    if (storage.full() && next == storage.m_tail && m_prev_offset.has_value()) {
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

CursorFindResult MidiStorageCursor::find_time_forward(
    uint32_t time, std::function<void(Elem *)> maybe_skip_msg_callback)
{
    auto print_offset = m_offset.has_value() ? (int)m_offset.value() : (int)-1;
    auto storage_ptr = m_storage.get();
    log<log_level_debug_trace>("find_time_forward (storage {}, cursor {}, target time {})", fmt::ptr(storage_ptr), print_offset, time);
    return find_fn_forward([time](Elem *e) {
        return e->storage_time >= time;
    }, maybe_skip_msg_callback);
}

CursorFindResult MidiStorageCursor::find_fn_forward(
    std::function<bool(Elem *)> fn, std::function<void(Elem *)> maybe_skip_msg_callback)
{
    CursorFindResult rval;
    rval.found_valid_elem = false;
    rval.n_processed = 0;

    auto print_offset = m_offset.has_value() ? (int)m_offset.value() : (int)-1;
    log<log_level_debug_trace>("find_fn_forward (storage {}, cursor {})", fmt::ptr(m_storage.get()), print_offset);
    if (!valid()) {
        log<log_level_debug_trace>("find_fn_forward: not valid, returning");
        return rval;
    }

    auto const& storage = *m_storage;
    uint32_t cap = storage.m_data.size();
    if (cap == 0) {
        invalidate();
        return rval;
    }

    uint32_t idx = m_offset.value();
    uint32_t prev_idx = idx;
    for (uint32_t step = 0; step < storage.m_n_events; ++step) {
        if (step > 0) {
            if (!storage.full() && idx == storage.m_head) { break; }
            if (storage.full() && idx == storage.m_tail) { break; }
        }

        Elem *elem = &storage.m_data.at(idx);
        if (fn(elem)) {
            if (step > 0) {
                m_prev_offset = prev_idx;
                m_offset = idx;
            }
            rval.found_valid_elem = true;
            return rval;
        }

        if (maybe_skip_msg_callback) {
            log<log_level_debug_trace>("Skip event @ {}", elem->storage_time);
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

MidiStorage::MidiStorage(uint32_t data_size)
    : MidiStorageBase(data_size) {
    m_cursors.reserve(n_starting_cursors);
}

typename MidiStorage::SharedCursor
MidiStorage::create_cursor() {
    auto maybe_self = MidiStorageBase::weak_from_this();
    if (auto self = maybe_self.lock()) {
        auto rval = shoop_make_shared<Cursor>(shoop_static_pointer_cast<const MidiStorageBase>(self));
        m_cursors.push_back(rval);
        rval->reset();
        return rval;
    } else {
        throw std::runtime_error(
            "Attempting to create cursor for destructed storage");
    }
}

void MidiStorage::clear() {
    for (auto &elem : m_cursors) {
        if (auto c = elem.lock()) {
            c->invalidate();
        }
    }
    m_cursors.clear();
    MidiStorageBase::clear();
}

void MidiStorage::truncate(uint32_t time, TruncateSide type, DroppedMsgCallback dropped_msg_cb) {
    ModuleLoggingEnabled<"Backend.MidiStorage">::log<log_level_debug_trace>("truncate to {}", time);
    if (type == TruncateSide::TruncateTail) {
        return truncate_fn([time](uint32_t t, uint16_t size, const uint8_t* data){ return t < time; }, type, dropped_msg_cb);
    } else if (type == TruncateSide::TruncateHead) {
        return truncate_fn([time](uint32_t t, uint16_t size, const uint8_t* data){ return t > time; }, type, dropped_msg_cb);
    }
}

void MidiStorage::truncate_fn(std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
                  TruncateSide type, DroppedMsgCallback dropped_msg_cb) {
    ModuleLoggingEnabled<"Backend.MidiStorage">::log<log_level_debug_trace>("truncate to function");
    auto prev_n_events = this->m_n_events;
    uint32_t cap = m_data.size();
    if (cap == 0 || m_n_events == 0) {
        return;
    }

    if (type == TruncateSide::TruncateHead) {
        uint32_t newest_idx = (m_head + cap - 1) % cap;
        auto &e = m_data[newest_idx];
        if (!should_truncate_fn(e.storage_time, e.size, e.data())) {
            this->template log<log_level_debug_trace>("truncate: head unchanged");
            return;
        }

        uint32_t idx = m_tail;
        uint32_t kept = 0;
        for (uint32_t i = 0; i < m_n_events; ++i) {
            auto &elem = m_data[idx];
            if (should_truncate_fn(elem.storage_time, elem.size, elem.data())) {
                break;
            }
            kept++;
            idx = (idx + 1) % cap;
        }

        if (dropped_msg_cb) {
            uint32_t drop_idx = idx;
            for (uint32_t i = kept; i < m_n_events; ++i) {
                auto &elem = m_data[drop_idx];
                dropped_msg_cb(elem.storage_time, elem.size, elem.data());
                drop_idx = (drop_idx + 1) % cap;
            }
        }

        this->template log<log_level_debug_trace>("truncate head: {} -> {}, n msgs {} -> {}", m_head, idx, m_n_events, kept);
        m_head = idx;
        m_n_events = kept;

    } else if (type == TruncateSide::TruncateTail) {
        auto &e = m_data[m_tail];
        if (!should_truncate_fn(e.storage_time, e.size, e.data())) {
            this->template log<log_level_debug_trace>("truncate: tail unchanged");
            return;
        }

        uint32_t idx = m_tail;
        uint32_t dropped = 0;
        for (uint32_t i = 0; i < m_n_events; ++i) {
            auto &elem = m_data[idx];
            if (!should_truncate_fn(elem.storage_time, elem.size, elem.data())) {
                break;
            }
            if (dropped_msg_cb) {
                dropped_msg_cb(elem.storage_time, elem.size, elem.data());
            }
            dropped++;
            idx = (idx + 1) % cap;
        }

        this->template log<log_level_debug_trace>("truncate tail: {} -> {}, n msgs {} -> {}", m_tail, idx, m_n_events, m_n_events - dropped);
        m_tail = idx;
        m_n_events -= dropped;
        if (m_n_events == 0) {
            m_head = m_tail;
        }
    }

    // Invalidate cursors that no longer point to valid elements
    auto is_valid_idx = [this](uint32_t idx) -> bool {
        if (m_n_events == 0) return false;
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

    ModuleLoggingEnabled<"Backend.MidiStorage">::log<log_level_debug_trace>("truncate: was {}, now {} msgs", prev_n_events, this->m_n_events);
}

void MidiStorage::for_each_msg_modify(
    std::function<void(uint32_t &t, uint16_t &s, uint8_t *data)> cb) {
    if (m_data.empty()) return;
    uint32_t idx = m_tail;
    for (uint32_t i = 0; i < m_n_events; ++i) {
        auto &elem = m_data[idx];
        cb(elem.storage_time, elem.size, elem.bytes);
        idx = (idx + 1) % m_data.size();
    }
}

void MidiStorage::for_each_msg(
    std::function<void(uint32_t t, uint16_t s, uint8_t *data)> cb)
{
    for_each_msg_modify([cb](uint32_t &t, uint16_t &s, uint8_t *data) {
        cb(t, s, data);
    });
}