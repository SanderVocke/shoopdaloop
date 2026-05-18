#include "RustMidiStorage.h"
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

inline MidiStorageElem make_msg(uint32_t time, std::initializer_list<uint8_t> data) {
    MidiStorageElem msg;
    msg.time = time;
    msg.size = static_cast<uint16_t>(data.size());
    memcpy(msg.bytes, data.begin(), data.size());
    return msg;
}

// Test MidiStorage interface via FFI to Rust implementation (RustMidiStorage)
TEST_CASE("MidiStorage via Rust - Round-trip", "[MidiStorage]") {
    using Storage = RustMidiStorage;

    std::vector<MidiStorageElem> in = {
        make_msg(0, {0, 1, 2}),
        make_msg(1, {3, 4, 5}),
        make_msg(10, {10})
    };
    uint32_t total_data_size = in.size() * sizeof(Storage::Elem);

    auto s = shoop_make_shared<Storage>(total_data_size);

    for(auto &i: in) { s->append(i.time, i.size, i.bytes); }

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 3);

    std::vector<MidiStorageElem> out;
    auto cursor = s->create_cursor();
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

    CHECK(out == in);
};

TEST_CASE("MidiStorage via Rust - prepend", "[MidiStorage]") {
    using Storage = RustMidiStorage;

    std::vector<MidiStorageElem> in = {
        make_msg(10, {0, 1, 2}),
        make_msg(11, {3, 4, 5})
    };
    std::vector<MidiStorageElem> prepend = {
        make_msg(9, {10}),
        make_msg(8, {10}),
    };
    std::vector<MidiStorageElem> expect_result = {
        make_msg(8, {10}),
        make_msg(9, {10}),
        make_msg(10, {0, 1, 2}),
        make_msg(11, {3, 4, 5})
    };
    uint32_t total_data_size = (in.size() + prepend.size()) * sizeof(Storage::Elem);

    auto s = shoop_make_shared<Storage>(total_data_size);

    for(auto &i: in) { s->append(i.time, i.size, i.bytes); }
    for(auto &i: prepend) { s->prepend(i.time, i.size, i.bytes); }

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 4);

    std::vector<MidiStorageElem> out;
    auto cursor = s->create_cursor();
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

    CHECK(out == expect_result);
};

TEST_CASE("MidiStorage via Rust - replace append", "[MidiStorage]") {
    using Storage = RustMidiStorage;

    std::vector<MidiStorageElem> in = {
        make_msg(0, {0, 1, 2}),
        make_msg(1, {3, 4, 5}),
        make_msg(10, {10})
    };
    MidiStorageElem append = make_msg(11, {4, 5, 6});
    std::vector<MidiStorageElem> expect_result = {
        make_msg(1, {3, 4, 5}),
        make_msg(10, {10}),
        make_msg(11, {4, 5, 6})
    };
    uint32_t total_data_size = in.size() * sizeof(Storage::Elem);

    auto s = shoop_make_shared<Storage>(total_data_size);

    for(auto &i: in) { s->append(i.time, i.size, i.bytes); }

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 3);

    CHECK(s->append(append.time, append.size, append.bytes) == false);
    CHECK(s->append(append.time, append.size, append.bytes, true) == true);

    CHECK(s->bytes_occupied() == s->bytes_capacity());
    CHECK(s->bytes_free() == 0);
    CHECK(s->n_events() == 3);

    std::vector<MidiStorageElem> out;
    auto cursor = s->create_cursor();
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

    CHECK(out == expect_result);
};

TEST_CASE("MidiStorage via Rust - wrap around", "[MidiStorage]") {
    using Storage = RustMidiStorage;

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

    std::vector<MidiStorageElem> expect = {
        make_msg(2, {2, 2, 2}),
        make_msg(3, {3, 3, 3}),
        make_msg(4, {4, 4, 4})
    };

    std::vector<MidiStorageElem> out;
    auto cursor = s->create_cursor();
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

    CHECK(out == expect);
};