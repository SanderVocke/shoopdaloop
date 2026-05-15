#include "MidiRingbuffer.h"
#include "RustMidiStorage.h"
#include "MidiStorage.h"  // For MidiStorage and MidiStorageCursor
#include "MidiStorageElem.h"
#include "IMidiStorageCore.h"
#include "backend_rust/src/midi_storage_cxx.rs.h"
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
        dst.time = backend_rust::get_elem_time_at_physical_offset(*m_rust_core, pos);
        dst.size = backend_rust::get_elem_size_at_physical_offset(*m_rust_core, pos);
        backend_rust::get_elem_bytes_at_physical_offset(*m_rust_core, pos, dst.bytes, 4);
        pos = (pos + 1) % cap;
    }
}

// Debug flag - set to 1 to enable debug prints, 0 to disable
#define DEBUG_MIDI_STORAGE 1

#if DEBUG_MIDI_STORAGE
#include <iostream>
#define DEBUG_PRINT(msg) std::cerr << "[DEBUG] " << msg << std::endl
#else
#define DEBUG_PRINT(msg) 
#endif

::MidiStorageElem* RustMidiStorage::get_elem_at_physical_offset(uint32_t idx) {
    DEBUG_PRINT("RustMidiStorage::get_elem_at_physical_offset(phys=" << idx << ") n_events=" << m_n_events << ", cap=" << m_data.size());
    if (m_data.empty()) return nullptr;
    return &m_data[idx % m_data.size()];
}

const ::MidiStorageElem* RustMidiStorage::get_elem_at_physical_offset(uint32_t idx) const {
    if (m_data.empty()) return nullptr;
    return &m_data[idx % m_data.size()];
}

::MidiStorageElem* RustMidiStorage::get_elem_logical(uint32_t idx) {
    DEBUG_PRINT("RustMidiStorage::get_elem_logical(logical=" << idx << ") n_events=" << m_n_events);
    if (idx >= m_n_events || m_data.empty()) return nullptr;
    uint32_t phys_idx = (m_tail + idx) % m_data.size();
    return &m_data[phys_idx];
}

const ::MidiStorageElem* RustMidiStorage::get_elem_logical(uint32_t idx) const {
    if (idx >= m_n_events || m_data.empty()) return nullptr;
    uint32_t phys_idx = (m_tail + idx) % m_data.size();
    return &m_data[phys_idx];
}

// Static trampoline function that forwards to the stored std::function
static void dropped_cb_trampoline(uint32_t time, uint16_t size, const uint8_t* data, uintptr_t ctx) {
    auto* cb = reinterpret_cast<DroppedMsgCallback*>(ctx);
    if (*cb) {
        (*cb)(time, size, data);
    }
}

