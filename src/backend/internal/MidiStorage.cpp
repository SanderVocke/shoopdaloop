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

uint32_t MidiStorageElem::total_size_of(uint32_t data_bytes) {
    return sizeof(MidiStorageElem) + data_bytes;
}

uint8_t *MidiStorageElem::data() const {
    return ((uint8_t *)this) + sizeof(MidiStorageElem);
}

const uint8_t *MidiStorageElem::get_data() const {
    return ((uint8_t *)this) + sizeof(MidiStorageElem);
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

bool MidiStorageBase::valid_elem_at(uint32_t offset) const {
    return m_n_events > 0 &&
           (m_head > m_tail ? (offset >= m_tail && offset < m_head)
                            : (offset < m_head || offset >= m_tail));
}

std::optional<uint32_t>
MidiStorageBase::maybe_next_elem_offset(Elem *e) const {
    if (!e) {
        return std::nullopt;
    }
    uint32_t m_data_offset = (uint8_t *)e - (uint8_t *)m_data.data();
    uint32_t next_m_data_offset =
        (m_data_offset + e->offset_to_next) % m_data.size();

    if (!valid_elem_at(next_m_data_offset)) {
        return std::nullopt;
    }
    return next_m_data_offset;
}

typename MidiStorageBase::Elem *
MidiStorageBase::unsafe_at(uint32_t offset) const {
    return (Elem *)&(m_data.at(offset));
}

uint32_t MidiStorageBase::bytes_size() const {
    return (m_head > m_tail) ? m_head - m_tail
                             : (m_head + m_data.size()) - m_tail;
}

void MidiStorageBase::store_unsafe(uint32_t offset,
                                    uint32_t t, uint16_t s,
                                    const uint8_t *d,
                                    uint16_t n_filler) {
    Elem elem;
    elem.size = s;
    elem.proc_time = elem.storage_time = t;
    Elem *to = (Elem*)&(m_data.at(offset));
    auto const data_size = (size_t)s;
    auto const meta_size = Elem::total_size_of(0);
    auto const total_size = Elem::total_size_of(s);
    log<log_level_debug_trace>("store unsafe: offset {}, time {}, size {}, stored size {}, filler {}", offset, t, s, total_size, n_filler);
    memcpy((void *)to, (void *)&elem, meta_size);
    void *data_ptr = (void*)((uint8_t*)to + meta_size);
    memcpy(data_ptr, d, data_size);
    to->offset_to_next = total_size + n_filler;
}

MidiStorageBase::MidiStorageBase(uint32_t data_size)
    : // MIDI storage is tied to a channel. For log readability we use that
      // name.
      m_head(0), m_tail(0), m_head_start(0), m_n_events(0), m_data(data_size) {
}

uint32_t MidiStorageBase::bytes_capacity() const {
    return m_data.size();
}

void MidiStorageBase::clear() {
    m_head = m_tail = m_head_start = m_n_events = 0;
}

uint32_t MidiStorageBase::bytes_occupied() const {
    if (m_head > m_tail) {
        return m_head - m_tail;
    } else if (m_head == m_tail && m_n_events > 0) {
        return m_data.size();
    } else if (m_head == m_tail && m_n_events == 0) {
        return 0;
    }
    return m_data.size() - (m_tail - m_head);
}

uint32_t MidiStorageBase::bytes_free() const {
    return m_data.size() - bytes_occupied();
}

uint32_t MidiStorageBase::n_events() const {
    return m_n_events;
}

bool MidiStorageBase::append(uint32_t time, uint16_t size,
                             const uint8_t *data, bool allow_replace,
                             DroppedMsgCallback dropped_msg_cb) {
    log<log_level_debug_trace>("append: time {}, size {}", time, size);

    uint32_t sz = Elem::total_size_of(size);
    if (sz > bytes_free() && !allow_replace) {
        log<log_level_warning>("Ignoring store of MIDI message: buffer full.");
        return false;
    }
    if (m_n_events > 0 && unsafe_at(m_head_start)->storage_time > time) {
        // Don't store out-of-order messages
        log<log_level_warning>("Ignoring store of out-of-order MIDI message.");
        return false;
    }

    int new_head_nowrap = (int)(m_head + sz);
    int m_next_tail_nowrap = (m_tail < m_head) ? (int)(m_tail + bytes_capacity()) : (int)m_tail;
    auto new_n_events = m_n_events;

    // if we are crossing our own tail, remove message(s) from the tail side
    uint32_t n_removed = 0;
    while(new_head_nowrap > m_next_tail_nowrap) {
        auto n = maybe_next_elem_offset(unsafe_at(m_tail));
        if (n.has_value()) {
            auto const next_offset = n.value();
            if (dropped_msg_cb) {
                auto dropped_msg = unsafe_at(m_tail);
                dropped_msg_cb(dropped_msg->storage_time, dropped_msg->size, dropped_msg->data());
            }
            auto shift_amount = (next_offset > m_tail) ?
                                 next_offset - m_tail :
                                (next_offset + m_data.size()) - m_tail;
            m_tail = (m_tail + shift_amount) % m_data.size();
            m_next_tail_nowrap += shift_amount;
            new_n_events -= 1;
            n_removed += 1;
        } else {
            break;
        }
    }

    // It could be that even though we freed up enough bytes, the free bytes
    // are spanning the boundary of the buffer. To handle this case, add filler
    // to our current head such that the next element will appear at the buffer
    // start, and re-do the procedure.
    auto const new_head = new_head_nowrap % m_data.size();
    if (new_head > 0 && new_head < m_head) {
        auto const prev_free = bytes_free();
        log<log_level_debug_trace>("append: freed up space crosses buffer boundary, adding filler");
        auto const new_offset_to_next = m_data.size() - m_head_start;
        auto const prev_offset_to_next = unsafe_at(m_head_start)->offset_to_next;
        unsafe_at(m_head_start)->offset_to_next = new_offset_to_next;
        m_head = 0;
        while (sz >= m_tail) {
            log<log_level_debug_trace>("append: removing additional message to make space after adding filler");
            // Adding filler caused us to cross our tail, remove one element.
            if (dropped_msg_cb) {
                auto dropped_msg = unsafe_at(m_tail);
                dropped_msg_cb(dropped_msg->storage_time, dropped_msg->size, dropped_msg->data());
            }
            m_tail = (m_tail + unsafe_at(m_tail)->offset_to_next) % m_data.size();
            n_removed += 1;
            new_n_events -= 1;
            // TODO test
        }
        if (n_removed > 0) {
            log<log_level_debug_trace>("append: removed {} messages to make space", n_removed);
        }

        m_n_events = new_n_events;
        return append(time, size, data, allow_replace, dropped_msg_cb);
    }

    m_head_start = m_head;
    m_head = new_head_nowrap % m_data.size();

    store_unsafe(m_head_start, time, size, data, 0);
    new_n_events += 1;
    m_n_events = new_n_events;

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
    uint32_t sz = Elem::total_size_of(size);
    if (sz > bytes_free()) {
        log<log_level_debug_trace>("Prepend: buffer full");
        return false;
    }
    if (m_n_events > 0 && unsafe_at(m_tail)->get_time() < time) {
        // Don't store out-of-order messages
        log<log_level_warning>("Ignoring store of out-of-order MIDI message.");
        return false;
    }

    int new_tail = (int)m_tail - (int)sz;
    uint16_t n_filler = 0;
    if (new_tail < 0) {
        new_tail = m_data.size() - (int)sz;
        if (new_tail < 0) {
            log<log_level_debug_trace>("Message too big to prepend");
            return false;
        }
        auto const new_offset = (m_data.size() - new_tail) + m_tail;
        n_filler = new_offset - sz;
        if((sz + n_filler) > bytes_free()) {
            log<log_level_debug_trace>("Prepend: buffer full (filler)");
            return false;
        }
    }
    m_tail = (uint32_t) new_tail;
    m_n_events++;
    store_unsafe(m_tail, time, size, data, n_filler);

    return true;
}

void MidiStorageBase::copy(
    MidiStorageBase &to) const {
    to.m_data.resize(m_data.size());
    if (m_head >= m_tail) {
        memcpy((void *)(to.m_data.data()), (void *)&(m_data.at(m_tail)),
               m_head - m_tail);
    } else {
        uint32_t first_copy = m_data.size() - m_tail;
        uint32_t second_copy = m_head;
        memcpy((void *)(to.m_data.data()), (void *)&(m_data.at(m_tail)),
               first_copy);
        memcpy((void *)&(to.m_data.at(first_copy)), (void *)m_data.data(),
               second_copy);
    }
    to.m_tail = 0;
    to.m_head = bytes_occupied();
    to.m_n_events = m_n_events;
    to.m_head_start = m_n_events > 0 ? to.m_head - (m_head - m_head_start) : 0;
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
    auto e = (Elem *)(&m_storage->m_data.at(raw_offset));
    return (Elem *)(&m_storage->m_data.at(raw_offset));
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
    auto next_offset = m_storage->maybe_next_elem_offset(get());
    if (next_offset.has_value()) {
        log<log_level_debug_trace>("cursor moving to next: {}", next_offset.value());
        m_prev_offset = m_offset;
        m_offset = next_offset;
    } else {
        invalidate();
    }
}

bool MidiStorageCursor::wrapped() const {
    return false;
}

CursorFindResult MidiStorageCursor::find_time_forward(
    uint32_t time, std::function<void(Elem *)> maybe_skip_msg_callback)
{
    auto print_offset = m_offset.has_value() ? (int)m_offset.value() : (int)-1;
    log<log_level_debug_trace>("find_time_forward (storage {}, cursor {}, target time {})", fmt::ptr(m_storage.get()), print_offset, time);
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
    std::optional<uint32_t> prev = m_offset;
    for (auto next_offset = m_offset, prev = m_prev_offset;
         next_offset.has_value(); prev = next_offset,
              next_offset = m_storage->maybe_next_elem_offset(
                  m_storage->unsafe_at(next_offset.value())),
              rval.n_processed++) {
        Elem *elem =
            prev.has_value() ? m_storage->unsafe_at(prev.value()) : nullptr;
        Elem *next_elem = m_storage->unsafe_at(next_offset.value());
        if (elem) {
            // skip current element
            log<log_level_debug_trace>("Skip event @ {}", elem->storage_time);
            if (maybe_skip_msg_callback) {
                maybe_skip_msg_callback(elem);
            }
        }
        if (fn(next_elem)) {
            // Found
            log<log_level_debug_trace>("find_fn_forward done, next msg @ {}", next_elem->storage_time);
            m_offset = next_offset;
            m_prev_offset = prev;
            rval.found_valid_elem = true;
            return rval;
        }
    }

    // If we reached here, we reached the end. Reset to an invalid
    // cursor.
    log<log_level_debug_trace>("find_fn_forward: none found");
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
    auto _should_truncate_fn = [this, type, should_truncate_fn](Elem* e) {
        return should_truncate_fn(e->storage_time, e->size, e->data());
    };

    if (type == TruncateSide::TruncateHead && !_should_truncate_fn(this->unsafe_at(this->m_head_start)))
    {
            this->template log<log_level_debug_trace>("truncate: head unchanged");
            return;
    }

    if (type == TruncateSide::TruncateTail && !_should_truncate_fn(this->unsafe_at(this->m_tail)))
    {
            this->template log<log_level_debug_trace>("truncate: tail unchanged");
            return;
    }

    if (this->m_n_events > 0) {
        auto cursor = create_cursor();
        if (cursor->valid()) {
            auto find_result = cursor->find_fn_forward([this, type, _should_truncate_fn, dropped_msg_cb](Elem* e) {
                // Iterate over messages which will be dropped (if truncating tail)
                // or remain (if truncating head)
                auto stop = (type == TruncateSide::TruncateHead) ?
                     _should_truncate_fn(e) :
                    !_should_truncate_fn(e);
                if (!stop && dropped_msg_cb && type == TruncateSide::TruncateTail) {
                    if (dropped_msg_cb) {
                        dropped_msg_cb(e->storage_time, e->size, e->data());
                    }
                }
                return stop;
            });

            if (type == TruncateSide::TruncateHead) {
                uint32_t new_head = cursor->offset().value();
                auto new_n = find_result.n_processed;
                this->template log<log_level_debug_trace>("truncate head: {} -> {}, n msgs {} -> {}", this->m_head, new_head, this->m_n_events, new_n);
                this->m_n_events = new_n;
                this->m_head = cursor->offset().value();
                this->m_head_start = cursor->prev_offset().value_or(0);
                // The rest of the messages will be dropped, so send them to the callback
                // if needed.
                if (dropped_msg_cb) {
                    for (cursor->next(); cursor->valid(); cursor->next()) {
                        auto *elem = cursor->get();
                        dropped_msg_cb(elem->storage_time, elem->size, elem->data());
                    }
                }
            } else if (type == TruncateSide::TruncateTail) {
                uint32_t new_tail = find_result.found_valid_elem ? cursor->offset().value() : this->m_head;
                auto new_n = this->m_n_events - find_result.n_processed;
                this->template log<log_level_debug_trace>("truncate tail: {} -> {}, n msgs {} -> {}", this->m_tail, new_tail, this->m_n_events, new_n);
                this->m_n_events = new_n;
                this->m_tail = new_tail;
                if (!find_result.found_valid_elem) {
                    this->m_head = this->m_tail;
                    this->m_head_start = this->m_tail;
                }
            }

            for (auto &cursor : m_cursors) {
                shoop_shared_ptr<Cursor> maybe_shared = cursor.lock();
                if (maybe_shared) {
                    if (type == TruncateSide::TruncateHead) {
                        if (maybe_shared->offset() > this->m_head) {
                            maybe_shared->overwrite(this->m_head,
                                                    this->m_head_start);
                        }
                    } else if (type == TruncateSide::TruncateTail) {
                        if (maybe_shared->offset() < this->m_tail || !find_result.found_valid_elem) {
                            maybe_shared->invalidate();
                        }
                    }
                }
            }
        } else {
            this->template log<log_level_error>("truncate: couldn't make valid cursor");
        }
    }
    ModuleLoggingEnabled<"Backend.MidiStorage">::log<log_level_debug_trace>("truncate: was {}, now {} msgs", prev_n_events, this->m_n_events);
}

void MidiStorage::for_each_msg_modify(
    std::function<void(uint32_t &t, uint16_t &s, uint8_t *data)> cb) {
    auto maybe_self = MidiStorageBase::weak_from_this();
    if (auto self = maybe_self.lock()) {
        auto cursor = shoop_make_shared<Cursor>(shoop_static_pointer_cast<const MidiStorageBase>(self));
        cursor->reset();
        auto *first_elem = cursor->get();
        decltype(first_elem) prev_elem = nullptr;
        for (; cursor->valid(); cursor->next()) {
            auto *elem = cursor->get();
            if (elem >= first_elem && prev_elem && ((prev_elem < first_elem) || (elem < prev_elem))) {
                throw std::runtime_error ("Message iterator looped back to start");
            }
            cb(elem->storage_time, elem->size, elem->data());
            prev_elem = elem;
        }
        cursor = nullptr;
    } else {
        throw std::runtime_error(
            "Attempting to retrieve contents of destructed storage");
    }
}

void MidiStorage::for_each_msg(
    std::function<void(uint32_t t, uint16_t s, uint8_t *data)> cb)
{
    for_each_msg_modify([cb](uint32_t &t, uint16_t &s, uint8_t *data) {
        cb(t, s, data);
    });
}