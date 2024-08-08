#include "MidiRingbuffer.h"
#include "MidiMessage.h"
#include "shoop_shared_ptr.h"

#include <iostream>
#include <sstream>

#include <catch2/catch_test_macros.hpp>

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
    struct StringMaker<MidiMessage<uint32_t, uint32_t>> {
        static std::string convert(const MidiMessage<uint32_t, uint32_t>& e) {
            std::ostringstream oss;
            oss << "{ t:" << e.time << ", s:" << e.size << ", d:" << StringMaker<std::vector<uint8_t>>::convert(e.data) << " }";
            return oss.str();
        }
    };

    template<>
    struct StringMaker<std::vector<MidiMessage<uint32_t, uint32_t>>> {
        static std::string convert(const std::vector<MidiMessage<uint32_t, uint32_t>>& vec) {
            std::ostringstream oss;
            oss << "[\n";
            for (auto &e : vec) {
                oss << "  " << StringMaker<MidiMessage<uint32_t, uint32_t>>::convert(e) << "\n";
            }
            oss << "]";
            return oss.str();
        }
    };
}

template<typename Storage>
std::vector<MidiMessage<uint32_t, uint32_t>> extract_messages(Storage &buf) {
    std::vector<MidiMessage<uint32_t, uint32_t>> out;
    auto cursor = buf.create_cursor();
    while (cursor->valid()) {
        auto elem = cursor->get();
        out.push_back(MidiMessage<uint32_t, uint32_t>(
            elem->storage_time,
            elem->size,
            std::vector<uint8_t>(elem->data(), elem->data() + elem->size)
        ));
        cursor->next();
        if(cursor->is_at_start()) { break; }
    }
    return out;
}

