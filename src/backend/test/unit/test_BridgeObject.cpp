#include <catch2/catch_test_macros.hpp>

#include "AudioMidiDriver.h"
#include "BridgeObject.h"
#include "DummyMidiPort.h"
#include "IProcessor.h"

#include <memory>
#include <vector>

struct DummyProcessor : IProcessor {
    uint32_t processed = 0;
    void PROC_process(uint32_t nframes) override { processed += nframes; }
};

TEST_CASE("BridgeObject - processor strong downgrades and weak upgrades", "[BridgeObject]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = std::make_unique<ProcessorBridgeStrong>(proc);
    auto weak = strong->downgrade();

    REQUIRE(strong);
    REQUIRE(weak);
    auto upgraded = weak->upgrade();
    REQUIRE(upgraded);
    CHECK(upgraded->shared_ptr() == proc);
    CHECK(weak->lock() == proc);
}

TEST_CASE("BridgeObject - processor weak expires when strong references are gone", "[BridgeObject]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = std::make_unique<ProcessorBridgeStrong>(proc);
    auto weak = strong->downgrade();

    strong.reset();
    proc.reset();

    CHECK(!weak->upgrade());
    CHECK(!weak->lock());
}

TEST_CASE("BridgeObject - strong handle gives access to contained object", "[BridgeObject]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = std::make_unique<ProcessorBridgeStrong>(proc);

    CHECK(&strong->get_ref() == proc.get());
    strong->get_pin_mut().PROC_process(64);
    CHECK(proc->processed == 64);
}

TEST_CASE("BridgeObject - processor weak handles are moveable containers", "[BridgeObject][container]") {
    auto proc = std::make_shared<DummyProcessor>();
    auto strong = std::make_unique<ProcessorBridgeStrong>(proc);

    std::vector<std::unique_ptr<ProcessorBridgeWeak>> handles;
    handles.push_back(strong->downgrade());
    handles.push_back(strong->downgrade());

    REQUIRE(handles.size() == 2);
    CHECK(handles[0]->lock() == proc);
    CHECK(handles[1]->lock() == proc);
}