bool RustMidiStorage::append(uint32_t time, uint16_t size,
                              const uint8_t* data, bool allow_replace,
                              DroppedMsgCallback dropped_msg_cb) {
    if (m_data.empty()) return false;
    
    // Prepare callback context
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    // Delegate to Rust - returns bool success
    bool success = backend_rust::append(
        *m_rust_core, time, size, data, allow_replace,
        cb_fn, cb_ctx
    );
    
    // Sync state from Rust to C++
    sync_rust_state();
    
    DEBUG_PRINT("RustMidiStorage::append: time=" << time << ", size=" << size << ", success=" << success);
    DEBUG_PRINT("  after sync: n_events=" << m_n_events << ", tail=" << m_tail << ", head=" << m_head);
    DEBUG_PRINT("  data contents:");
    for (uint32_t i = 0; i < m_n_events; ++i) {
        uint32_t phys = (m_tail + i) % m_data.size();
        DEBUG_PRINT("    [" << i << "] phys=" << phys << ": t=" << m_data[phys].time << ", s=" << m_data[phys].size);
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
        
        // Copy elements by iterating physically from tail
        uint32_t n = m_rust_core->n_events();
        
        DEBUG_PRINT("  n=" << n << ", m_tail=" << m_tail);
        
        uint32_t pos = m_tail;
        for (uint32_t i = 0; i < n; ++i) {
            uint32_t time = backend_rust::get_elem_time_at_physical_offset(*m_rust_core, pos);
            uint16_t size = backend_rust::get_elem_size_at_physical_offset(*m_rust_core, pos);
            uint8_t bytes[4];
            backend_rust::get_elem_bytes_at_physical_offset(*m_rust_core, pos, bytes, 4);
            DEBUG_PRINT("  copying elem[" << i << "] from pos=" << pos << ": t=" << time);
            t->append(time, size, bytes, true, nullptr);
            pos = (pos + 1) % m_data.size();
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
    uint8_t rust_side = (side == MidiStorageTruncateSide::TruncateTail) ? 0 : 1;
    
    // Prepare callback context
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    // Delegate to Rust - callback is invoked for each dropped message
    backend_rust::truncate(*m_rust_core, time, rust_side, cb_fn, cb_ctx);
    
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
    
    // Use atomic truncate with no callback (we don't report drops from set_n_samples)
    backend_rust::truncate(*m_rust_core, min_time, 0, 0, 0);
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
    // Prepare callback context
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    // Delegate to Rust - callback is invoked for each dropped message
    backend_rust::time_window_next_buffer(*m_time_window, *m_rust_core, n_frames, cb_fn, cb_ctx);
    
    // Sync C++ state
    sync_rust_state();
}

bool RustMidiStorage::put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data,
                         DroppedMsgCallback dropped_msg_cb) {
    // Prepare callback context
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    // Delegate to Rust
    bool success = backend_rust::time_window_put(
        *m_time_window, *m_rust_core, frame_in_current_buffer, size, data,
        cb_fn, cb_ctx
    );
    
    // Sync state
    sync_rust_state();
    
    return success;
}

void RustMidiStorage::snapshot(RustMidiStorage& target, std::optional<uint32_t> start_offset_from_end) const {
    uint32_t offset = start_offset_from_end.value_or(get_n_samples());
    std::cerr << "[C++ DEBUG] RustMidiStorage::snapshot called, offset=" << offset << std::endl;
    std::cerr << "[C++ DEBUG]   this n_events=" << n_events() << ", tail=" << raw_tail() << ", head=" << raw_head() << std::endl;
    std::cerr << "[C++ DEBUG]   target n_events before=" << target.n_events() << std::endl;
    
    // Print all messages in this (source) storage
    std::cerr << "[C++ DEBUG]   Messages in source (this):" << std::endl;
    for (uint32_t i = 0; i < n_events(); ++i) {
        uint32_t phys = (raw_tail() + i) % raw_capacity();
        auto* elem = get_elem_at_physical_offset(phys);
        if (elem) {
            std::cerr << "[C++ DEBUG]     logical=" << i << ", phys=" << phys << ", time=" << elem->time << ", size=" << elem->size << std::endl;
        }
    }
    
    // Cast away const: Rust needs mutable reference for its copy operation
    backend_rust::time_window_snapshot(*m_time_window, const_cast<backend_rust::MidiStorageCore&>(*m_rust_core), *target.m_rust_core, offset);
    target.sync_rust_state();
    
    std::cerr << "[C++ DEBUG]   target n_events after=" << target.n_events() << std::endl;
    
    // Print all messages in target after snapshot
    std::cerr << "[C++ DEBUG]   Messages in target after snapshot:" << std::endl;
    for (uint32_t i = 0; i < target.n_events(); ++i) {
        uint32_t phys = (target.raw_tail() + i) % target.raw_capacity();
        auto* elem = target.get_elem_at_physical_offset(phys);
        if (elem) {
            std::cerr << "[C++ DEBUG]     logical=" << i << ", phys=" << phys << ", time=" << elem->time << ", size=" << elem->size << std::endl;
        }
    }
}
// MidiRingbuffer implementation - thin wrapper around Rust storage
// All core logic is handled by Rust (MidiTimeWindow + MidiStorageCore)

MidiRingbuffer::MidiRingbuffer(uint32_t data_size)
    : m_storage(shoop_make_shared<RustMidiStorage>(data_size))
{}

void MidiRingbuffer::set_n_samples(uint32_t n) {
    m_storage->set_n_samples(n);
}

uint32_t MidiRingbuffer::get_n_samples() const {
    return m_storage->get_n_samples();
}

uint32_t MidiRingbuffer::get_current_start_time() const {
    return m_storage->get_current_start_time();
}

uint32_t MidiRingbuffer::get_current_end_time() const {
    return m_storage->get_current_end_time();
}

void MidiRingbuffer::next_buffer(uint32_t n_frames, DroppedMsgCallback dropped_msg_cb) {
    m_storage->next_buffer(n_frames, dropped_msg_cb);
}

bool MidiRingbuffer::put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data, DroppedMsgCallback dropped_msg_cb) {
    return m_storage->put(frame_in_current_buffer, size, data, dropped_msg_cb);
}

void MidiRingbuffer::snapshot(IMidiStorage &target, std::optional<uint32_t> start_offset_from_end) const {
    // Handle RustMidiStorage target directly (efficient)
    if (auto* rust_target = dynamic_cast<RustMidiStorage*>(&target)) {
        m_storage->snapshot(*rust_target, start_offset_from_end);
        return;
    }
    // Handle MidiStorage target via Rust snapshot then copy
    auto temp = shoop_make_shared<RustMidiStorage>(target.bytes_capacity());
    m_storage->snapshot(*temp, start_offset_from_end);
    target.clear();
    temp->copy(target);
}

IMidiStorage& MidiRingbuffer::storage() {
    return *m_storage;
}

const IMidiStorage& MidiRingbuffer::storage() const {
    return *m_storage;
}

shoop_shared_ptr<MidiStorageCursor> MidiRingbuffer::create_cursor() {
    return m_storage->create_cursor();
}
