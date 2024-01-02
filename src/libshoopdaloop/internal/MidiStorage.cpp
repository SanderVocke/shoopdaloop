#include "MidiStorage.h"
#include <atomic>
#include <cstdint>
#include <cstring>
#include <functional>
#include <iostream>
#include <memory>
#include <optional>
#include <stdio.h>
#include <vector>
#include <fmt/format.h>

using namespace logging;

template <typename TimeType, typename SizeType>
uint32_t MidiStorageElem<TimeType, SizeType>::total_size_of(uint32_t size) {
    return sizeof(MidiStorageElem<TimeType, SizeType>) + size;
}

template <typename TimeType, typename SizeType>
uint8_t *MidiStorageElem<TimeType, SizeType>::data() const {
    return ((uint8_t *)this) + sizeof(MidiStorageElem<TimeType, SizeType>);
}

template <typename TimeType, typename SizeType>
const uint8_t *MidiStorageElem<TimeType, SizeType>::get_data() const {
    return ((uint8_t *)this) + sizeof(MidiStorageElem<TimeType, SizeType>);
}

template <typename TimeType, typename SizeType>
uint32_t MidiStorageElem<TimeType, SizeType>::get_time() const {
    return proc_time;
}

template <typename TimeType, typename SizeType>
uint32_t MidiStorageElem<TimeType, SizeType>::get_size() const {
    return size;
}

template <typename TimeType, typename SizeType>
void MidiStorageElem<TimeType, SizeType>::get(uint32_t &size_out,
                                              uint32_t &time_out,
                                              const uint8_t *&data_out) const {
    size_out = size;
    time_out = proc_time;
    data_out = data();
}

template <typename TimeType, typename SizeType>
bool MidiStorageBase<TimeType, SizeType>::valid_elem_at(uint32_t offset) const {
    return m_n_events > 0 &&
           (m_head > m_tail ? (offset >= m_tail && offset < m_head)
                            : (offset < m_head || offset >= m_tail));
}

template <typename TimeType, typename SizeType>
std::optional<uint32_t>
MidiStorageBase<TimeType, SizeType>::maybe_next_elem_offset(Elem *e) const {
    if (!e) {
        return std::nullopt;
    }
    uint32_t m_data_offset = (uint8_t *)e - (uint8_t *)m_data.data();
    uint32_t next_m_data_offset =
        (m_data_offset + Elem::total_size_of(e->size)) % m_data.size();

    if (!valid_elem_at(next_m_data_offset)) {
        return std::nullopt;
    }
    return next_m_data_offset;
}

template <typename TimeType, typename SizeType>
typename MidiStorageBase<TimeType, SizeType>::Elem *
MidiStorageBase<TimeType, SizeType>::unsafe_at(uint32_t offset) const {
    return (Elem *)&(m_data.at(offset));
}

template <typename TimeType, typename SizeType>
uint32_t MidiStorageBase<TimeType, SizeType>::bytes_size() const {
    return (m_head > m_tail) ? m_head - m_tail
                             : (m_head + m_data.size()) - m_tail;
}

template <typename TimeType, typename SizeType>
void MidiStorageBase<TimeType, SizeType>::store_unsafe(uint32_t offset,
                                                       TimeType t, SizeType s,
                                                       const uint8_t *d) {
    static Elem elem;
    elem.size = s;
    elem.proc_time = elem.storage_time = t;
    uint8_t *to = &(m_data.at(offset));
    memcpy((void *)to, (void *)&elem, sizeof(Elem));
    void *data_ptr = to + sizeof(Elem);
    memcpy(data_ptr, d, s);
}

template <typename TimeType, typename SizeType>
MidiStorageBase<TimeType, SizeType>::MidiStorageBase(uint32_t data_size)
    : // MIDI storage is tied to a channel. For log readability we use that
      // name.
      m_head(0), m_tail(0), m_head_start(0), m_n_events(0), m_data(data_size) {
}

template <typename TimeType, typename SizeType>
uint32_t MidiStorageBase<TimeType, SizeType>::bytes_capacity() const {
    return m_data.size();
}

