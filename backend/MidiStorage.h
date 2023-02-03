#pragma once
#include <cstdint>
#include <vector>
#include <stdio.h>
#include <optional>
#include <memory>
#include <atomic>
#include <cstring>
#include <iostream>
#include <functional>

template<typename _MidiStorage>
class MidiStorageCursor {
    std::optional<size_t> offset;
    std::optional<size_t> prev_offset;
    std::shared_ptr<_MidiStorage> storage;
    
    using Elem = typename _MidiStorage::Elem;
public:
    MidiStorageCursor(std::shared_ptr<_MidiStorage> &_storage) : storage(_storage) {}

    bool valid() const { return offset.has_value(); }

    void invalidate() {
        offset = prev_offset = std::nullopt;
    }

    void reset() {
        if (storage->m_n_events == 0) { invalidate(); }
        else {
            offset = storage->m_tail;
            prev_offset = std::nullopt;
        }
    }

    Elem *get() const {
        return offset.has_value() ?
            (Elem*)(&storage->m_data.at(offset.value())) :
            nullptr;
    }

    void next() {
        auto next_offset = storage->maybe_next_elem_offset(get());
        if (next_offset.has_value()) {
            prev_offset = offset;
            offset = next_offset;
        } else {
            invalidate();
        }
    }

    // Iterate a cursor to the point that it points to the
    // first event after a given time.
    void find_time_forward(size_t time) {
        if (!valid()) { reset(); }
        if (!valid()) { return; }
        std::optional<size_t> prev = offset;
        for (auto next_offset = storage->maybe_next_elem_offset(get()),
             prev = offset
             ;
             next_offset.has_value()
             ;
             prev = next_offset,
             next_offset = storage->maybe_next_elem_offset(get()))
        {
            Elem* next_elem = storage->unsafe_at(next_offset);
            if (next_elem->time >= time) {
                // Found
                offset = next_offset;
                prev_offset = prev;
                return;
            }
        }

        // If we reached here, we reached the end. Reset to an invalid
        // cursor.
        reset();
    }

};

template<typename TimeType, typename SizeType>
class MidiStorage : public std::enable_shared_from_this<MidiStorage<TimeType, SizeType>>{
public:
    // Note: variable-sized C-style structs are useful
    // here, but dangerous in general. Careful!
    struct Elem {
        TimeType time;
        SizeType size;
        uint8_t  data[];

        static size_t size_of(Elem const& e) {
            return sizeof(time) + sizeof(size) + e.size;
        }
    };

    typedef MidiStorage<TimeType, SizeType> MyType;
    typedef MidiStorageCursor<MyType> Cursor;
    typedef std::shared_ptr<Cursor> SharedCursor;
    friend class MidiStorageCursor<MyType>;

private:
    std::vector<uint8_t> m_data;
    size_t m_tail;
    size_t m_head;
    size_t m_head_start;
    size_t m_n_events;
    std::vector<std::weak_ptr<Cursor>> m_cursors;
    static constexpr size_t n_starting_cursors = 10;

    bool valid_elem_at(size_t offset) const {
        return m_head > m_tail ?
            (offset >= m_tail && offset < m_head) :
            (offset < m_head || offset >= m_tail);
    }

    std::optional<size_t> maybe_next_elem_offset(Elem *e) const {
        if (!e) { return std::nullopt; }
        size_t m_data_offset = (uint8_t*)e - (uint8_t*)m_data.data();
        size_t next_m_data_offset = (m_data_offset + Elem::size_of(*e)) % m_data.size();
        
        if (!valid_elem_at(next_m_data_offset)) { return std::nullopt; }
        return next_m_data_offset;
    }

    Elem *unsafe_at(size_t offset) {
        return (Elem*)&(m_data.at(offset));
    }

    size_t bytes_size() const {
        return (m_head > m_tail) ?
            m_head - m_tail :
            (m_head + m_data.size()) - m_tail;
    }

