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
      m_time_window(new_midi_time_window()),
      m_data(data_size / sizeof(Elem)),
      m_tail(0),
      m_head(0),
      m_n_events(0) {
}

RustMidiStorage::~RustMidiStorage() = default;

// Sync Rust core state with C++ state (for future use if we need to query Rust)
void RustMidiStorage::sync_rust_state() {
    // Query Rust for current state
    m_tail = m_rust_core->raw_tail();
    m_head = m_rust_core->raw_head();
    m_n_events = m_rust_core->n_events();
    
    // Sync data from Rust to C++ for elements that are in use
    uint32_t cap = m_data.size();
    uint32_t pos = m_tail;
    for (uint32_t i = 0; i < m_n_events; ++i) {
        auto& dst = m_data[pos];
        dst.time = backend_rust::get_elem_time(*m_rust_core, pos);
        dst.size = backend_rust::get_elem_size(*m_rust_core, pos);
        backend_rust::get_elem_bytes(*m_rust_core, pos, dst.bytes, 4);
        pos = (pos + 1) % cap;
    }
}

// Debug flag - set to 1 to enable debug prints, 0 to disable
#define DEBUG_MIDI_STORAGE 0

#if DEBUG_MIDI_STORAGE
#include <iostream>
#define DEBUG_PRINT(msg) std::cerr << "[DEBUG] " << msg << std::endl
#else
#define DEBUG_PRINT(msg) 
#endif

::MidiStorageElem* RustMidiStorage::get_elem(uint32_t idx) {
    // Physical indexing: idx is a raw physical index into the data array.
    // Used by cursor which works with raw offsets (tail, head, etc.)
    DEBUG_PRINT("RustMidiStorage::get_elem(phys=" << idx << ") n_events=" << m_n_events << ", cap=" << m_data.size());
    if (m_data.empty()) return nullptr;
    return &m_data[idx % m_data.size()];
}

const ::MidiStorageElem* RustMidiStorage::get_elem(uint32_t idx) const {
    // Physical indexing: idx is a raw physical index into the data array.
    if (m_data.empty()) return nullptr;
    return &m_data[idx % m_data.size()];
}

bool RustMidiStorage::append(uint32_t time, uint16_t size,
                              const uint8_t* data, bool allow_replace,
                              DroppedMsgCallback dropped_msg_cb) {
    if (m_data.empty()) return false;
    
    // Delegate to Rust - returns flags byte: bit 0=success, bit 1=dropped
    uint8_t flags = backend_rust::append(*m_rust_core, time, size, data, allow_replace);
    
    bool success = flags & 1;
    bool dropped = flags & 2;
    
    // Sync state from Rust to C++
    sync_rust_state();
    
    DEBUG_PRINT("RustMidiStorage::append: time=" << time << ", size=" << size << ", success=" << success << ", dropped=" << dropped);
    DEBUG_PRINT("  after sync: n_events=" << m_n_events << ", tail=" << m_tail << ", head=" << m_head);
    DEBUG_PRINT("  data contents:");
    for (uint32_t i = 0; i < m_n_events; ++i) {
        uint32_t phys = (m_tail + i) % m_data.size();
        DEBUG_PRINT("    [" << i << "] phys=" << phys << ": t=" << m_data[phys].time << ", s=" << m_data[phys].size);
    }
    
    // Handle dropped message callback if provided
    if (dropped && dropped_msg_cb) {
        uint32_t dropped_idx = backend_rust::get_last_dropped_elem(*m_rust_core);
        if (dropped_idx != UINT32_MAX) {
            uint32_t time = backend_rust::get_elem_time(*m_rust_core, dropped_idx);
            uint16_t size = backend_rust::get_elem_size(*m_rust_core, dropped_idx);
            uint8_t bytes[4];
            backend_rust::get_elem_bytes(*m_rust_core, dropped_idx, bytes, 4);
            dropped_msg_cb(time, size, bytes);
        }
    }
    
    return success;
}

