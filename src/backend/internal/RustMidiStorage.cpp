#include "MidiRingbuffer.h"
#include "RustMidiStorage.h"
#include "MidiStorageCursor.h"  // For MidiStorageCursor
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

static void dropped_cb_trampoline(uint32_t time, uint16_t size, const uint8_t* data, uintptr_t ctx) {
    auto* cb = reinterpret_cast<DroppedMsgCallback*>(ctx);
    if (*cb) {
        (*cb)(time, size, data);
    }
}

::MidiStorageElem* RustMidiStorage::get_elem_physical(uint32_t idx) {
    if (m_data.empty()) return nullptr;
    return &m_data[idx % m_data.size()];
}

const ::MidiStorageElem* RustMidiStorage::get_elem_physical(uint32_t idx) const {
    if (m_data.empty()) return nullptr;
    return &m_data[idx % m_data.size()];
}

::MidiStorageElem* RustMidiStorage::get_elem_logical(uint32_t idx) {
    if (idx >= m_n_events || m_data.empty()) return nullptr;
    uint32_t phys_idx = (m_tail + idx) % m_data.size();
    return &m_data[phys_idx];
}

const ::MidiStorageElem* RustMidiStorage::get_elem_logical(uint32_t idx) const {
    if (idx >= m_n_events || m_data.empty()) return nullptr;
    uint32_t phys_idx = (m_tail + idx) % m_data.size();
    return &m_data[phys_idx];
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
    
    return success;
}

bool RustMidiStorage::prepend(uint32_t time, uint16_t size,
                               const uint8_t* data,
                               DroppedMsgCallback dropped_msg_cb) {
    if (m_data.empty()) return false;
    
    // Prepare callback context (prepend doesn't use dropped callback, but we pass 0)
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = 0;  // prepend doesn't invoke dropped callback
    uintptr_t cb_ctx = 0;
    (void)cb_store;  // Suppress unused warning
    
    // Delegate to Rust - returns bool success
    bool success = backend_rust::prepend(
        *m_rust_core, time, size, data, cb_fn, cb_ctx
    );
    
    // Sync state from Rust to C++
    sync_rust_state();
    
    return success;
}

void RustMidiStorage::clear() {
    // Delegate to Rust
    backend_rust::clear_storage(*m_rust_core);
    
    // Sync C++ state
    sync_rust_state();
}

void RustMidiStorage::copy(IMidiStorageCore& target) const {
    // Handle RustMidiStorage target
    if (auto* t = dynamic_cast<RustMidiStorage*>(&target)) {
        // Use Rust copy operation
        backend_rust::copy_to_storage(*m_rust_core, *t->m_rust_core);
        
        // Sync C++ state
        t->sync_rust_state();
        return;
    }
    
    throw std::runtime_error("copy target must be RustMidiStorage");
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
    
    uint8_t rust_side = (side == MidiStorageTruncateSide::TruncateTail) ? 0 : 1;
    
    // Create a trampoline for the predicate function
    // We need to store the std::function somewhere accessible from the trampoline
    static thread_local std::function<bool(uint32_t, uint16_t, const uint8_t*)>* g_predicate = nullptr;
    
    auto predicate_fn = +[](uint32_t time, uint16_t size, const uint8_t* data, uintptr_t ctx) -> bool {
        if (g_predicate) {
            return (*g_predicate)(time, size, data);
        }
        return false; // Don't drop if no predicate
    };
    
    // Prepare callback context for predicate
    g_predicate = &should_truncate_fn;
    uintptr_t pred_fn = reinterpret_cast<uintptr_t>(&predicate_fn);
    uintptr_t pred_ctx = 0;  // Predicate uses the global
    
    // Prepare callback context for dropped messages
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    // Delegate to Rust - predicate and dropped_cb are invoked for each message
    backend_rust::truncate_fn(*m_rust_core, pred_fn, pred_ctx, rust_side, cb_fn, cb_ctx);
    
    // Clear the static predicate
    g_predicate = nullptr;
    
    // Sync C++ state
    sync_rust_state();
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
    
    // Cast away const: Rust needs mutable reference for its copy operation
    backend_rust::time_window_snapshot(*m_time_window, const_cast<backend_rust::MidiStorageCore&>(*m_rust_core), *target.m_rust_core, offset);
    target.sync_rust_state();
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