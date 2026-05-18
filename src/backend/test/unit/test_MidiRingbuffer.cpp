#include "RustMidiStorage.h"
#include "shoop_shared_ptr.h"

#include <iostream>
#include <sstream>

#include <catch2/catch_test_macros.hpp>

#ifdef _WIN32
#undef min
#undef max
#endif

namespace Catch {
    template<>
    struct StringMaker<std::vector<uint8_t>> {
        static std::string convert(const std::vector<uint8_t>& vec) {
            std::ostringstream oss;
            oss << "[";
            for (size_t i=0; i<vec.size(); i++) {
                if (i>0) { oss << ", "; }
                oss << (int)vec[i];
            }
            oss << "]";
            return oss.str();
        }
    };

    template<>
    struct StringMaker<MidiStorageElem> {
        static std::string convert(const MidiStorageElem& e) {
            std::ostringstream oss;
            oss << "{ t:" << e.time << ", s:" << e.size << ", d:";
            oss << "[";
            for (size_t i=0; i<e.size; i++) {
                if (i>0) { oss << ", "; }
                oss << (int)e.bytes[i];
            }
            oss << "] }";
            return oss.str();
        }
    };

    template<>
    struct StringMaker<std::vector<MidiStorageElem>> {
        static std::string convert(const std::vector<MidiStorageElem>& vec) {
            std::ostringstream oss;
            oss << "[\n";
            for (auto &e : vec) {
                oss << "  " << StringMaker<MidiStorageElem>::convert(e) << "\n";
            }
            oss << "]";
            return oss.str();
        }
    };
}

template<typename Storage>
std::vector<MidiStorageElem> extract_messages(Storage &buf) {
    std::vector<MidiStorageElem> out;
    auto cursor = buf.create_cursor();
    while (cursor->valid()) {
        auto elem = cursor->get();
        MidiStorageElem msg;
        msg.time = elem->time;
        msg.size = elem->size;
        memcpy(msg.bytes, elem->bytes, elem->size);
        out.push_back(msg);
        cursor->next();
        if(cursor->is_at_start()) { break; }
    }
    return out;
}

inline MidiStorageElem make_msg(uint32_t time, std::initializer_list<uint8_t> data) {
    MidiStorageElem msg;
    msg.time = time;
    msg.size = static_cast<uint16_t>(data.size());
    memcpy(msg.bytes, data.begin(), data.size());
    return msg;
}

