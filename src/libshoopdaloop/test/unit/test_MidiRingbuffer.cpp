#include "MidiRingbuffer.h"
#include "MidiMessage.h"

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

TEST_CASE("MidiRingbuffer - Put and increment", "[MidiRingbuffer]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer<uint32_t, uint32_t>;
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

    std::vector<Msg> out;
    std::vector<Msg> expect = {
        Msg(0, 3, {0, 0, 0}),
        Msg(1, 3, {1, 1, 1}),
        Msg(12, 3, {2, 2, 2}),
        Msg(23, 3, {3, 3, 3})
    };

    auto cursor = b->create_cursor();
    while (cursor->valid()) {
        auto elem = cursor->get();
        out.push_back(Msg(
            elem->storage_time,
            elem->size,
            std::vector<uint8_t>(elem->data(), elem->data() + elem->size)
        ));
        cursor->next();
        if(cursor->is_at_start()) { break; }
    }

    CHECK(out == expect);
};

TEST_CASE("MidiRingbuffer - Put and truncate", "[MidiRingbuffer]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Ringbuffer = MidiRingbuffer<uint32_t, uint32_t>;
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

    auto cursor = b->create_cursor();
    while (cursor->valid()) {
        auto elem = cursor->get();
        out.push_back(Msg(
            elem->storage_time,
            elem->size,
            std::vector<uint8_t>(elem->data(), elem->data() + elem->size)
        ));
        cursor->next();
        if(cursor->is_at_start()) { break; }
    }

    CHECK(out == expect);
};