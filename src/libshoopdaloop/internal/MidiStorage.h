#pragma once
#include "MidiPort.h"
#include "LoggingEnabled.h"
#include <vector>
#include <functional>

// We need a variable-sized struct that also supports interface inheritance.
// This cannot really be done in C++, so instead we just make a fixed-size
// struct that is designed with the knowledge that its data bytes will always
// be stored directly following it in the buffer.
// Some convenience functions are added for total size calculation and
// access to the data "member".
template<typename TimeType, typename SizeType>
struct MidiStorageElem : public MidiSortableMessageInterface {
    TimeType storage_time; // Overall time in the loop storage
    TimeType proc_time;    // time w.r.t. some reference point (position in this process iteration)
    SizeType size;

    static uint32_t total_size_of(uint32_t size);

    uint8_t* data() const;

    const uint8_t* get_data() const override;

    uint32_t get_time() const override;
    uint32_t get_size() const override;
    void get(uint32_t &size_out,
                uint32_t &time_out,
                const uint8_t* &data_out) const override;
};

template<typename TimeType, typename SizeType>
class MidiStorageCursor;

template<typename TimeType, typename SizeType>
class MidiStorageBase : public std::enable_shared_from_this<MidiStorageBase<TimeType, SizeType>>,
                        protected ModuleLoggingEnabled<"Backend.MidiChannel.Storage"> {
public:
    using Elem = MidiStorageElem<TimeType, SizeType>;
    Elem dummy_elem; // Prevents incomplete template type in log_level_debug build
    friend class MidiStorageCursor<TimeType, SizeType>;

protected:
    std::vector<uint8_t> m_data;
    uint32_t m_tail;
    uint32_t m_head;
    uint32_t m_head_start;
    uint32_t m_n_events;
    static constexpr uint32_t n_starting_cursors = 10;

    bool valid_elem_at(uint32_t offset) const;
    std::optional<uint32_t> maybe_next_elem_offset(Elem *e) const;
    Elem *unsafe_at(uint32_t offset) const;
    uint32_t bytes_size() const;
    void store_unsafe(uint32_t offset, TimeType t, SizeType s, const uint8_t* d);

public:
    MidiStorageBase(uint32_t data_size);

    uint32_t bytes_capacity() const;

    void clear();

    uint32_t bytes_occupied() const;
    uint32_t bytes_free() const;
    uint32_t n_events() const;

    bool append(TimeType time, SizeType size,  const uint8_t* data);
    bool prepend(TimeType time, SizeType size, const uint8_t* data);
    void copy(MidiStorageBase<TimeType, SizeType> &to) const;
};

template<typename TimeType, typename SizeType>
class MidiStorageCursor : protected ModuleLoggingEnabled<"Backend.MidiChannel.Storage.Cursor"> {
public:
    using Storage = MidiStorageBase<TimeType, SizeType>;
    using Elem = MidiStorageElem<TimeType, SizeType>;
    Elem dummy_elem; // Prevents incomplete template type in log_level_debug build

private:
    std::optional<uint32_t> m_offset;
    std::optional<uint32_t> m_prev_offset;
    std::shared_ptr<const Storage> m_storage;

public:
    MidiStorageCursor(std::shared_ptr<const Storage> _storage);

    bool valid() const;

    std::optional<uint32_t> offset() const;
    std::optional<uint32_t> prev_offset() const;

    void invalidate();
    bool is_at_start() const;
    void overwrite(uint32_t offset, uint32_t prev_offset);

    void reset();

    Elem *get(uint32_t raw_offset) const;
    Elem *get() const;
    Elem *get_prev() const;
    void next();

    uint32_t find_time_forward(uint32_t time, std::function<void(Elem *)> maybe_skip_msg_callback = nullptr);
};

template<typename TimeType, typename SizeType>
class MidiStorage : public MidiStorageBase<TimeType, SizeType> {
public:
    using Elem = MidiStorageElem<TimeType, SizeType>;
    using Cursor = MidiStorageCursor<TimeType, SizeType>;
    using SharedCursor = std::shared_ptr<Cursor>;

private:
    std::vector<std::weak_ptr<Cursor>> m_cursors;
    static constexpr uint32_t n_starting_cursors = 10;

public:
    MidiStorage(uint32_t data_size);

    SharedCursor create_cursor();

    void clear();
    void truncate(TimeType time);
    void for_each_msg(std::function<void(TimeType t, SizeType s, uint8_t* data)> cb);
};

extern template class MidiStorageElem<uint32_t, uint16_t>;
extern template class MidiStorageElem<uint32_t, uint32_t>;
extern template class MidiStorageElem<uint16_t, uint16_t>;
extern template class MidiStorageElem<uint16_t, uint32_t>;
extern template class MidiStorageCursor<uint32_t, uint16_t>;
extern template class MidiStorageCursor<uint32_t, uint32_t>;
extern template class MidiStorageCursor<uint16_t, uint16_t>;
extern template class MidiStorageCursor<uint16_t, uint32_t>;
extern template class MidiStorageBase<uint32_t, uint16_t>;
extern template class MidiStorageBase<uint32_t, uint32_t>;
extern template class MidiStorageBase<uint16_t, uint16_t>;
extern template class MidiStorageBase<uint16_t, uint32_t>;
extern template class MidiStorage<uint32_t, uint16_t>;
extern template class MidiStorage<uint32_t, uint32_t>;
extern template class MidiStorage<uint16_t, uint16_t>;
extern template class MidiStorage<uint16_t, uint32_t>;