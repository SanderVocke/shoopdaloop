#include "MidiStorageCursor.h"
#include "RustMidiStorage.h"
#include "IMidiStorageCore.h"
#include <cstring>
#include <functional>

using namespace backend_rust;

MidiStorageCursor::MidiStorageCursor(shoop_shared_ptr<IMidiStorage> _storage)
    : ModuleLoggingEnabled<"Backend.MidiStorage">(),
      m_rust_cursor(new_midi_cursor()),
      m_storage(_storage) {
    reset();
}

template<typename StorageType>
MidiStorageCursor::MidiStorageCursor(shoop_shared_ptr<StorageType> _storage)
    : ModuleLoggingEnabled<"Backend.MidiStorage">(),
      m_rust_cursor(new_midi_cursor()),
      m_storage(_storage) {
    reset();
}

void MidiStorageCursor::set_storage(shoop_shared_ptr<IMidiStorage> storage) {
    m_storage = storage;
}

bool MidiStorageCursor::valid() const {
    return backend_rust::cursor_valid(*m_rust_cursor);
}

std::optional<uint32_t> MidiStorageCursor::offset() const {
    uint32_t off = backend_rust::cursor_offset(*m_rust_cursor);
    if (off != INVALID_OFFSET) {
        return off;
    }
    return std::nullopt;
}

std::optional<uint32_t> MidiStorageCursor::prev_offset() const {
    uint32_t prev = backend_rust::cursor_prev_offset(*m_rust_cursor);
    if (prev != INVALID_OFFSET) {
        return prev;
    }
    return std::nullopt;
}

void MidiStorageCursor::invalidate() {
    backend_rust::cursor_invalidate(*m_rust_cursor);
}

bool MidiStorageCursor::is_at_start() const {
    // Get the underlying storage to pass to Rust
    if (!m_storage) return false;
    
    // For RustMidiStorage, we can get the Rust core directly
    auto* rust_storage = dynamic_cast<RustMidiStorage*>(m_storage.get());
    if (rust_storage) {
        // Create a temporary cursor on the Rust storage just for is_at_start check
        return backend_rust::cursor_is_at_start(*m_rust_cursor, *rust_storage->m_rust_core);
    }
    
    // Fallback: check if offset == tail and prev_offset is None
    auto off = offset();
    auto prev = prev_offset();
    return off.has_value() && *off == m_storage->raw_tail() && !prev.has_value();
}

void MidiStorageCursor::reset() {
    if (!m_storage) return;
    
    auto* rust_storage = dynamic_cast<RustMidiStorage*>(m_storage.get());
    if (rust_storage) {
        backend_rust::cursor_reset(*m_rust_cursor, *rust_storage->m_rust_core);
        return;
    }
    
    // Fallback: manual reset
    if (m_storage->n_events() == 0) {
        invalidate();
    } else {
        auto tail = m_storage->raw_tail();
        overwrite(tail, INVALID_OFFSET);
    }
}

void MidiStorageCursor::overwrite(uint32_t _offset, uint32_t _prev_offset) {
    backend_rust::cursor_overwrite(*m_rust_cursor, _offset, _prev_offset);
}

void MidiStorageCursor::next() {
    if (!m_storage) return;
    
    auto* rust_storage = dynamic_cast<RustMidiStorage*>(m_storage.get());
    if (rust_storage) {
        backend_rust::cursor_next(*m_rust_cursor, *rust_storage->m_rust_core);
        return;
    }
    
    // Fallback: manual next operation
    if (!valid()) return;
    
    uint32_t cap = m_storage->raw_capacity();
    uint32_t n_events = m_storage->n_events();
    if (cap == 0) { invalidate(); return; }

    auto curr = offset().value();
    uint32_t next = (curr + 1) % cap;

    bool raw_full = m_storage->raw_full();
    uint32_t head = m_storage->raw_head();
    uint32_t tail = m_storage->raw_tail();

    if (cap == 1) {
        invalidate();
        return;
    }

    if (!raw_full && next == head) {
        invalidate();
        return;
    }

    if (raw_full && next == tail && prev_offset().has_value()) {
        invalidate();
        return;
    }

    overwrite(next, curr);
}

