#include <catch2/catch_test_macros.hpp>

#include "AudioMidiDriver.h"
#include "BridgeObject.h"
#include "DecoupledMidiPort.h"
#include "DummyMidiPort.h"
#include "HasAudioProcessingFunction.h"

#include <memory>
#include <vector>

struct DummyProcessor : HasAudioProcessingFunction {
    uint32_t processed = 0;
    void PROC_process(uint32_t nframes) override { processed += nframes; }
};

TEST_CASE("BridgeObject - processor strong downgrades and weak upgrades", "[BridgeObject]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = make_processor_bridge_strong(proc);
    auto weak = strong->downgrade_unique();

    REQUIRE(strong);
    REQUIRE(weak);
    auto upgraded = weak->upgrade();
    REQUIRE(upgraded);
    CHECK(upgraded->shared_ptr() == proc);
    CHECK(processor_bridge_lock(*weak) == proc);
}

TEST_CASE("BridgeObject - processor weak expires when strong references are gone", "[BridgeObject]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = make_processor_bridge_strong(proc);
    auto weak = strong->downgrade_unique();

    strong.reset();
    proc.reset();

    CHECK(!weak->upgrade());
    CHECK(!processor_bridge_lock(*weak));
}

TEST_CASE("BridgeObject - processor operation helper calls object", "[BridgeObject]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = make_processor_bridge_strong(proc);
    auto weak = strong->downgrade_unique();

    processor_bridge_proc_process(*weak, 64);
    CHECK(proc->processed == 64);
}

TEST_CASE("BridgeObject - processor weak handles are moveable containers", "[BridgeObject][container]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = make_processor_bridge_strong(proc);

    std::vector<std::unique_ptr<ProcessorBridgeWeak>> handles;
    handles.push_back(strong->downgrade_unique());
    handles.push_back(strong->downgrade_unique());

    REQUIRE(handles.size() == 2);
    CHECK(processor_bridge_lock(*handles[0]) == proc);
    CHECK(processor_bridge_lock(*handles[1]) == proc);
}

TEST_CASE("BridgeObject - decoupled MIDI strong downgrades and weak upgrades", "[BridgeObject][decoupled]") {
    auto dummy_port = std::make_shared<DummyMidiPort>("", shoop_port_direction_t::ShoopPortDirection_Input);
    auto decoupled = std::make_shared<shoop_types::_DecoupledMidiPort>(
        dummy_port, std::weak_ptr<AudioMidiDriver>(), 128,
        shoop_port_direction_t::ShoopPortDirection_Input);

    auto strong = make_decoupled_midi_port_bridge_strong(decoupled);
    auto weak = strong->downgrade_unique();

    REQUIRE(strong);
    REQUIRE(weak);
    auto upgraded = weak->upgrade();
    REQUIRE(upgraded);
    CHECK(upgraded->shared_ptr() == decoupled);
    CHECK(decoupled_midi_port_bridge_lock(*weak) == decoupled);
}

TEST_CASE("BridgeObject - decoupled MIDI weak expires when strong references are gone", "[BridgeObject][decoupled]") {
    auto dummy_port = std::make_shared<DummyMidiPort>("", shoop_port_direction_t::ShoopPortDirection_Input);
    auto decoupled = std::make_shared<shoop_types::_DecoupledMidiPort>(
        dummy_port, std::weak_ptr<AudioMidiDriver>(), 128,
        shoop_port_direction_t::ShoopPortDirection_Input);

    auto strong = make_decoupled_midi_port_bridge_strong(decoupled);
    auto weak = strong->downgrade_unique();

    strong.reset();
    decoupled.reset();

    CHECK(!weak->upgrade());
    CHECK(!decoupled_midi_port_bridge_lock(*weak));
}