template <typename TimeType, typename SizeType>
void MidiStorageBase<TimeType, SizeType>::clear() {
    m_head = m_tail = m_head_start = m_n_events = 0;
}

template <typename TimeType, typename SizeType>
uint32_t MidiStorageBase<TimeType, SizeType>::bytes_occupied() const {
    if (m_head > m_tail) {
        return m_head - m_tail;
    } else if (m_head == m_tail && m_n_events > 0) {
        return m_data.size();
    } else if (m_head == m_tail && m_n_events == 0) {
        return 0;
    }
    return m_data.size() - (m_tail - m_head);
}

template <typename TimeType, typename SizeType>
uint32_t MidiStorageBase<TimeType, SizeType>::bytes_free() const {
    return m_data.size() - bytes_occupied();
}

template <typename TimeType, typename SizeType>
uint32_t MidiStorageBase<TimeType, SizeType>::n_events() const {
    return m_n_events;
}

template <typename TimeType, typename SizeType>
bool MidiStorageBase<TimeType, SizeType>::append(TimeType time, SizeType size,
                                                 const uint8_t *data) {
    uint32_t sz = Elem::total_size_of(size);
    if (sz > bytes_free()) {
        log<log_level_warning>("Ignoring store of MIDI message: buffer full.");
        return false;
    }
    if (m_n_events > 0 && unsafe_at(m_head_start)->storage_time > time) {
        // Don't store out-of-order messages
        log<log_level_warning>("Ignoring store of out-of-order MIDI message.");
        return false;
    }

    m_head_start = m_head;
    m_head = (m_head + sz) % m_data.size();
    m_n_events++;

    store_unsafe(m_head_start, time, size, data);

    return true;
}

template <typename TimeType, typename SizeType>
bool MidiStorageBase<TimeType, SizeType>::prepend(TimeType time, SizeType size,
                                                  const uint8_t *data) {
    uint32_t sz = sizeof(time) + sizeof(sz) + size;
    if (sz > bytes_free()) {
        return false;
    }
    if (m_n_events > 0 && unsafe_at(m_tail)->get_time() < time) {
        // Don't store out-of-order messages
        log<log_level_warning>("Ignoring store of out-of-order MIDI message.");
        return false;
    }

    int new_tail = (int)m_tail - (int)sz;
    m_tail =
        new_tail < 0 ? (uint32_t)(new_tail + m_data.size()) : (uint32_t)new_tail;
    m_n_events++;

    store_unsafe(m_tail, time, size, data);

    return true;
}

template <typename TimeType, typename SizeType>
void MidiStorageBase<TimeType, SizeType>::copy(
    MidiStorageBase<TimeType, SizeType> &to) const {
    if (to.m_data.size() < m_data.size()) {
        to.m_data.resize(m_data.size());
    }
    if (m_head >= m_tail) {
        memcpy((void *)(to.m_data.data()), (void *)&(m_data.at(m_tail)),
               m_head - m_tail);
    } else {
        uint32_t first_copy = m_data.size() - m_tail;
        uint32_t second_copy = m_head;
        memcpy((void *)(to.m_data.data()), (void *)&(m_data.at(m_tail)),
               first_copy);
        memcpy((void *)(to.m_data.at(first_copy)), (void *)m_data.data(),
               second_copy);
    }
    to.m_tail = 0;
    to.m_head = bytes_occupied();
    to.m_n_events = m_n_events;
    to.m_head_start = m_n_events > 0 ? to.m_head - (m_head - m_head_start) : 0;
}

template <typename TimeType, typename SizeType>
MidiStorageCursor<TimeType, SizeType>::MidiStorageCursor(
    std::shared_ptr<const Storage> _storage)
    : m_storage(_storage) {}

template <typename TimeType, typename SizeType>
bool MidiStorageCursor<TimeType, SizeType>::valid() const {
    return m_offset.has_value();
}

template <typename TimeType, typename SizeType>
std::optional<uint32_t> MidiStorageCursor<TimeType, SizeType>::offset() const {
    return m_offset;
}