bool RustMidiStorage::prepend(uint32_t time, uint16_t size,
                               const uint8_t* data,
                               DroppedMsgCallback dropped_msg_cb) {
    if (m_data.empty()) return false;
    
    // For prepend, we'll use C++ implementation since Rust doesn't expose it yet
    // This maintains the existing behavior
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
    // Delegate to Rust
    backend_rust::clear_storage(*m_rust_core);
    
    // Sync C++ state
    sync_rust_state();
}

void RustMidiStorage::copy(IMidiStorageCore& target) const {
    DEBUG_PRINT("RustMidiStorage::copy called, this m_n_events=" << m_n_events << ", m_tail=" << m_tail << ", m_head=" << m_head);
    
    // Handle RustMidiStorage target
    if (auto* t = dynamic_cast<RustMidiStorage*>(&target)) {
        DEBUG_PRINT("  target is RustMidiStorage");
        // Use Rust copy operation
        backend_rust::copy_to_storage(*m_rust_core, *t->m_rust_core);
        
        // Sync C++ state
        t->sync_rust_state();
        return;
    }
    
    // Handle MidiStorage target
    auto* t = dynamic_cast<MidiStorage*>(&target);
    if (t) {
        DEBUG_PRINT("  target is MidiStorage");
        t->clear();
        
        // Copy elements by querying Rust state
        uint32_t n = m_rust_core->n_events();
        uint32_t cap = m_rust_core->capacity();
        
        DEBUG_PRINT("  n=" << n << ", cap=" << cap << ", m_tail=" << m_tail);
        
        uint32_t pos = m_tail;
        for (uint32_t i = 0; i < n; ++i) {
            if (pos >= cap) {
                DEBUG_PRINT("  pos=" << pos << " >= cap, breaking");
                break;
            }
            uint32_t time = backend_rust::get_elem_time(*m_rust_core, pos);
            uint16_t size = backend_rust::get_elem_size(*m_rust_core, pos);
            uint8_t bytes[4];
            backend_rust::get_elem_bytes(*m_rust_core, pos, bytes, 4);
            DEBUG_PRINT("  copying elem[" << i << "] from pos=" << pos << ": t=" << time);
            t->append(time, size, bytes, true, nullptr);
            pos = (pos + 1) % cap;
        }
        DEBUG_PRINT("  done copying, t->n_events()=" << t->n_events());
        return;
    }
    
    throw std::runtime_error("copy target must be RustMidiStorage or MidiStorage");
}

void RustMidiStorage::copy_from(const IMidiStorageCore& from) {
    auto* s = dynamic_cast<const RustMidiStorage*>(&from);
    if (s) {
        // Use Rust copy operation
        backend_rust::copy_from_storage(*m_rust_core, *s->m_rust_core);
        
        // Sync C++ state
        sync_rust_state();
    } else {
        throw std::runtime_error("copy_from source must be RustMidiStorage");
    }
}

void RustMidiStorage::truncate(uint32_t time, MidiStorageTruncateSide side,
                                DroppedMsgCallback dropped_msg_cb) {
    // Use the preview-then-act pattern:
    // 1. Get preview of what would be dropped
    // 2. Handle dropped messages
    // 3. Perform actual truncate
    
    uint8_t rust_side = (side == MidiStorageTruncateSide::TruncateTail) ? 0 : 1;
    
    // Get preview
    uint32_t count = backend_rust::truncate_preview(*m_rust_core, time, rust_side);
    
    // Handle dropped messages if any
    for (uint32_t i = 0; i < count; ++i) {
        if (dropped_msg_cb) {
            uint32_t time = backend_rust::get_preview_elem_time(*m_rust_core, i);
            uint16_t size = backend_rust::get_preview_elem_size(*m_rust_core, i);
            uint8_t bytes[4];
            backend_rust::get_preview_elem_bytes(*m_rust_core, i, bytes, 4);
            dropped_msg_cb(time, size, bytes);
        }
    }
    
    // Perform actual truncate
    backend_rust::truncate_doit(*m_rust_core, time, rust_side);
    
    // Sync C++ state
    sync_rust_state();
}

