#include "MidiStorageCursor.h"

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

    if (!valid()) {
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
            maybe_skip_msg_callback(elem);
        }

        prev_idx = idx;
        idx = (idx + 1) % cap;
        rval.n_processed++;
    }

    invalidate();
    return rval;
}