TEST_CASE("MidiRingbuffer - Put and increment", "[MidiRingbuffer]") {
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    auto b = std::make_shared<Ringbuffer>(200);
    b->set_n_samples(50);
    b->next_buffer(10);

    {
        auto m = make_msg(0, {0, 0, 0});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        auto m = make_msg(1, {1, 1, 1});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    b->next_buffer(10);
    {
        auto m = make_msg(2, {2, 2, 2});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    b->next_buffer(10);
    {
        auto m = make_msg(3, {3, 3, 3});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }

    CHECK(b->n_events() == 4);

    std::vector<MidiStorageElem> expect = {
        make_msg(0, {0, 0, 0}),
        make_msg(1, {1, 1, 1}),
        make_msg(12, {2, 2, 2}),
        make_msg(23, {3, 3, 3})
    };

    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put and truncate", "[MidiRingbuffer]") {
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    auto b = std::make_shared<Ringbuffer>(200);
    b->set_n_samples(17);
    b->next_buffer(10);

    {
        auto m = make_msg(0, {0, 0, 0});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        auto m = make_msg(1, {1, 1, 1});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    b->next_buffer(10);
    {
        auto m = make_msg(2, {2, 2, 2});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        auto m = make_msg(3, {3, 3, 3});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    b->next_buffer(10);
    {
        auto m = make_msg(3, {4, 4, 4});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }

    std::vector<MidiStorageElem> out;
    std::vector<MidiStorageElem> expect = {
        make_msg(13, {3, 3, 3}),
        make_msg(23, {4, 4, 4})
    };
    CHECK(b->n_events() == expect.size());
    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put and wrap", "[MidiRingbuffer]") {
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    // enough for exactly 3 messages
    auto b = std::make_shared<Ringbuffer>(sizeof(Storage::Elem) * 3);

    b->set_n_samples(10000);
    b->next_buffer(10);

    {
        auto m = make_msg(0, {0, 0, 0});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        auto m = make_msg(1, {1, 1, 1});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        auto m = make_msg(2, {2, 2, 2});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        // 4th message overwrites oldest (time 0)
        auto m = make_msg(3, {3, 3, 3});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        // 5th message overwrites next oldest (time 1)
        auto m = make_msg(4, {4, 4, 4});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    b->next_buffer(10);

    std::vector<MidiStorageElem> expect = {
        make_msg(2, {2, 2, 2}),
        make_msg(3, {3, 3, 3}),
        make_msg(4, {4, 4, 4})
    };
    CHECK(b->n_events() == expect.size());
    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put and wrap then truncate", "[MidiRingbuffer]") {
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    // enough for exactly 3 messages
    auto b = std::make_shared<Ringbuffer>(sizeof(Storage::Elem) * 3);

    b->set_n_samples(17);
    b->next_buffer(10);

    {
        auto m = make_msg(0, {0, 0, 0});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        auto m = make_msg(1, {1, 1, 1});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        auto m = make_msg(2, {2, 2, 2});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        // 4th message overwrites oldest
        auto m = make_msg(3, {3, 3, 3});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    {
        // 5th message overwrites next oldest
        auto m = make_msg(4, {4, 4, 4});
        CHECK(b->put(m.time, m.size, m.bytes) == true);
    }
    b->next_buffer(10);

    std::vector<MidiStorageElem> expect = {
        make_msg(3, {3, 3, 3}),
        make_msg(4, {4, 4, 4})
    };
    CHECK(b->n_events() == expect.size());
    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put then overflow then snapshot", "[MidiRingbuffer]") {
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    // enough for exactly 3 messages
    auto b = std::make_shared<Ringbuffer>(sizeof(Storage::Elem) * 3);

    b->set_n_samples(17);

    // Process samples such that the midi ringbuffer is exactly 4 samples removed
    // from having integer overflow on its time values.
    auto const target = std::numeric_limits<uint32_t>::max() - 2;
    while (b->get_current_end_time() < target) {
        auto const end_time = b->get_current_end_time();
        auto const process = std::min(512, (int)(target - end_time));
        b->next_buffer(process);
    }

    std::vector<MidiStorageElem> to_put = {
        make_msg(0, {0, 0, 0}),
        make_msg(2, {1, 1, 1}),
        make_msg(5, {2, 2, 2}),
    };
    for (auto &m : to_put) { CHECK(b->put(m.time, m.size, m.bytes) == true); }

    b->next_buffer(10);

    auto copy = shoop_make_shared<RustMidiStorage>(b->bytes_capacity());
    b->snapshot(*copy, 8);

    std::vector<MidiStorageElem> expect_b = {
        make_msg(10, {0, 0, 0}),
        make_msg(12, {1, 1, 1}),
        make_msg(15, {2, 2, 2})
    };
    std::vector<MidiStorageElem> expect_copy = {
        make_msg(1, {0, 0, 0}),
        make_msg(3, {1, 1, 1}),
        make_msg(6, {2, 2, 2})
    };
    CHECK(b->n_events() == expect_b.size());
    CHECK(extract_messages(*b) == expect_b);

    CHECK(copy->n_events() == expect_copy.size());
    CHECK(extract_messages(*copy) == expect_copy);
}

TEST_CASE("MidiRingbuffer - Put then truncated snapshot", "[MidiRingbuffer]") {
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    // enough for exactly 3 messages
    auto b = std::make_shared<Ringbuffer>(sizeof(Storage::Elem) * 3);

    b->set_n_samples(20);
    b->next_buffer(10);

    std::vector<MidiStorageElem> to_put = {
        make_msg(0, {0, 0, 0}),
        make_msg(2, {1, 1, 1}),
        make_msg(5, {2, 2, 2}),
    };
    for (auto &m : to_put) { CHECK(b->put(m.time, m.size, m.bytes) == true); }

    b->next_buffer(10);

    auto copy = shoop_make_shared<RustMidiStorage>(b->bytes_capacity());
    b->snapshot(*copy, 17);

    std::vector<MidiStorageElem> expect_b = {
        make_msg(0, {0, 0, 0}),
        make_msg(2, {1, 1, 1}),
        make_msg(5, {2, 2, 2})
    };
    std::vector<MidiStorageElem> expect_copy = {
        make_msg(2, {2, 2, 2})
    };
    CHECK(b->n_events() == expect_b.size());
    CHECK(extract_messages(*b) == expect_b);

    CHECK(copy->n_events() == expect_copy.size());
    CHECK(extract_messages(*copy) == expect_copy);
}