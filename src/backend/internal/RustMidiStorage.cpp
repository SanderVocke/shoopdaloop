#include "RustMidiStorage.h"
#include "MidiStorage.h"
#include "MidiStorageElem.h"
#include "IMidiStorageCore.h"
#include <stdexcept>
#include <cstring>
#include <array>
#include <vector>

using namespace backend_rust;

RustMidiStorage::RustMidiStorage(uint32_t data_size)
    : m_rust_core(new_midi_storage_core(data_size)),
      m_data(data_size / sizeof(Elem)),
      m_tail(0),
      m_head(0),
      m_n_events(0) {
}

RustMidiStorage::~RustMidiStorage() = default;

// Sync Rust core state with C++ state (for future use if we need to query Rust)
void RustMidiStorage::sync_rust_state() {
    // Currently we use C++ state for queries, so no sync needed
    // This function exists for future use if we need to update Rust state
}

::MidiStorageElem* RustMidiStorage::get_elem(uint32_t idx) {
    // Use physical indexing: logical idx maps to (tail + idx) % capacity
    if (m_data.empty() || idx >= m_n_events) return nullptr;
    uint32_t physical_idx = (m_tail + idx) % m_data.size();
    return &m_data[physical_idx];
}

const ::MidiStorageElem* RustMidiStorage::get_elem(uint32_t idx) const {
    // Use physical indexing: logical idx maps to (tail + idx) % capacity
    if (m_data.empty() || idx >= m_n_events) return nullptr;
    uint32_t physical_idx = (m_tail + idx) % m_data.size();
    return &m_data[physical_idx];
}

bool RustMidiStorage::append(uint32_t time, uint16_t size,
                              const uint8_t* data, bool allow_replace,
                              DroppedMsgCallback dropped_msg_cb) {
    if (m_data.empty()) return false;
    
    if (full() && !allow_replace) {
        return false;
    }
    
    if (m_n_events > 0) {
        uint32_t newest_idx = (m_head + m_data.size() - 1) % m_data.size();
        if (m_data[newest_idx].time > time) {
            return false;
        }
    }

    if (full()) {
        if (dropped_msg_cb) {
            auto& dropped = m_data[m_tail];
            dropped_msg_cb(dropped.time, dropped.size, dropped.data());
        }
        m_tail = (m_tail + 1) % m_data.size();
    }

    auto& elem = m_data[m_head];
    elem.time = time;
    elem.size = size;
    memcpy(elem.bytes, data, size);

    m_head = (m_head + 1) % m_data.size();
    if (m_n_events < m_data.size()) {
        m_n_events++;
    }

    return true;
}

bool RustMidiStorage::prepend(uint32_t time, uint16_t size,
                               const uint8_t* data,
                               DroppedMsgCallback dropped_msg_cb) {
    if (m_data.empty()) return false;
    
    if (full()) {
        if (dropped_msg_cb) {
            dropped_msg_cb(time, size, data);
        }
        return false;
    }

    if (m_n_events > 0 && m_data[m_tail].time < time) {
        return false;
    }

    m_tail = (m_tail + m_data.size() - 1) % m_data.size();
    m_n_events++;

    auto& elem = m_data[m_tail];
    elem.time = time;
    elem.size = size;
    memcpy(elem.bytes, data, size);

    return true;
}

void RustMidiStorage::clear() {
    m_head = m_tail = m_n_events = 0;
}

void RustMidiStorage::copy(IMidiStorageCore& target) const {
    // Handle RustMidiStorage target
    if (auto* t = dynamic_cast<RustMidiStorage*>(&target)) {
        t->m_data.resize(m_data.size());
        t->m_tail = 0;
        t->m_n_events = m_n_events;

        if (m_n_events == 0) {
            t->m_head = 0;
            return;
        }

        uint32_t count = 0;
        uint32_t idx = m_tail;
        while (count < m_n_events) {
            t->m_data[count] = m_data[idx];
            idx = (idx + 1) % m_data.size();
            count++;
        }
        t->m_head = count % m_data.size();
        return;
    }
    
    // Handle MidiStorage target - use copy_from by swapping const_cast approach
    // or delegate to the non-const version
    auto* t = dynamic_cast<MidiStorage*>(&target);
    if (t) {
        t->clear();
        
        // Copy elements manually using ringbuffer traversal
        uint32_t cap = t->bytes_capacity() / sizeof(::MidiStorageElem);
        uint32_t idx = 0;
        
        uint32_t pos = m_tail;
        for (uint32_t i = 0; i < m_n_events; ++i) {
            if (idx >= cap) break;
            const auto& elem = m_data[pos];
            t->append(elem.time, elem.size, elem.bytes, true, nullptr);
            pos = (pos + 1) % m_data.size();
            ++idx;
        }
        return;
    }
    
    throw std::runtime_error("copy target must be RustMidiStorage or MidiStorage");
}