TEST_CASE("MidiRingbuffer - Put and increment", "[MidiRingbuffer]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    auto b = std::make_shared<Ringbuffer>(200);
    b->set_n_samples(50);
    b->next_buffer(10);

    {
        auto m = Msg(0, 3, {0, 0, 0});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        auto m = Msg(1, 3, {1, 1, 1});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    b->next_buffer(10);
    {
        auto m = Msg(2, 3, {2, 2, 2});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    b->next_buffer(10);
    {
        auto m = Msg(3, 3, {3, 3, 3});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }

    CHECK(b->n_events() == 4);

    std::vector<Msg> expect = {
        Msg(0, 3, {0, 0, 0}),
        Msg(1, 3, {1, 1, 1}),
        Msg(12, 3, {2, 2, 2}),
        Msg(23, 3, {3, 3, 3})
    };

    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put and truncate", "[MidiRingbuffer]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    auto b = std::make_shared<Ringbuffer>(200);
    b->set_n_samples(17);
    b->next_buffer(10);

    {
        auto m = Msg(0, 3, {0, 0, 0});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        auto m = Msg(1, 3, {1, 1, 1});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    b->next_buffer(10);
    {
        auto m = Msg(2, 3, {2, 2, 2});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        auto m = Msg(3, 3, {3, 3, 3});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    b->next_buffer(10);
    {
        auto m = Msg(3, 3, {4, 4, 4});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }

    std::vector<Msg> out;
    std::vector<Msg> expect = {
        Msg(13, 3, {3, 3, 3}),
        Msg(23, 3, {4, 4, 4})
    };
    CHECK(b->n_events() == expect.size());
    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put and wrap without filler", "[MidiRingbuffer]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    const size_t three_byte_msg_size = Storage::Elem::total_size_of(3);

    // enough for almost 4 messages
    auto b = std::make_shared<Ringbuffer>(three_byte_msg_size * 3);

    b->set_n_samples(10000);
    b->next_buffer(10);

    {
        // 1st message
        auto m = Msg(0, 3, {0, 0, 0});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 2nd message
        auto m = Msg(1, 3, {1, 1, 1});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 3rd message
        auto m = Msg(2, 3, {2, 2, 2});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 4th message, should be stored wrapped, mostly replaces 1st msg
        auto m = Msg(3, 2, {3, 3});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 5th message, should be stored wrapped after 4th, mostly replaces 2nd msg
        auto m = Msg(4, 3, {4, 4, 4});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    b->next_buffer(10);

    std::vector<Msg> out;
    std::vector<Msg> expect = {
        Msg(2, 3, {2, 2, 2}),
        Msg(3, 2, {3, 3}),
        Msg(4, 3, {4, 4, 4})
    };
    CHECK(b->n_events() == expect.size());
    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put and wrap with filler", "[MidiRingbuffer]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    const size_t three_byte_msg_size = Storage::Elem::total_size_of(3);

    // enough for almost 4 messages
    auto b = std::make_shared<Ringbuffer>(three_byte_msg_size * 4 - 2);

    b->set_n_samples(10000);
    b->next_buffer(10);

    {
        // 1st message
        auto m = Msg(0, 3, {0, 0, 0});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 2nd message
        auto m = Msg(1, 3, {1, 1, 1});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 3rd message
        auto m = Msg(2, 3, {2, 2, 2});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 4th message, should be stored wrapped, mostly replaces 1st msg
        auto m = Msg(3, 2, {3, 3});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 5th message, should be stored wrapped after 4th, mostly replaces 2nd msg
        auto m = Msg(4, 3, {4, 4, 4});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    b->next_buffer(10);

    std::vector<Msg> out;
    std::vector<Msg> expect = {
         Msg(2, 3, {2, 2, 2}),
        Msg(3, 2, {3, 3}),
        Msg(4, 3, {4, 4, 4})
    };
    CHECK(b->n_events() == expect.size());
    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put and wrap with filler big msg", "[MidiRingbuffer]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    const size_t three_byte_msg_size = Storage::Elem::total_size_of(3);

    // enough for almost 4 messages
    auto b = std::make_shared<Ringbuffer>(three_byte_msg_size * 4 - 2);

    b->set_n_samples(10000);
    b->next_buffer(10);

    {
        // 1st message
        auto m = Msg(0, 3, {0, 0, 0});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 2nd message
        auto m = Msg(1, 3, {1, 1, 1});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 3rd message
        auto m = Msg(2, 3, {2, 2, 2});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    auto const last_msg_data_sz = three_byte_msg_size * 2 - 2 - Storage::Elem::total_size_of(0);
    {
        // 4th message, should be stored wrapped, replaces 1st and 2nd
        auto m = Msg(3, last_msg_data_sz, std::vector<uint8_t>(last_msg_data_sz));
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    b->next_buffer(10);

    std::vector<Msg> out;
    std::vector<Msg> expect = {
        Msg(2, 3, {2, 2, 2}),
        Msg(3, last_msg_data_sz, std::vector<uint8_t>(last_msg_data_sz))
    };
    CHECK(b->n_events() == expect.size());
    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put and wrap then truncate", "[MidiRingbuffer]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    constexpr size_t elem_size = sizeof(Storage::Elem);
    constexpr size_t three_byte_msg_size = elem_size + 3;

    // enough for almost 4 messages
    auto b = std::make_shared<Ringbuffer>(three_byte_msg_size * 3 + (three_byte_msg_size - 2));

    b->set_n_samples(17);
    b->next_buffer(10);

    {
        // 1st message
        auto m = Msg(0, 3, {0, 0, 0});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 2nd message
        auto m = Msg(1, 3, {1, 1, 1});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 3rd message
        auto m = Msg(2, 3, {2, 2, 2});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 4th message, should be stored wrapped, mostly replaces 1st msg
        auto m = Msg(3, 2, {3, 3});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    {
        // 5th message, should be stored wrapped after 4th, mostly replaces 2nd msg
        auto m = Msg(4, 3, {4, 4, 4});
        CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true);
    }
    b->next_buffer(10);

    std::vector<Msg> out;
    std::vector<Msg> expect = {
        Msg(3, 2, {3, 3}),
        Msg(4, 3, {4, 4, 4})
    };
    CHECK(b->n_events() == expect.size());
    CHECK(extract_messages(*b) == expect);
};

TEST_CASE("MidiRingbuffer - Put then overflow then snapshot", "[MidiRingbuffer]") {
using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    constexpr size_t elem_size = sizeof(Storage::Elem);
    constexpr size_t three_byte_msg_size = elem_size + 3;

    // enough for 3 messages
    auto b = std::make_shared<Ringbuffer>(three_byte_msg_size * 3 + 1);

    b->set_n_samples(17);

    // Process samples such that the midi ringbuffer is exactly 4 samples removed
    // from having integer overflow on its time values.
    auto const target = std::numeric_limits<uint32_t>::max() - 2;
    while (b->get_current_end_time() < target) {
        auto const end_time = b->get_current_end_time();
        auto const process = std::min(512, (int)(target - end_time));
        b->next_buffer(process);
    }

    std::vector<Msg> to_put = {
        Msg(0, 3, {0, 0, 0}),
        Msg(2, 3, {1, 1, 1}),
        Msg(5, 3, {2, 2, 2}),
    };
    for (auto &m : to_put) { CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true); }

    b->next_buffer(10);

    auto copy = shoop_make_shared<MidiStorage>(b->bytes_capacity());
    b->snapshot(*copy, 8);

    std::vector<Msg> expect_b = {
        Msg(10, 3, {0, 0, 0}),
        Msg(12, 3, {1, 1, 1}),
        Msg(15, 3, {2, 2, 2})
    };
    std::vector<Msg> expect_copy = {
        Msg(1, 3, {0, 0, 0}),
        Msg(3, 3, {1, 1, 1}),
        Msg(6, 3, {2, 2, 2})
    };
    CHECK(b->n_events() == expect_b.size());
    CHECK(extract_messages(*b) == expect_b);

    CHECK(copy->n_events() == expect_copy.size());
    CHECK(extract_messages(*copy) == expect_copy);
}

TEST_CASE("MidiRingbuffer - Put then truncated snapshot", "[MidiRingbuffer]") {
using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer;
    using Storage = Ringbuffer::Storage;

    constexpr size_t elem_size = sizeof(Storage::Elem);
    constexpr size_t three_byte_msg_size = elem_size + 3;

    // enough for 3 messages
    auto b = std::make_shared<Ringbuffer>(three_byte_msg_size * 3 + 1);

    b->set_n_samples(20);
    b->next_buffer(10);

    std::vector<Msg> to_put = {
        Msg(0, 3, {0, 0, 0}),
        Msg(2, 3, {1, 1, 1}),
        Msg(5, 3, {2, 2, 2}),
    };
    for (auto &m : to_put) { CHECK(b->put(m.get_time(), m.get_size(), m.get_data()) == true); }

    b->next_buffer(10);

    auto copy = shoop_make_shared<MidiStorage>(b->bytes_capacity());
    b->snapshot(*copy, 17);

    std::vector<Msg> expect_b = {
        Msg(0, 3, {0, 0, 0}),
        Msg(2, 3, {1, 1, 1}),
        Msg(5, 3, {2, 2, 2})
    };
    std::vector<Msg> expect_copy = {
        Msg(2, 3, {2, 2, 2})
    };
    CHECK(b->n_events() == expect_b.size());
    CHECK(extract_messages(*b) == expect_b);

    CHECK(copy->n_events() == expect_copy.size());
    CHECK(extract_messages(*copy) == expect_copy);
}