    void store_unsafe(size_t offset, TimeType t, SizeType s, uint8_t* d) {
        uint8_t* to = &(m_data.at(offset));
        memcpy((void*)to, &t, sizeof(t));
        memcpy((void*)(to + sizeof(t)), &s, sizeof(s));
        memcpy((void*)(to + sizeof(t) + sizeof(s)), d, s);
    }

public:
    MidiStorage<TimeType, SizeType>(size_t data_size) :
        m_head(0), m_tail(0), m_head_start(0), m_n_events(0),
        m_data(data_size)
    {
        m_cursors.reserve(n_starting_cursors);
    }

    SharedCursor create_cursor() {
        auto rval = std::make_shared<Cursor>(
            std::enable_shared_from_this<MidiStorage<TimeType, SizeType>>::shared_from_this()
        );
        m_cursors.push_back(rval);
        rval->reset();
        return rval;
    }

    size_t bytes_capacity() const { return m_data.size(); }

    void clear() {
        for (auto &elem: m_cursors) {
            if(auto c = elem.lock()) {
                c->reset(true);
            }
        }
        m_cursors.clear();
        m_head = m_tail = m_head_start = m_n_events = 0;
    };

    void copy(MyType &to) const {
        if (to.m_data.size() < m_data.size()) { to.m_data.resize(m_data.size()); }
        if(m_head >= m_tail) {
            memcpy((void*)(to.m_data.data()), (void*)&(m_data.at(m_tail)), m_head - m_tail);
        } else {
            size_t first_copy = m_data.size() - m_tail;
            size_t second_copy = m_head;
            memcpy((void*)(to.m_data.data()), (void*)&(m_data.at(m_tail)), first_copy);
            memcpy((void*)(to.m_data.at(first_copy)), (void*)m_data.data(), second_copy);
        }
        to.m_head = to.m_tail = 0;
        to.m_n_events = m_n_events;
        to.m_head_start = m_n_events > 0 ?
            to.m_data.size() - (m_head - m_head_start) : 0;
    }

    size_t bytes_occupied() const {
        if (m_head > m_tail) {
            return m_head - m_tail;
        } else if (m_head == m_tail && m_n_events > 0) {
            return m_data.size();
        } else if (m_head == m_tail && m_n_events == 0) {
            return 0;
        }
        return m_data.size() - (m_tail - m_head);
    }

    size_t bytes_free() const {
        return m_data.size() - bytes_occupied();
    }

    bool append(TimeType time, SizeType size, uint8_t* data) {
        size_t sz = sizeof(time) + sizeof(sz) + size;
        if (sz > bytes_free()) {
            std::cerr << "Ignoring store of MIDI message: buffer full." << std::endl;
            return false;
        }
        if (m_n_events > 0 && unsafe_at(m_head_start)->time > time) {
            // Don't store out-of-order messages
            std::cerr << "Ignoring store of out-of-order MIDI message." << std::endl;
            return false;
        }

        m_head_start = m_head;
        m_head = (m_head + sz) % m_data.size();
        m_n_events++;

        store_unsafe(m_head_start, time, size, data);

        return true;
    }

    bool prepend(TimeType time, SizeType size, uint8_t* data) {
        size_t sz = sizeof(time) + sizeof(sz) + size;
        if (sz > bytes_free()) { return false; }
        if (m_n_events > 0 && unsafe_at(m_tail)->time < time) {
            // Don't store out-of-order messages
            throw std::runtime_error("Attempting to store out-of-order MIDI message.");
        }

        int new_tail = (int)m_tail - (int)sz;
        m_tail = new_tail < 0 ?
            (size_t)(new_tail + m_data.size()) : (size_t) new_tail;
        m_n_events++;

        store_unsafe(m_tail, time, size, data);

        return true;
    }

    void for_each_msg(std::function<void(TimeType t, SizeType s, uint8_t* data)> cb) {
        auto cursor = std::make_shared<Cursor>(
            std::enable_shared_from_this<MidiStorage<TimeType, SizeType>>::shared_from_this()
        );
        cursor.reset();
        for(; cursor->next(); cursor.valid()) {
            auto *elem = cursor->get();
            cb(elem->time, elem->size, elem->data);
        }
    }
};