#include <catch2/catch_test_macros.hpp>
#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include "DummyAudioMidiDriver.h"
#include "DummyMidiPort.h"
#include "BridgeObject.h"
#include "backend_rust/src/bridge_object_cxx.rs.h"

#include <vector>
#include <set>

// Comparator so BridgeWeakHandle works in std::set (no built-in < operator).
struct BridgeWeakHandleLess {
    bool operator()(bridge_object::BridgeWeakHandle a, bridge_object::BridgeWeakHandle b) const {
        return std::tie(a.id, a.type_id) < std::tie(b.id, b.type_id);
    }
};

struct DummyProcessor : HasAudioProcessingFunction {
    void PROC_process(uint32_t) override {}
};


TEST_CASE("BridgeObject - type_id and id fields are plain POD", "[BridgeObject][pod]") {
    bridge_object::BridgeStrongHandle sh{123, 456};
    bridge_object::BridgeWeakHandle wh{789, 2};

    static_assert(sizeof(sh.id) == sizeof(uint64_t), "id is not uint64_t-sized");
    static_assert(sizeof(sh.type_id) == sizeof(uint32_t), "type_id is not uint32_t-sized");

    CHECK(sh.id == 123);
    CHECK(sh.type_id == 456);
    CHECK(wh.id == 789);
    CHECK(wh.type_id == 2);

    // Handles are trivially copyable.
    auto sh2 = sh;
    auto wh2 = wh;
    CHECK(sh2.id == sh.id);
    CHECK(wh2.type_id == wh.type_id);
}

TEST_CASE("BridgeObject - downgrade produces correct weak handle", "[BridgeObject]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong = bridge_object::register_processor(proc);
    CHECK(strong.id != 0);
    CHECK(strong.type_id == static_cast<uint32_t>(bridge_object::BridgeObjectType::Processor));

    auto weak = bridge_object::downgrade(strong);
    CHECK(weak.id == strong.id);
    CHECK(weak.type_id == strong.type_id);

    bridge_object::release_strong(strong);
}

TEST_CASE("BridgeObject - clone_strong yields another strong to the same object", "[BridgeObject]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong1 = bridge_object::register_processor(proc);

    auto strong2 = bridge_object::clone_strong(strong1);
    CHECK(strong2.id == strong1.id);
    CHECK(strong2.type_id == strong1.type_id);

    // Both strongs resolve to the same shared_ptr.
    auto locked1 = bridge_object::lock_processor(bridge_object::BridgeWeakHandle{strong1.id, strong1.type_id});
    auto locked2 = bridge_object::lock_processor(bridge_object::BridgeWeakHandle{strong2.id, strong2.type_id});
    CHECK(locked1.has_value());
    CHECK(locked2.has_value());

    bridge_object::release_strong(strong1);
    bridge_object::release_strong(strong2);
}

TEST_CASE("BridgeObject - release_strong makes weak lock fail", "[BridgeObject]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong = bridge_object::register_processor(proc);
    auto weak = bridge_object::downgrade(strong);

    bridge_object::release_strong(strong);

    auto locked = bridge_object::lock_processor(weak);
    CHECK(!locked.has_value());
}

TEST_CASE("BridgeObject - zero-id handles produce nullopt", "[BridgeObject]") {
    bridge_object::BridgeWeakHandle zero{0, 0};
    CHECK(!bridge_object::lock_processor(zero).has_value());
    CHECK(!bridge_object::lock_decoupled_midi_port(zero).has_value());
}

TEST_CASE("BridgeObject - wrong type_id for lock fails", "[BridgeObject]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong = bridge_object::register_processor(proc);
    auto weak = bridge_object::downgrade(strong);

    // Register a decoupled port with type_id == 2.
    auto dummy_port = shoop_make_shared<DummyMidiPort>("", shoop_port_direction_t::ShoopPortDirection_Input);
    auto decoupled = shoop_make_shared<shoop_types::_DecoupledMidiPort>(
        dummy_port, shoop_weak_ptr<AudioMidiDriver>(), 128,
        shoop_port_direction_t::ShoopPortDirection_Input);
    auto decoupled_strong = bridge_object::register_decoupled_midi_port(decoupled);
    auto decoupled_weak = bridge_object::downgrade(decoupled_strong);

    // Lock decoupled weak with wrong (processor) type_id — should fail.
    auto bad_lock = bridge_object::lock_decoupled_midi_port(
        bridge_object::BridgeWeakHandle{decoupled_weak.id, weak.type_id});
    CHECK(!bad_lock.has_value());

    // Correct type_id still works.
    auto good_lock = bridge_object::lock_decoupled_midi_port(decoupled_weak);
    CHECK(good_lock.has_value());

    bridge_object::release_strong(strong);
    bridge_object::release_strong(decoupled_strong);
}

