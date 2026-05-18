#include "MidiRingbuffer.h"
#include "RustMidiStorage.h"
#include "MidiStorageCursor.h"
#include "MidiStorageElem.h"
#include "IMidiStorageCore.h"
#include "backend_rust/src/midi_storage_cxx.rs.h"
#include <stdexcept>
#include <cstring>
#include <cstdio>
#include <array>
#include <cstdio>

using namespace backend_rust;

RustMidiStorage::RustMidiStorage(uint32_t data_size)
    : m_rust_core(new_midi_storage_core(data_size)),
      m_time_window(new_midi_time_window()),
      m_empty_data() {
}

RustMidiStorage::~RustMidiStorage() = default;

::MidiStorageElem* RustMidiStorage::get_elem_physical(uint32_t idx) {
    uint32_t time;
    uint16_t size;
    std::array<uint8_t, 4> bytes{};
    
    if (backend_rust::get_elem_at_physical_offset(
            *m_rust_core, idx, time, size, bytes)) {
        m_elem_buffer.time = time;
        m_elem_buffer.size = size;
        m_elem_buffer.bytes[0] = bytes[0];
        m_elem_buffer.bytes[1] = bytes[1];
        m_elem_buffer.bytes[2] = bytes[2];
        m_elem_buffer.bytes[3] = bytes[3];
        return &m_elem_buffer;
    }
    return nullptr;
}

const ::MidiStorageElem* RustMidiStorage::get_elem_physical(uint32_t idx) const {
    uint32_t time;
    uint16_t size;
    std::array<uint8_t, 4> bytes{};
    
    if (backend_rust::get_elem_at_physical_offset(
            *m_rust_core, idx, time, size, bytes)) {
        m_elem_buffer.time = time;
        m_elem_buffer.size = size;
        m_elem_buffer.bytes[0] = bytes[0];
        m_elem_buffer.bytes[1] = bytes[1];
        m_elem_buffer.bytes[2] = bytes[2];
        m_elem_buffer.bytes[3] = bytes[3];
        return &m_elem_buffer;
    }
    return nullptr;
}

::MidiStorageElem* RustMidiStorage::get_elem_logical(uint32_t idx) {
    uint32_t time;
    uint16_t size;
    std::array<uint8_t, 4> bytes{};
    
    if (backend_rust::get_elem_at_logical_index(
            *m_rust_core, idx, time, size, bytes)) {
        m_elem_buffer.time = time;
        m_elem_buffer.size = size;
        m_elem_buffer.bytes[0] = bytes[0];
        m_elem_buffer.bytes[1] = bytes[1];
        m_elem_buffer.bytes[2] = bytes[2];
        m_elem_buffer.bytes[3] = bytes[3];
        return &m_elem_buffer;
    }
    return nullptr;
}

const ::MidiStorageElem* RustMidiStorage::get_elem_logical(uint32_t idx) const {
    uint32_t time;
    uint16_t size;
    std::array<uint8_t, 4> bytes{};
    
    if (backend_rust::get_elem_at_logical_index(
            *m_rust_core, idx, time, size, bytes)) {
        m_elem_buffer.time = time;
        m_elem_buffer.size = size;
        m_elem_buffer.bytes[0] = bytes[0];
        m_elem_buffer.bytes[1] = bytes[1];
        m_elem_buffer.bytes[2] = bytes[2];
        m_elem_buffer.bytes[3] = bytes[3];
        return &m_elem_buffer;
    }
    return nullptr;
}

bool RustMidiStorage::append(uint32_t time, uint16_t size,
                              const uint8_t* data, bool allow_replace,
                              DroppedMsgCallback dropped_msg_cb) {
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    return backend_rust::append(
        *m_rust_core, time, size, data, allow_replace,
        cb_fn, cb_ctx
    );
}

bool RustMidiStorage::prepend(uint32_t time, uint16_t size,
                               const uint8_t* data,
                               DroppedMsgCallback dropped_msg_cb) {
    (void)dropped_msg_cb;  // prepend doesn't invoke dropped callback
    
    return backend_rust::prepend(
        *m_rust_core, time, size, data, 0, 0
    );
}

void RustMidiStorage::clear() {
    backend_rust::clear_storage(*m_rust_core);
}

void RustMidiStorage::copy(IMidiStorageCore& target) const {
    auto* t = dynamic_cast<RustMidiStorage*>(&target);
    if (t) {
        backend_rust::copy_to_storage(*m_rust_core, *t->m_rust_core);
        return;
    }
    
    throw std::runtime_error("copy target must be RustMidiStorage");
}

void RustMidiStorage::copy_from(const IMidiStorageCore& from) {
    auto* s = dynamic_cast<const RustMidiStorage*>(&from);
    if (s) {
        backend_rust::copy_from_storage(*m_rust_core, *s->m_rust_core);
        return;
    }
    throw std::runtime_error("copy_from source must be RustMidiStorage");
}

void RustMidiStorage::truncate(uint32_t time, MidiStorageTruncateSide side,
                                DroppedMsgCallback dropped_msg_cb) {
    uint8_t rust_side = (side == MidiStorageTruncateSide::TruncateTail) ? 0 : 1;
    
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    backend_rust::truncate(*m_rust_core, time, rust_side, cb_fn, cb_ctx);
}

