#include "MidiStorage.h"
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

TEST_CASE("MidiStorage - Round-trip", "[MidiStorage]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Storage = MidiStorage;

    std::vector<Msg> in = {
        Msg(0, 3, {0, 1, 2}),
        Msg(1, 3, {3, 4, 5}),
        Msg(10, 1, {10})
    };
    uint32_t total_data_size = in.size() * sizeof(Storage::Elem);

    auto s = shoop_make_shared<Storage>(total_data_size);

    for(auto &i: in) { s->append(i.time, i.size, i.data.data()); }

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 3);

    std::vector<Msg> out;
    auto cursor = s->create_cursor();
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

    CHECK(out == in);
};

TEST_CASE("MidiStorage - prepend", "[MidiStorage]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Storage = MidiStorage;

    std::vector<Msg> in = {
        Msg(10, 3, {0, 1, 2}),
        Msg(11, 3, {3, 4, 5})
    };
    std::vector<Msg> prepend = {
        Msg(9, 1, {10}),
        Msg(8, 1, {10}),
    };
    std::vector<Msg> expect_result = {
        Msg(8, 1, {10}),
        Msg(9, 1, {10}),
        Msg(10, 3, {0, 1, 2}),
        Msg(11, 3, {3, 4, 5})
    };
    uint32_t total_data_size = (in.size() + prepend.size()) * sizeof(Storage::Elem);

    auto s = shoop_make_shared<Storage>(total_data_size);

    for(auto &i: in) { s->append(i.time, i.size, i.data.data()); }
    for(auto &i: prepend) { s->prepend(i.time, i.size, i.data.data()); }

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 4);

    std::vector<Msg> out;
    auto cursor = s->create_cursor();
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

    CHECK(out == expect_result);
};

TEST_CASE("MidiStorage - replace append", "[MidiStorage]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Storage = MidiStorage;

    std::vector<Msg> in = {
        Msg(0, 3, {0, 1, 2}),
        Msg(1, 3, {3, 4, 5}),
        Msg(10, 1, {10})
    };
    Msg append = Msg(11, 3, {4, 5, 6});
    std::vector<Msg> expect_result = {
        Msg(1, 3, {3, 4, 5}),
        Msg(10, 1, {10}),
        Msg(11, 3, {4, 5, 6})
    };
    uint32_t total_data_size = in.size() * sizeof(Storage::Elem);

    auto s = shoop_make_shared<Storage>(total_data_size);

    for(auto &i: in) { s->append(i.time, i.size, i.data.data()); }

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 3);

    CHECK(s->append(append.time, append.size, append.data.data()) == false);
    CHECK(s->append(append.time, append.size, append.data.data(), true) == true);

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 3);

    std::vector<Msg> out;
    auto cursor = s->create_cursor();
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

    CHECK(out == expect_result);
};

TEST_CASE("MidiStorage - wrap around", "[MidiStorage]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Storage = MidiStorage;

    // Buffer holds exactly 3 elements
    auto s = shoop_make_shared<Storage>(3 * sizeof(Storage::Elem));

    const uint8_t d0[] = {0, 0, 0};
    const uint8_t d1[] = {1, 1, 1};
    const uint8_t d2[] = {2, 2, 2};
    const uint8_t d3[] = {3, 3, 3};
    const uint8_t d4[] = {4, 4, 4};
    s->append(0, 3, d0);
    s->append(1, 3, d1);
    s->append(2, 3, d2);
    CHECK(s->n_events() == 3);

    // This overwrites the oldest (time 0)
    s->append(3, 3, d3, true);
    CHECK(s->n_events() == 3);

    // And again (overwrites time 1)
    s->append(4, 3, d4, true);
    CHECK(s->n_events() == 3);

    std::vector<Msg> expect = {
        Msg(2, 3, {2, 2, 2}),
        Msg(3, 3, {3, 3, 3}),
        Msg(4, 3, {4, 4, 4})
    };

    std::vector<Msg> out;
    auto cursor = s->create_cursor();
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
