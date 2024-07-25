#pragma once
#include "LoggingEnabled.h"
#include <vector>
#include <functional>
#include "shoop_shared_ptr.h"
#include "MidiBufferInterfaces.h"

typedef std::function<void(uint32_t time, uint16_t size, const uint8_t* data)> DroppedMsgCallback;

// We need a contiguously stored struct that also supports interface inheritance.
// This cannot really be done in C++, so instead we just make a fixed-size
// struct that is designed with the knowledge that its data bytes will always
// be stored directly following it in the buffer.
#pragma pack(push, 1)
struct MidiStorageElem : public MidiSortableMessageInterface {
    uint8_t is_filler = 0; // If nonzero, this byte in the buffer is filler and should be skipped.
                           // This can happen e.g. around the buffer boundaries when used as a ringbuffer.
                           // Messages are always kept contiguous.
    uint32_t storage_time = 0; // Overall time in the loop storage
    uint16_t proc_time = 0;    // time w.r.t. some reference point (position in this process iteration)
    uint16_t size = 0;

    static uint32_t total_size_of(uint32_t data_bytes);
    uint8_t* data() const;
    const uint8_t* get_data() const override;
    uint32_t get_time() const override;
    uint32_t get_size() const override;
    void get(uint32_t &size_out,
                uint32_t &time_out,
                const uint8_t* &data_out) const override;
};
#pragma pack(pop)

class MidiStorageCursor;

class MidiStorageBase : public shoop_enable_shared_from_this<MidiStorageBase>,
                        protected ModuleLoggingEnabled<"Backend.MidiStorage"> {
public:
    friend class MidiStorageCursor;
    using Elem = MidiStorageElem;

protected:
    std::vector<uint8_t> m_data;
    uint32_t m_tail = 0;
    uint32_t m_head = 0;
    uint32_t m_head_start = 0;
    uint32_t m_n_events = 0;
    static constexpr uint32_t n_starting_cursors = 10;

    bool valid_elem_at(uint32_t offset) const;
    std::optional<uint32_t> maybe_next_elem_offset(Elem* elem) const;
    uint32_t bytes_size() const;
    void store_unsafe(uint32_t offset, uint32_t t, uint16_t s, const uint8_t* d);
    Elem *unsafe_at(uint32_t offset) const;

public:
    MidiStorageBase(uint32_t data_size);

    uint32_t bytes_capacity() const;

    void clear();

    uint32_t bytes_occupied() const;
    uint32_t bytes_free() const;
    uint32_t n_events() const;

    virtual bool append(uint32_t time, uint16_t size,  const uint8_t* data,
                        bool allow_replace=false,
                        DroppedMsgCallback dropped_msg_cb=nullptr);
    bool prepend(uint32_t time, uint16_t size, const uint8_t* data);
    void copy(MidiStorageBase &to) const;
};

struct CursorFindResult {
    uint32_t n_processed;
    bool found_valid_elem;
};

class MidiStorageCursor : protected ModuleLoggingEnabled<"Backend.MidiStorage"> {
public:
    using Storage = MidiStorageBase;
    using Elem = MidiStorageElem;

private:
    std::optional<uint32_t> m_offset = std::nullopt;
    std::optional<uint32_t> m_prev_offset = std::nullopt;
    shoop_shared_ptr<const Storage> m_storage = nullptr;

public:
    MidiStorageCursor(shoop_shared_ptr<const Storage> _storage);

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

    // True if previous elem is valid, current elem is valid,
    // but stepping between them steps over the ringbuffer
    // boundary
    bool wrapped() const;

    // Iterate to the next message until the given time (or later) or end of buffer is reached.
    CursorFindResult find_time_forward(uint32_t time, std::function<void(Elem *)> maybe_skip_msg_callback = nullptr);

    // Iterate to the next message until the given function returns true or end of buffer is reached.
    CursorFindResult find_fn_forward(std::function<bool(Elem *)> fn, std::function<void(Elem *)> maybe_skip_msg_callback = nullptr);
};

class MidiStorage : public MidiStorageBase {
public:
    using Elem = MidiStorageElem;
    using Cursor = MidiStorageCursor;
    using SharedCursor = shoop_shared_ptr<Cursor>;

    enum class TruncateSide {
        TruncateHead, // Remove all messages with time > t from the head
        TruncateTail, // Remove all messages with time < t from the tail
    };

private:
    std::vector<shoop_weak_ptr<Cursor>> m_cursors;
    static constexpr uint32_t n_starting_cursors = 10;

public:
    MidiStorage(uint32_t data_size);

    SharedCursor create_cursor();

    void clear();

    // Truncate to a certain point in time
    void truncate(uint32_t time, TruncateSide side, DroppedMsgCallback dropped_msg_cb=nullptr);

    // Truncate away any msg for which the given function returns true
    void truncate_fn(std::function<bool(uint32_t time, uint16_t size, const uint8_t* data)> should_truncate_fn,
                     TruncateSide side, DroppedMsgCallback dropped_msg_cb=nullptr);

    void for_each_msg_modify(std::function<void(uint32_t &t, uint16_t &s, uint8_t* data)> cb);
    void for_each_msg(std::function<void(uint32_t t, uint16_t s, uint8_t* data)> cb);

    bool append(uint32_t time, uint16_t size, const uint8_t* data,
                bool allow_replace=false,
                DroppedMsgCallback dropped_msg_cb=nullptr) override;
};