void RustMidiStorage::truncate_fn(
    std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
    MidiStorageTruncateSide side, DroppedMsgCallback dropped_msg_cb) {
    
    // For truncate_fn with predicate, use C++ implementation
    // since Rust's truncate doesn't support custom predicates yet
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

// Time window operations
void RustMidiStorage::set_n_samples(uint32_t n) {
    backend_rust::time_window_set_n_samples(*m_time_window, n);
    // Truncate storage to the new window
    uint32_t end = backend_rust::time_window_get_current_end_time(*m_time_window);
    uint32_t min_time = end - std::min(end, n);
    
    uint8_t rust_side = 0; // TruncateTail
    backend_rust::truncate_preview(*m_rust_core, min_time, rust_side);
    backend_rust::truncate_doit(*m_rust_core, min_time, rust_side);
    sync_rust_state();
}

uint32_t RustMidiStorage::get_n_samples() const {
    return backend_rust::time_window_get_n_samples(*m_time_window);
}

uint32_t RustMidiStorage::get_current_start_time() const {
    return backend_rust::time_window_get_current_start_time(*m_time_window);
}

uint32_t RustMidiStorage::get_current_end_time() const {
    return backend_rust::time_window_get_current_end_time(*m_time_window);
}

void RustMidiStorage::next_buffer(uint32_t n_frames, DroppedMsgCallback dropped_msg_cb) {
    // Use preview-then-act pattern
    uint32_t count = backend_rust::time_window_next_buffer_preview(*m_time_window, *m_rust_core, n_frames);
    
    // Handle dropped messages
    for (uint32_t i = 0; i < count; ++i) {
        if (dropped_msg_cb) {
            uint32_t time = backend_rust::time_window_get_preview_elem_time(*m_time_window, i);
            uint16_t size = backend_rust::time_window_get_preview_elem_size(*m_time_window, i);
            uint8_t bytes[4];
            backend_rust::time_window_get_preview_elem_bytes(*m_time_window, i, bytes, 4);
            dropped_msg_cb(time, size, bytes);
        }
    }
    
    // Perform actual next_buffer
    backend_rust::time_window_next_buffer_doit(*m_time_window, *m_rust_core, n_frames);
    
    // Sync C++ state
    sync_rust_state();
}

bool RustMidiStorage::put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data,
                         DroppedMsgCallback dropped_msg_cb) {
    // Delegate to Rust time_window_put
    uint8_t flags = backend_rust::time_window_put(*m_time_window, *m_rust_core, frame_in_current_buffer, size, data);
    
    bool success = flags & 1;
    bool dropped = flags & 2;
    
    // Sync state
    sync_rust_state();
    
    // Handle dropped message
    if (dropped && dropped_msg_cb) {
        uint32_t dropped_idx = backend_rust::get_last_dropped_elem(*m_rust_core);
        if (dropped_idx != UINT32_MAX) {
            uint32_t time = backend_rust::get_elem_time(*m_rust_core, dropped_idx);
            uint16_t size = backend_rust::get_elem_size(*m_rust_core, dropped_idx);
            uint8_t bytes[4];
            backend_rust::get_elem_bytes(*m_rust_core, dropped_idx, bytes, 4);
            dropped_msg_cb(time, size, bytes);
        }
    }
    
    return success;
}

void RustMidiStorage::snapshot(RustMidiStorage& target, std::optional<uint32_t> start_offset_from_end) const {
    uint32_t offset = start_offset_from_end.value_or(get_n_samples());
    backend_rust::time_window_snapshot(*m_time_window, *m_rust_core, *target.m_rust_core, offset);
    target.sync_rust_state();
}