TEST_CASE("BridgeObject - handles can be stored in std::vector", "[BridgeObject][container]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong = bridge_object::register_processor(proc);
    auto weak = bridge_object::downgrade(strong);

    std::vector<bridge_object::BridgeWeakHandle> vec;
    vec.push_back(weak);
    vec.push_back(weak);
    vec.push_back(weak);

    CHECK(vec.size() == 3);
    CHECK(vec[0].id == weak.id);
    CHECK(vec[1].id == weak.id);
    CHECK(vec[2].id == weak.id);

    for (auto& h : vec) {
        auto locked = bridge_object::lock_processor(h);
        CHECK(locked.has_value());
    }

    bridge_object::release_strong(strong);
}

TEST_CASE("BridgeObject - handles can be stored in std::set with custom comparator", "[BridgeObject][container]") {
    auto proc1 = shoop_make_shared<DummyProcessor>();
    auto proc2 = shoop_make_shared<DummyProcessor>();

    auto strong1 = bridge_object::register_processor(proc1);
    auto strong2 = bridge_object::register_processor(proc2);

    auto weak1 = bridge_object::downgrade(strong1);
    auto weak2 = bridge_object::downgrade(strong2);

    std::set<bridge_object::BridgeWeakHandle, BridgeWeakHandleLess> s;
    s.insert(weak1);
    s.insert(weak2);
    s.insert(weak1); // duplicate — set must handle it

    CHECK(s.size() == 2);

    std::vector<uint64_t> ids;
    for (auto& h : s) {
        auto locked = bridge_object::lock_processor(h);
        CHECK(locked.has_value());
        ids.push_back(h.id);
    }
    CHECK(ids[0] != ids[1]);

    bridge_object::release_strong(strong1);
    bridge_object::release_strong(strong2);
}

TEST_CASE("BridgeObject - processor and decoupled port have distinct type IDs", "[BridgeObject]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto dummy_port = shoop_make_shared<DummyMidiPort>("", shoop_port_direction_t::ShoopPortDirection_Input);

    auto proc_strong = bridge_object::register_processor(proc);
    auto decoupled = shoop_make_shared<shoop_types::_DecoupledMidiPort>(
        dummy_port, shoop_weak_ptr<AudioMidiDriver>(), 128,
        shoop_port_direction_t::ShoopPortDirection_Input);
    auto decoupled_strong = bridge_object::register_decoupled_midi_port(decoupled);

    CHECK(proc_strong.type_id != decoupled_strong.type_id);
    CHECK(proc_strong.type_id == static_cast<uint32_t>(bridge_object::BridgeObjectType::Processor));
    CHECK(decoupled_strong.type_id == static_cast<uint32_t>(bridge_object::BridgeObjectType::DecoupledMidiPort));

    bridge_object::release_strong(proc_strong);
    bridge_object::release_strong(decoupled_strong);
}

TEST_CASE("BridgeObject - lock succeeds before release, nullopt after", "[BridgeObject]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong = bridge_object::register_processor(proc);
    auto weak = bridge_object::downgrade(strong);

    auto before = bridge_object::lock_processor(weak);
    CHECK(before.has_value());

    bridge_object::release_strong(strong);

    auto after = bridge_object::lock_processor(weak);
    CHECK(!after.has_value());
}

TEST_CASE("BridgeObject - clone_strong on invalid handle returns empty", "[BridgeObject]") {
    bridge_object::BridgeStrongHandle zero{0, 0};
    auto result = bridge_object::clone_strong(zero);
    CHECK(result.id == 0);
    CHECK(result.type_id == 0);
}

TEST_CASE("BridgeObject - downgrade on empty handle produces empty weak", "[BridgeObject]") {
    bridge_object::BridgeStrongHandle zero{0, 0};
    auto weak = bridge_object::downgrade(zero);
    CHECK(weak.id == 0);
    CHECK(weak.type_id == 0);
}

