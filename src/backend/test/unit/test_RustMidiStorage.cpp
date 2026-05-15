#include "RustMidiStorage.h"
#include "MidiStorage.h"
#include "shoop_shared_ptr.h"

#include <iostream>

#include <catch2/catch_test_macros.hpp>

namespace Catch {
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
}

inline MidiStorageElem make_msg(uint32_t time, std::initializer_list<uint8_t> data) {
    MidiStorageElem msg;
    msg.time = time;
    msg.size = static_cast<uint16_t>(data.size());
    memcpy(msg.bytes, data.begin(), data.size());
    return msg;
}

TEST_CASE("RustMidiStorage - Basic append and query", "[RustMidiStorage]") {
    auto storage = shoop_make_shared<RustMidiStorage>(64);
    
    // Test empty state
    CHECK(storage->empty() == true);
    CHECK(storage->n_events() == 0);
    CHECK(storage->full() == false);
    
    // Append a message
    auto msg = make_msg(100, {0x90, 0x3C, 0x64});
    bool appended = storage->append(msg.time, msg.size, msg.bytes);
    CHECK(appended == true);
    
    // Test non-empty state
    CHECK(storage->empty() == false);
    CHECK(storage->n_events() == 1);
    CHECK(storage->full() == false);
    
    // Test get_elem
    auto* elem = storage->get_elem(0);
    CHECK(elem != nullptr);
    CHECK(elem->time == 100);
    CHECK(elem->size == 3);
    CHECK(elem->bytes[0] == 0x90);
    CHECK(elem->bytes[1] == 0x3C);
    CHECK(elem->bytes[2] == 0x64);
}

TEST_CASE("RustMidiStorage - Multiple appends", "[RustMidiStorage]") {
    auto storage = shoop_make_shared<RustMidiStorage>(128);
    
    // Append multiple messages
    auto msgs = {
        make_msg(0, {0x90, 0x3C, 0x64}),
        make_msg(10, {0x90, 0x40, 0x64}),
        make_msg(20, {0x90, 0x43, 0x64}),
    };
    
    int count = 0;
    for (auto& msg : msgs) {
        bool appended = storage->append(msg.time, msg.size, msg.bytes);
        CHECK(appended == true);
        count++;
    }
    
    CHECK(storage->n_events() == 3);
    
    // Test iteration
    int idx = 0;
    storage->for_each_msg([&idx](uint32_t time, uint16_t size, uint8_t* data) {
        CHECK(size == 3);
        idx++;
    });
    CHECK(idx == 3);
}

TEST_CASE("RustMidiStorage - Clear", "[RustMidiStorage]") {
    auto storage = shoop_make_shared<RustMidiStorage>(64);
    
    // Add some messages
    auto msg = make_msg(100, {0x90, 0x3C, 0x64});
    storage->append(msg.time, msg.size, msg.bytes);
    CHECK(storage->n_events() == 1);
    
    // Clear
    storage->clear();
    CHECK(storage->n_events() == 0);
    CHECK(storage->empty() == true);
}

TEST_CASE("RustMidiStorage - Copy to MidiStorage", "[RustMidiStorage]") {
    auto rust_storage = shoop_make_shared<RustMidiStorage>(128);
    
    // Add messages to RustMidiStorage
    auto msgs = {
        make_msg(0, {0x90, 0x3C, 0x64}),
        make_msg(10, {0x90, 0x40, 0x64}),
        make_msg(20, {0x90, 0x43, 0x64}),
    };
    
    for (auto& msg : msgs) {
        rust_storage->append(msg.time, msg.size, msg.bytes);
    }
    CHECK(rust_storage->n_events() == 3);
    
    // Copy to C++ MidiStorage
    auto cpp_storage = shoop_make_shared<MidiStorage>(128);
    rust_storage->copy(*cpp_storage);
    
    CHECK(cpp_storage->n_events() == 3);
    
    // Verify contents
    auto* elem = cpp_storage->get_elem(0);
    CHECK(elem->time == 0);
    elem = cpp_storage->get_elem(1);
    CHECK(elem->time == 10);
    elem = cpp_storage->get_elem(2);
    CHECK(elem->time == 20);
}

