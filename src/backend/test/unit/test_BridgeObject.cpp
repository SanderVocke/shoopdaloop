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
    auto strong = bridge_object::make_processor_bridge_strong(proc);
    auto weak = bridge_object::processor_bridge_downgrade(*strong);

    REQUIRE(strong);
    REQUIRE(weak);
    auto upgraded = bridge_object::processor_bridge_upgrade(*weak);
    REQUIRE(upgraded);
    CHECK(upgraded->shared_ptr() == proc);
    CHECK(bridge_object::processor_bridge_lock(*weak) == proc);
}

TEST_CASE("BridgeObject - processor weak expires when strong references are gone", "[BridgeObject]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = bridge_object::make_processor_bridge_strong(proc);
    auto weak = bridge_object::processor_bridge_downgrade(*strong);

    strong.reset();
    proc.reset();

    CHECK(!bridge_object::processor_bridge_upgrade(*weak));
    CHECK(!bridge_object::processor_bridge_lock(*weak));
}

TEST_CASE("BridgeObject - processor operation helper calls object", "[BridgeObject]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = bridge_object::make_processor_bridge_strong(proc);
    auto weak = bridge_object::processor_bridge_downgrade(*strong);

    bridge_object::processor_bridge_proc_process(*weak, 64);
    CHECK(proc->processed == 64);
}

TEST_CASE("BridgeObject - processor weak handles are moveable containers", "[BridgeObject][container]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = bridge_object::make_processor_bridge_strong(proc);

    std::vector<std::unique_ptr<bridge_object::ProcessorBridgeWeak>> handles;
    handles.push_back(bridge_object::processor_bridge_downgrade(*strong));
    handles.push_back(bridge_object::processor_bridge_downgrade(*strong));

    REQUIRE(handles.size() == 2);
    CHECK(bridge_object::processor_bridge_lock(*handles[0]) == proc);
    CHECK(bridge_object::processor_bridge_lock(*handles[1]) == proc);
}

TEST_CASE("BridgeObject - decoupled MIDI strong downgrades and weak upgrades", "[BridgeObject][decoupled]") {
    auto dummy_port = std::make_shared<DummyMidiPort>("", shoop_port_direction_t::ShoopPortDirection_Input);
    auto decoupled = std::make_shared<shoop_types::_DecoupledMidiPort>(
        dummy_port, std::weak_ptr<AudioMidiDriver>(), 128,
        shoop_port_direction_t::ShoopPortDirection_Input);

    auto strong = bridge_object::make_decoupled_midi_port_bridge_strong(decoupled);
    auto weak = bridge_object::decoupled_midi_port_bridge_downgrade(*strong);

    REQUIRE(strong);
    REQUIRE(weak);
    auto upgraded = bridge_object::decoupled_midi_port_bridge_upgrade(*weak);
    REQUIRE(upgraded);
    CHECK(upgraded->shared_ptr() == decoupled);
    CHECK(bridge_object::decoupled_midi_port_bridge_lock(*weak) == decoupled);
}

TEST_CASE("BridgeObject - decoupled MIDI weak expires when strong references are gone", "[BridgeObject][decoupled]") {
    auto dummy_port = std::make_shared<DummyMidiPort>("", shoop_port_direction_t::ShoopPortDirection_Input);
    auto decoupled = std::make_shared<shoop_types::_DecoupledMidiPort>(
        dummy_port, std::weak_ptr<AudioMidiDriver>(), 128,
        shoop_port_direction_t::ShoopPortDirection_Input);

    auto strong = bridge_object::make_decoupled_midi_port_bridge_strong(decoupled);
    auto weak = bridge_object::decoupled_midi_port_bridge_downgrade(*strong);

    strong.reset();
    decoupled.reset();

    CHECK(!bridge_object::decoupled_midi_port_bridge_upgrade(*weak));
    CHECK(!bridge_object::decoupled_midi_port_bridge_lock(*weak));
}