TEST_CASE("BridgeObject - multiple clones all resolve and share the object", "[BridgeObject]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong1 = bridge_object::register_processor(proc);
    auto strong2 = bridge_object::clone_strong(strong1);
    auto strong3 = bridge_object::clone_strong(strong1);

    auto weak1 = bridge_object::downgrade(strong1);
    auto weak2 = bridge_object::downgrade(strong2);
    auto weak3 = bridge_object::downgrade(strong3);

    auto locked1 = bridge_object::lock_processor(weak1);
    auto locked2 = bridge_object::lock_processor(weak2);
    auto locked3 = bridge_object::lock_processor(weak3);
    CHECK(locked1.has_value());
    CHECK(locked2.has_value());
    CHECK(locked3.has_value());

    // All point to the same underlying object.
    CHECK(locked1.value().get() == locked2.value().get());
    CHECK(locked2.value().get() == locked3.value().get());

    bridge_object::release_strong(strong1);
    bridge_object::release_strong(strong2);
    bridge_object::release_strong(strong3);
}

TEST_CASE("BridgeObject - distinct objects get distinct registry IDs", "[BridgeObject]") {
    auto proc1 = shoop_make_shared<DummyProcessor>();
    auto proc2 = shoop_make_shared<DummyProcessor>();
    auto proc3 = shoop_make_shared<DummyProcessor>();

    auto strong1 = bridge_object::register_processor(proc1);
    auto strong2 = bridge_object::register_processor(proc2);
    auto strong3 = bridge_object::register_processor(proc3);

    CHECK(strong1.id != strong2.id);
    CHECK(strong2.id != strong3.id);
    CHECK(strong1.id != strong3.id);
    CHECK(strong1.type_id == strong2.type_id);
    CHECK(strong2.type_id == strong3.type_id);

    bridge_object::release_strong(strong1);
    bridge_object::release_strong(strong2);
    bridge_object::release_strong(strong3);
}

TEST_CASE("BridgeObject - Rust-facing upgrade/release shims operate on C++ registry", "[BridgeObject][rust-facing]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong = bridge_object::register_processor(proc);
    auto weak = bridge_object::downgrade(strong);

    CHECK(backend_rust::bridge_upgrade_for_rust(weak.id, weak.type_id));
    auto locked_after_upgrade = bridge_object::lock_processor(weak);
    CHECK(locked_after_upgrade.has_value());

    backend_rust::bridge_release_strong_for_rust(weak.id, weak.type_id);
    auto locked_after_release = bridge_object::lock_processor(weak);
    CHECK(!locked_after_release.has_value());
}

TEST_CASE("BridgeObject - Rust-facing upgrade shim rejects invalid weak handles", "[BridgeObject][rust-facing]") {
    CHECK(!backend_rust::bridge_upgrade_for_rust(0, 0));
    CHECK(!backend_rust::bridge_upgrade_for_rust(99999, 1));
}

TEST_CASE("BridgeObject - C++ generic shims operate on Rust-owned registry entries", "[BridgeObject][rust-facing]") {
    constexpr uint32_t type_id = 4242;
    auto strong = backend_rust::bridge_test_register_rust_object(type_id);
    CHECK(backend_rust::bridge_is_rust_owned(strong.id));

    std::vector<backend_rust::BridgeStrongHandle> strong_vec{strong};
    CHECK(strong_vec[0].id == strong.id);

    CHECK(backend_rust::bridge_upgrade_generic(strong.id, strong.type_id));
    backend_rust::bridge_release_strong_generic(strong.id, strong.type_id);

    // The test helper does not retain an external Arc, so after release the weak is stale.
    CHECK(!backend_rust::bridge_upgrade_generic(strong.id, strong.type_id));
}

TEST_CASE("BridgeObject - C++ generic shims reject Rust-owned type mismatches", "[BridgeObject][rust-facing]") {
    auto strong = backend_rust::bridge_test_register_rust_object(5151);
    CHECK(backend_rust::bridge_is_rust_owned(strong.id));

    CHECK(!backend_rust::bridge_upgrade_generic(strong.id, 5152));
    backend_rust::bridge_release_strong_generic(strong.id, strong.type_id);
}

TEST_CASE("BridgeObject - lock on unregistered ID returns nullopt", "[BridgeObject]") {
    auto proc = shoop_make_shared<DummyProcessor>();
    auto strong = bridge_object::register_processor(proc);
    auto weak = bridge_object::downgrade(strong);
    bridge_object::release_strong(strong);

    // ID is now unregistered — lock should fail.
    CHECK(!bridge_object::lock_processor(weak).has_value());

    // Also test with a clearly bogus ID.
    bridge_object::BridgeWeakHandle bogus{99999, 1};
    CHECK(!bridge_object::lock_processor(bogus).has_value());
    CHECK(!bridge_object::lock_decoupled_midi_port(bogus).has_value());
}