bool MidiStorageCursor::wrapped() const {
    return backend_rust::cursor_wrapped(*m_rust_cursor);
}

::MidiStorageElem* MidiStorageCursor::get() {
    auto off = offset();
    if (!off.has_value()) return nullptr;
    return get(*off);
}

const ::MidiStorageElem* MidiStorageCursor::get() const {
    auto off = offset();
    if (!off.has_value()) return nullptr;
    return get(*off);
}

::MidiStorageElem* MidiStorageCursor::get(uint32_t raw_offset) {
    if (!m_storage) return nullptr;
    auto* elem = m_storage->get_elem_physical(raw_offset);
    if (elem) {
    } else {
    }
    return elem;
}

const ::MidiStorageElem* MidiStorageCursor::get(uint32_t raw_offset) const {
    if (!m_storage) return nullptr;
    return m_storage->get_elem_physical(raw_offset);
}

::MidiStorageElem* MidiStorageCursor::get_prev() {
    auto prev = prev_offset();
    if (!prev.has_value()) return nullptr;
    return get(*prev);
}

const ::MidiStorageElem* MidiStorageCursor::get_prev() const {
    auto prev = prev_offset();
    if (!prev.has_value()) return nullptr;
    return get(*prev);
}

MidiStorageCursor::FindResult MidiStorageCursor::find_time_forward(
    uint32_t time, std::function<void(::MidiStorageElem*)> maybe_skip_msg_callback) {
    
    FindResult rval;
    rval.n_processed = 0;
    rval.found_valid_elem = false;

    if (!valid()) {
        return rval;
    }
    
    auto* rust_storage = dynamic_cast<RustMidiStorage*>(m_storage.get());
    if (rust_storage) {
        // Create the find predicate that checks time
        auto pred_fn = +[](uint32_t t, uint16_t s, const uint8_t* d, uintptr_t ctx) -> bool {
            (void)d;
            uint32_t target_time = static_cast<uint32_t>(ctx);
            return t >= target_time;
        };
        
        uintptr_t pred_ctx = time;
        
        if (maybe_skip_msg_callback) {
            // Use the variant that also calls the skip callback
            
            // Create a C callback that will invoke the C++ std::function
            auto skip_fn = +[](uint32_t t, uint16_t s, const uint8_t* d, uintptr_t ctx) {
                (void)d;
                ::MidiStorageElem elem;
                elem.time = t;
                elem.size = s;
                memcpy(elem.bytes, d, s);
                auto* callback = reinterpret_cast<std::function<void(::MidiStorageElem*)>*>(ctx);
                (*callback)(&elem);
            };
            uintptr_t skip_ctx = reinterpret_cast<uintptr_t>(&maybe_skip_msg_callback);
            
            auto result = backend_rust::cursor_find_fn_forward_with_skip(
                *m_rust_cursor, *rust_storage->m_rust_core,
                reinterpret_cast<uintptr_t>(pred_fn), pred_ctx,
                reinterpret_cast<uintptr_t>(skip_fn), skip_ctx
            );
            
            rval.n_processed = backend_rust::find_result_n_processed(*result);
            rval.found_valid_elem = backend_rust::find_result_found_valid_elem(*result);
        } else {
            // No skip callback needed, use the simpler variant
            auto pred_fn = +[](uint32_t t, uint16_t s, const uint8_t* d, uintptr_t ctx) -> bool {
                (void)d;
                uint32_t target_time = static_cast<uint32_t>(ctx);
                return t >= target_time;
            };
            uintptr_t pred_ctx = time;
            auto result = backend_rust::cursor_find_fn_forward(
                *m_rust_cursor, *rust_storage->m_rust_core,
                reinterpret_cast<uintptr_t>(pred_fn), pred_ctx
            );
            
            rval.n_processed = backend_rust::find_result_n_processed(*result);
            rval.found_valid_elem = backend_rust::find_result_found_valid_elem(*result);
        }
        return rval;
    }
    
    // Fallback: manual find_time_forward
    uint32_t cap = m_storage->raw_capacity();
    uint32_t n_events = m_storage->n_events();
    if (cap == 0) {
        invalidate();
        return rval;
    }

    uint32_t head = m_storage->raw_head();
    uint32_t tail = m_storage->raw_tail();
    bool raw_full = m_storage->raw_full();

    uint32_t idx = offset().value();
    uint32_t prev_idx = idx;
    
    auto curr = offset().value();
    auto prev = prev_offset();
    
    for (uint32_t step = 0; step < n_events; ++step) {
        if (step > 0) {
            if (!raw_full && idx == head) { break; }
            if (raw_full && idx == tail) { break; }
        }

        auto elem = m_storage->get_elem_physical(idx);
        if (elem && elem->time >= time) {
            if (step > 0) {
                overwrite(idx, prev_idx);
            }
            rval.found_valid_elem = true;
            return rval;
        }

        if (maybe_skip_msg_callback) {
            maybe_skip_msg_callback(elem);
        }

        prev_idx = idx;
        idx = (idx + 1) % cap;
        rval.n_processed++;
    }

    invalidate();
    return rval;
}