void RustMidiStorage::copy_from(const IMidiStorageCore& from) {
    auto* s = dynamic_cast<const RustMidiStorage*>(&from);
    if (!s) {
        throw std::runtime_error("copy_from source must be RustMidiStorage");
    }
    
    m_data.resize(s->m_data.size());
    m_tail = s->m_tail;
    m_head = s->m_head;
    m_n_events = s->m_n_events;

    if (m_n_events == 0) return;

    uint32_t count = 0;
    uint32_t idx = s->m_tail;
    while (count < m_n_events) {
        m_data[idx] = s->m_data[idx];
        idx = (idx + 1) % s->m_data.size();
        count++;
    }
}

void RustMidiStorage::truncate(uint32_t time, MidiStorageTruncateSide side,
                                DroppedMsgCallback dropped_msg_cb) {
    if (side == MidiStorageTruncateSide::TruncateTail) {
        truncate_fn([time](uint32_t t, uint16_t, const uint8_t*) { return t < time; },
                    side, dropped_msg_cb);
    } else {
        truncate_fn([time](uint32_t t, uint16_t, const uint8_t*) { return t > time; },
                    side, dropped_msg_cb);
    }
}

void RustMidiStorage::truncate_fn(
    std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
    MidiStorageTruncateSide side, DroppedMsgCallback dropped_msg_cb) {
    
    uint32_t cap = m_data.size();
    if (cap == 0 || m_n_events == 0) return;

    if (side == MidiStorageTruncateSide::TruncateHead) {
        uint32_t newest_idx = (m_head + cap - 1) % cap;
        auto& e = m_data[newest_idx];
        if (!should_truncate_fn(e.time, e.size, e.data())) return;

        uint32_t idx = m_tail;
        uint32_t kept = 0;
        for (uint32_t i = 0; i < m_n_events; ++i) {
            auto& elem = m_data[idx];
            if (should_truncate_fn(elem.time, elem.size, elem.data())) break;
            kept++;
            idx = (idx + 1) % cap;
        }

        if (dropped_msg_cb) {
            uint32_t drop_idx = idx;
            for (uint32_t i = kept; i < m_n_events; ++i) {
                auto& elem = m_data[drop_idx];
                dropped_msg_cb(elem.time, elem.size, elem.data());
                drop_idx = (drop_idx + 1) % cap;
            }
        }

        m_head = idx;
        m_n_events = kept;

    } else if (side == MidiStorageTruncateSide::TruncateTail) {
        auto& e = m_data[m_tail];
        if (!should_truncate_fn(e.time, e.size, e.data())) return;

        uint32_t idx = m_tail;
        uint32_t dropped = 0;
        for (uint32_t i = 0; i < m_n_events; ++i) {
            auto& elem = m_data[idx];
            if (!should_truncate_fn(elem.time, elem.size, elem.data())) break;
            if (dropped_msg_cb) {
                dropped_msg_cb(elem.time, elem.size, elem.data());
            }
            dropped++;
            idx = (idx + 1) % cap;
        }

        m_tail = idx;
        m_n_events -= dropped;
        if (m_n_events == 0) m_head = m_tail;
    }
}

void RustMidiStorage::for_each_msg_modify(
    std::function<void(uint32_t&, uint16_t&, uint8_t*)> cb) {
    if (m_data.empty()) return;
    uint32_t idx = m_tail;
    for (uint32_t i = 0; i < m_n_events; ++i) {
        auto& elem = m_data[idx];
        cb(elem.time, elem.size, elem.bytes);
        idx = (idx + 1) % m_data.size();
    }
}

void RustMidiStorage::for_each_msg(
    std::function<void(uint32_t, uint16_t, uint8_t*)> cb) {
    for_each_msg_modify([cb](uint32_t& t, uint16_t& s, uint8_t* data) {
        cb(t, s, data);
    });
}

// Cursor creation - uses shared_from_this pattern
shoop_shared_ptr<MidiStorageCursor> 
RustMidiStorage::create_cursor() {
    auto self = shared_from_this();
    // Convert to IMidiStorage shared_ptr for cursor
    shoop_shared_ptr<IMidiStorage> storage_ptr = self;
    auto cursor = shoop_make_shared<MidiStorageCursor>(storage_ptr);
    cursor->reset();  // Initialize cursor to valid position
    return cursor;
}

shoop_shared_ptr<void>
RustMidiStorage::create_cursor_shared() {
    return create_cursor();
}

// Note: shared_from_this() is inherited from shoop_enable_shared_from_this<RustMidiStorage>