TEST_CASE("RustMidiStorage - Copy from MidiStorage", "[RustMidiStorage]") {
    auto cpp_storage = shoop_make_shared<MidiStorage>(128);
    
    // Add messages to C++ MidiStorage
    auto msgs = {
        make_msg(0, {0x90, 0x3C, 0x64}),
        make_msg(10, {0x90, 0x40, 0x64}),
        make_msg(20, {0x90, 0x43, 0x64}),
    };
    
    for (auto& msg : msgs) {
        cpp_storage->append(msg.time, msg.size, msg.bytes);
    }
    CHECK(cpp_storage->n_events() == 3);
    
    // Copy to RustMidiStorage
    auto rust_storage = shoop_make_shared<RustMidiStorage>(128);
    cpp_storage->copy(*rust_storage);
    
    CHECK(rust_storage->n_events() == 3);
    
    // Verify contents via RustMidiStorage
    auto* elem = rust_storage->get_elem(0);
    CHECK(elem->time == 0);
    elem = rust_storage->get_elem(1);
    CHECK(elem->time == 10);
    elem = rust_storage->get_elem(2);
    CHECK(elem->time == 20);
}

TEST_CASE("RustMidiStorage - Cursor operations", "[RustMidiStorage]") {
    auto storage = shoop_make_shared<RustMidiStorage>(128);
    
    // Add messages
    auto msgs = {
        make_msg(0, {0x90, 0x3C, 0x64}),
        make_msg(10, {0x90, 0x40, 0x64}),
        make_msg(20, {0x90, 0x43, 0x64}),
    };
    
    for (auto& msg : msgs) {
        storage->append(msg.time, msg.size, msg.bytes);
    }
    
    // Create cursor
    auto cursor = storage->create_cursor();
    CHECK(cursor != nullptr);
    CHECK(cursor->valid() == true);
    
    // First element
    auto* elem = cursor->get();
    CHECK(elem != nullptr);
    CHECK(elem->time == 0);
    
    // Next
    cursor->next();
    CHECK(cursor->valid() == true);
    elem = cursor->get();
    CHECK(elem->time == 10);
    
    // Next again
    cursor->next();
    CHECK(cursor->valid() == true);
    elem = cursor->get();
    CHECK(elem->time == 20);
    
    // End
    cursor->next();
    CHECK(cursor->valid() == false);
}

TEST_CASE("RustMidiStorage - Truncate", "[RustMidiStorage]") {
    auto storage = shoop_make_shared<RustMidiStorage>(128);
    
    // Add messages with times 0, 10, 20, 30, 40
    for (int i = 0; i < 5; i++) {
        auto msg = make_msg(i * 10, {0x90, 0x3C, 0x64});
        storage->append(msg.time, msg.size, msg.bytes);
    }
    CHECK(storage->n_events() == 5);
    
    // Truncate head (remove messages with time > 25)
    storage->truncate(25, MidiStorageTruncateSide::TruncateHead);
    CHECK(storage->n_events() == 3);
    
    // Verify remaining messages
    int idx = 0;
    storage->for_each_msg([&idx](uint32_t time, uint16_t, uint8_t*) {
        CHECK(time == idx * 10);
        idx++;
    });
    CHECK(idx == 3);
    
    // Truncate tail (remove messages with time < 15)
    storage->truncate(15, MidiStorageTruncateSide::TruncateTail);
    CHECK(storage->n_events() == 1);
    
    // Should only have time=20
    auto* elem = storage->get_elem(0);
    CHECK(elem->time == 20);
}

TEST_CASE("RustMidiStorage - Out of order rejection", "[RustMidiStorage]") {
    auto storage = shoop_make_shared<RustMidiStorage>(64);
    
    // Add first message
    auto msg1 = make_msg(100, {0x90, 0x3C, 0x64});
    bool appended = storage->append(msg1.time, msg1.size, msg1.bytes);
    CHECK(appended == true);
    CHECK(storage->n_events() == 1);
    
    // Try to add out-of-order message (earlier time)
    auto msg2 = make_msg(50, {0x90, 0x40, 0x64});
    appended = storage->append(msg2.time, msg2.size, msg2.bytes);
    CHECK(appended == false);  // Should be rejected
    CHECK(storage->n_events() == 1);  // Still only 1 message
}

TEST_CASE("RustMidiStorage - Capacity queries", "[RustMidiStorage]") {
    auto storage = shoop_make_shared<RustMidiStorage>(256);
    
    // Check capacity
    uint32_t cap = storage->capacity_elems();
    CHECK(cap > 0);
    
    uint32_t bytes_cap = storage->bytes_capacity();
    CHECK(bytes_cap == cap * sizeof(MidiStorageElem));
    
    // Raw accessors
    CHECK(storage->raw_capacity() == cap);
    CHECK(storage->raw_tail() == 0);
    CHECK(storage->raw_head() == 0);
    CHECK(storage->raw_full() == false);
}