void RustMidiStorage::truncate_fn(
    std::function<bool(uint32_t, uint16_t, const uint8_t*)> should_truncate_fn,
    MidiStorageTruncateSide side, DroppedMsgCallback dropped_msg_cb) {
    
    uint8_t rust_side = (side == MidiStorageTruncateSide::TruncateTail) ? 0 : 1;
    
    // Store predicate in thread-local for trampoline
    static thread_local std::function<bool(uint32_t, uint16_t, const uint8_t*)>* g_predicate = nullptr;
    g_predicate = &should_truncate_fn;
    
    auto predicate_fn = +[](uint32_t time, uint16_t size, const uint8_t* data, uintptr_t ctx) -> bool {
        (void)ctx;
        if (g_predicate) {
            return (*g_predicate)(time, size, data);
        }
        return false;
    };
    
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    backend_rust::truncate_fn(
        *m_rust_core,
        reinterpret_cast<uintptr_t>(&predicate_fn), 0,
        rust_side, cb_fn, cb_ctx
    );
    
    g_predicate = nullptr;
}

// Global trampoline storage for for_each_msg_modify callbacks
using ForEachModifyCb = std::function<void(uint32_t&, uint16_t&, uint8_t*)>;
static ForEachModifyCb* g_for_each_modify_cb = nullptr;

static void for_each_modify_trampoline(uint32_t* time, uint16_t* size, uint8_t* data, uintptr_t ctx) {
    (void)ctx;
    if (g_for_each_modify_cb && *g_for_each_modify_cb) {
        (*g_for_each_modify_cb)(*time, *size, data);
    }
}

void RustMidiStorage::for_each_msg_modify(
    std::function<void(uint32_t&, uint16_t&, uint8_t*)> cb) {
    g_for_each_modify_cb = &cb;
    
    backend_rust::for_each_msg_modify(
        *m_rust_core,
        reinterpret_cast<uintptr_t>(&for_each_modify_trampoline),
        0
    );
    
    g_for_each_modify_cb = nullptr;
}

// Global trampoline storage for for_each_msg callbacks
using ForEachMsgCb = std::function<void(uint32_t, uint16_t, uint8_t*)>;
static ForEachMsgCb* g_for_each_msg_cb = nullptr;

static void for_each_msg_trampoline(uint32_t* time, uint16_t* size, uint8_t* data, uintptr_t ctx) {
    (void)ctx;
    if (g_for_each_msg_cb && *g_for_each_msg_cb) {
        (*g_for_each_msg_cb)(*time, *size, data);
    }
}

void RustMidiStorage::for_each_msg(
    std::function<void(uint32_t, uint16_t, uint8_t*)> cb) {
    // Store callback in global pointer
    g_for_each_msg_cb = &cb;
    
    backend_rust::for_each_msg_modify(
        *m_rust_core,
        reinterpret_cast<uintptr_t>(&for_each_msg_trampoline),
        0
    );
    
    g_for_each_msg_cb = nullptr;
}

// Cursor creation
shoop_shared_ptr<MidiStorageCursor> 
RustMidiStorage::create_cursor() {
    auto self = shared_from_this();
    shoop_shared_ptr<IMidiStorage> storage_ptr = self;
    auto cursor = shoop_make_shared<MidiStorageCursor>(storage_ptr);
    cursor->reset();
    return cursor;
}

shoop_shared_ptr<void>
RustMidiStorage::create_cursor_shared() {
    return create_cursor();
}

// Time window operations
void RustMidiStorage::set_n_samples(uint32_t n) {
    backend_rust::time_window_set_n_samples(*m_time_window, n);
    // Truncate storage to the new window
    uint32_t end = backend_rust::time_window_get_current_end_time(*m_time_window);
    uint32_t min_time = end - std::min(end, n);
    
    // Use truncate with no callback
    backend_rust::truncate(*m_rust_core, min_time, 0, 0, 0);
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
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    backend_rust::time_window_next_buffer(*m_time_window, *m_rust_core, n_frames, cb_fn, cb_ctx);
}

bool RustMidiStorage::put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data,
                         DroppedMsgCallback dropped_msg_cb) {
    DroppedMsgCallback cb_store = dropped_msg_cb;
    uintptr_t cb_fn = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&dropped_cb_trampoline) : 0;
    uintptr_t cb_ctx = dropped_msg_cb ? reinterpret_cast<uintptr_t>(&cb_store) : 0;
    
    return backend_rust::time_window_put(
        *m_time_window, *m_rust_core, frame_in_current_buffer, size, data,
        cb_fn, cb_ctx
    );
}

void RustMidiStorage::snapshot(RustMidiStorage& target, std::optional<uint32_t> start_offset_from_end) const {
    uint32_t offset = start_offset_from_end.value_or(get_n_samples());
    
    backend_rust::time_window_snapshot(
        *m_time_window, *m_rust_core, *target.m_rust_core, offset
    );
}

// MidiRingbuffer implementation
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
    auto* rust_target = dynamic_cast<RustMidiStorage*>(&target);
    if (rust_target) {
        m_storage->snapshot(*rust_target, start_offset_from_end);
        return;
    }
    throw std::runtime_error("snapshot target must be RustMidiStorage");
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