MidiStorageCursor::FindResult MidiStorageCursor::find_fn_forward(
    std::function<bool(::MidiStorageElem*)> fn, std::function<void(::MidiStorageElem*)> maybe_skip_msg_callback) {
    
    FindResult rval;
    rval.n_processed = 0;
    rval.found_valid_elem = false;

    if (!valid()) {
        return rval;
    }
    
    auto* rust_storage = dynamic_cast<RustMidiStorage*>(m_storage.get());
    if (rust_storage) {
        // Store the std::function in a thread-local for the trampoline
        static thread_local std::function<bool(::MidiStorageElem*)>* g_predicate = nullptr;
        g_predicate = &fn;
        
        auto pred_fn = +[](uint32_t time, uint16_t size, const uint8_t* data, uintptr_t ctx) -> bool {
            (void)ctx;
            if (g_predicate) {
                ::MidiStorageElem elem;
                elem.time = time;
                elem.size = size;
                memcpy(elem.bytes, data, size);
                return (*g_predicate)(&elem);
            }
            return false;
        };
        
        auto result = backend_rust::cursor_find_fn_forward(
            *m_rust_cursor, *rust_storage->m_rust_core,
            reinterpret_cast<uintptr_t>(pred_fn), 0
        );
        
        rval.n_processed = backend_rust::find_result_n_processed(*result);
        rval.found_valid_elem = backend_rust::find_result_found_valid_elem(*result);
        
        g_predicate = nullptr;
        return rval;
    }
    
    // Fallback: manual find_fn_forward
    uint32_t cap = m_storage->raw_capacity();
    uint32_t n_events = m_storage->n_events();
    if (cap == 0) {
        invalidate();
        return rval;
    }

    uint32_t head = m_storage->raw_head();
    uint32_t tail = m_storage->raw_tail();
    bool raw_full = m_storage->raw_full();

    uint32_t idx = offset().value();
    uint32_t prev_idx = idx;
    
    for (uint32_t step = 0; step < n_events; ++step) {
        if (step > 0) {
            if (!raw_full && idx == head) { break; }
            if (raw_full && idx == tail) { break; }
        }

        auto elem = m_storage->get_elem_physical(idx);
        if (elem && fn(elem)) {
            if (step > 0) {
                overwrite(idx, prev_idx);
            }
            rval.found_valid_elem = true;
            return rval;
        }

        if (maybe_skip_msg_callback) {
            maybe_skip_msg_callback(elem);
        }

        prev_idx = idx;
        idx = (idx + 1) % cap;
        rval.n_processed++;
    }

    invalidate();
    return rval;
}