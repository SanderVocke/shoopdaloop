#include "MidiStorage.h"
#include "MidiMessage.h"

#include <iostream>

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
    using Storage = MidiStorage<uint32_t, uint32_t>;

    std::vector<Msg> in = {
        Msg(0, 3, {0, 1, 2}),
        Msg(1, 3, {3, 4, 5}),
        Msg(10, 1, {10})
    };
    uint32_t total_data_size = 0;
    for(auto &i : in) { total_data_size += Storage::Elem::total_size_of(i.data.size()); }

    auto s = std::make_shared<Storage>(total_data_size);

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
    using Storage = MidiStorage<uint32_t, uint32_t>;

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
    uint32_t total_data_size = 0;
    for(auto &i : in) { total_data_size += Storage::Elem::total_size_of(i.data.size()); }
    for(auto &i : prepend) { total_data_size += Storage::Elem::total_size_of(i.data.size()); }

    auto s = std::make_shared<Storage>(total_data_size);

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
    using Storage = MidiStorage<uint32_t, uint32_t>;

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
    uint32_t total_data_size = 0;
    for(auto &i : in) { total_data_size += Storage::Elem::total_size_of(i.data.size()); }

    auto s = std::make_shared<Storage>(total_data_size);

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

TEST_CASE("MidiStorage - replace multiple append", "[MidiStorage]") {
    using Msg = MidiMessage<uint32_t, uint32_t>;
    using Storage = MidiStorage<uint32_t, uint32_t>;

    std::vector<Msg> in = {
        Msg(0, 3, {0, 1, 2}),
        Msg(1, 3, {3, 4, 5}),
        Msg(10, 1, {10})
    };
    Msg append = Msg(11, 5, {4, 5, 6, 7, 8});
    std::vector<Msg> expect_result = {
        Msg(10, 1, {10}),
        Msg(11, 5, {4, 5, 6, 7, 8})
    };
    uint32_t total_data_size = 0;
    for(auto &i : in) { total_data_size += Storage::Elem::total_size_of(i.data.size()); }

    auto s = std::make_shared<Storage>(total_data_size);

    for(auto &i: in) { s->append(i.time, i.size, i.data.data()); }

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 3);

    CHECK(s->append(append.time, append.size, append.data.data()) == false);
    CHECK(s->append(append.time, append.size, append.data.data(), true) == true);

    CHECK(s->bytes_occupied() == s->bytes_capacity() - Storage::Elem::total_size_of(1));
    CHECK(s->bytes_free() == Storage::Elem::total_size_of(1));
    CHECK(s->n_events() == 2);

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