template <typename TimeType, typename SizeType>
std::optional<uint32_t>
MidiStorageCursor<TimeType, SizeType>::prev_offset() const {
    return m_prev_offset;
}

template <typename TimeType, typename SizeType>
void MidiStorageCursor<TimeType, SizeType>::invalidate() {
    m_offset.reset();
    m_prev_offset.reset();
}

template <typename TimeType, typename SizeType>
bool MidiStorageCursor<TimeType, SizeType>::is_at_start() const {
    return !m_prev_offset.has_value();
}

template <typename TimeType, typename SizeType>
void MidiStorageCursor<TimeType, SizeType>::overwrite(uint32_t offset,
                                                      uint32_t prev_offset) {
    m_offset = offset;
    m_prev_offset = prev_offset;
}

template <typename TimeType, typename SizeType>
void MidiStorageCursor<TimeType, SizeType>::reset() {
    if (m_storage->m_n_events == 0) {
        invalidate();
    } else {
        m_offset = m_storage->m_tail;
        m_prev_offset = std::nullopt;
    }
}

template <typename TimeType, typename SizeType>
typename MidiStorageCursor<TimeType, SizeType>::Elem *
MidiStorageCursor<TimeType, SizeType>::get(uint32_t raw_offset) const {
    auto e = (Elem *)(&m_storage->m_data.at(raw_offset));
    return (Elem *)(&m_storage->m_data.at(raw_offset));
}

template <typename TimeType, typename SizeType>
typename MidiStorageCursor<TimeType, SizeType>::Elem *
MidiStorageCursor<TimeType, SizeType>::get() const {
    return m_offset.has_value() ? get(m_offset.value()) : nullptr;
}

template <typename TimeType, typename SizeType>
typename MidiStorageCursor<TimeType, SizeType>::Elem *
MidiStorageCursor<TimeType, SizeType>::get_prev() const {
    return m_prev_offset.has_value() ? get(m_prev_offset.value()) : nullptr;
}

template <typename TimeType, typename SizeType>
void MidiStorageCursor<TimeType, SizeType>::next() {
    auto next_offset = m_storage->maybe_next_elem_offset(get());
    if (next_offset.has_value()) {
        m_prev_offset = m_offset;
        m_offset = next_offset;
    } else {
        invalidate();
    }
}

template <typename TimeType, typename SizeType>
uint32_t MidiStorageCursor<TimeType, SizeType>::find_time_forward(
    uint32_t time, std::function<void(Elem *)> maybe_skip_msg_callback)
{
    log<log_level_trace>("find_time_forward (storage {}, time {})", fmt::ptr(m_storage), time);
    if (!valid()) {
        log<log_level_trace>("find_time_forward: resetting (not valid)");
        reset();
    }
    if (!valid()) {
        log<log_level_trace>("find_time_forward: not valid after reset (no msgs)");
        return 0;
    }
    std::optional<uint32_t> prev = m_offset;
    uint32_t n_processed = 0;
    for (auto next_offset = m_offset, prev = m_prev_offset;
         next_offset.has_value(); prev = next_offset,
              next_offset = m_storage->maybe_next_elem_offset(
                  m_storage->unsafe_at(next_offset.value())),
              n_processed++) {
        Elem *elem =
            prev.has_value() ? m_storage->unsafe_at(prev.value()) : nullptr;
        Elem *next_elem = m_storage->unsafe_at(next_offset.value());
        if (elem) {
            // skip current element
            log<log_level_trace>("Skip event @ {}", elem->storage_time);
            if (maybe_skip_msg_callback) {
                maybe_skip_msg_callback(elem);
            }
        }
        if (next_elem->storage_time >= time) {
            // Found
            log<log_level_trace>("find_time_forward to {} done, next msg @ {}", time, next_elem->storage_time);
            m_offset = next_offset;
            m_prev_offset = prev;
            return n_processed;
        }
    }

    // If we reached here, we reached the end. Reset to an invalid
    // cursor.
    log<log_level_trace>("find_time_forward to {}: none found, reset", time);
    reset();
    return n_processed;
}

template <typename TimeType, typename SizeType>
MidiStorage<TimeType, SizeType>::MidiStorage(uint32_t data_size)
    : MidiStorageBase<TimeType, SizeType>(data_size) {
    m_cursors.reserve(n_starting_cursors);
}

template <typename TimeType, typename SizeType>
typename MidiStorage<TimeType, SizeType>::SharedCursor
MidiStorage<TimeType, SizeType>::create_cursor() {
    auto maybe_self = MidiStorageBase<TimeType, SizeType>::weak_from_this();
    if (auto self = maybe_self.lock()) {
        auto rval = std::make_shared<Cursor>(self);
        m_cursors.push_back(rval);
        rval->reset();
        return rval;
    } else {
        throw std::runtime_error(
            "Attempting to create cursor for destructed storage");
    }
}

template <typename TimeType, typename SizeType>
void MidiStorage<TimeType, SizeType>::clear() {
    for (auto &elem : m_cursors) {
        if (auto c = elem.lock()) {
            c->invalidate();
        }
    }
    m_cursors.clear();
    MidiStorageBase<TimeType, SizeType>::clear();
}

template <typename TimeType, typename SizeType>
void MidiStorage<TimeType, SizeType>::truncate(TimeType time) {
    ModuleLoggingEnabled<"Backend.MidiChannel.Storage">::log<log_level_trace>("truncate to {}", time);
    if (this->m_n_events > 0 &&
        this->unsafe_at(this->m_head_start)->storage_time > time) {
        auto cursor = create_cursor();
        if (cursor->valid()) {
            auto prev_n_events = this->m_n_events;
            this->m_n_events = cursor->find_time_forward(time);
            ModuleLoggingEnabled<"Backend.MidiChannel.Storage">::log<log_level_trace>("truncate: was {}, now {} msgs", prev_n_events, this->m_n_events);

            this->m_head = cursor->offset().value();
            this->m_head_start = cursor->prev_offset().value_or(0);

            for (auto &cursor : m_cursors) {
                std::shared_ptr<Cursor> maybe_shared = cursor.lock();
                if (maybe_shared) {
                    if (maybe_shared->offset() > this->m_head) {
                        maybe_shared->overwrite(this->m_head,
                                                this->m_head_start);
                    }
                }
            }
        }
    }
}

template <typename TimeType, typename SizeType>
void MidiStorage<TimeType, SizeType>::for_each_msg(
    std::function<void(TimeType t, SizeType s, uint8_t *data)> cb) {
    auto maybe_self = MidiStorageBase<TimeType, SizeType>::weak_from_this();
    if (auto self = maybe_self.lock()) {
        auto cursor = std::make_shared<Cursor>(self);
        for (cursor->reset(); cursor->valid(); cursor->next()) {
            auto *elem = cursor->get();
            cb(elem->storage_time, elem->size, elem->data());
        }
        cursor = nullptr;
    } else {
        throw std::runtime_error(
            "Attempting to retrieve contents of destructed storage");
    }
}

template class MidiStorageElem<uint32_t, uint16_t>;
template class MidiStorageElem<uint32_t, uint32_t>;
template class MidiStorageElem<uint16_t, uint16_t>;
template class MidiStorageElem<uint16_t, uint32_t>;
template class MidiStorageCursor<uint32_t, uint16_t>;
template class MidiStorageCursor<uint32_t, uint32_t>;
template class MidiStorageCursor<uint16_t, uint16_t>;
template class MidiStorageCursor<uint16_t, uint32_t>;
template class MidiStorageBase<uint32_t, uint16_t>;
template class MidiStorageBase<uint32_t, uint32_t>;
template class MidiStorageBase<uint16_t, uint16_t>;
template class MidiStorageBase<uint16_t, uint32_t>;
template class MidiStorage<uint32_t, uint16_t>;
template class MidiStorage<uint32_t, uint32_t>;
template class MidiStorage<uint16_t, uint16_t>;
template class MidiStorage<uint16_t, uint32_t>;