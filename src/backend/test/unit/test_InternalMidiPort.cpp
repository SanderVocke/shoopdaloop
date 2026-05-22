#include "InternalMidiPort.h"
#include "catch2/catch_approx.hpp"
#include <catch2/catch_test_macros.hpp>

TEST_CASE("Ports - Internal Midi - Properties", "[InternalMidiPort][ports][midi]") {
    InternalMidiPort port("dummy", 0, 0);

    CHECK(port.has_internal_read_access());
    CHECK(port.has_internal_write_access());
    CHECK(!port.has_implicit_input_source());
    CHECK(!port.has_implicit_output_sink());
    CHECK(port.type() == PortDataType::Midi);
    CHECK(std::string(port.name()) == "dummy");
}

TEST_CASE("Ports - Internal Midi - Write and Read", "[InternalMidiPort][ports][midi]") {
    InternalMidiPort port("dummy", 0, 0);

    MidiStorageElem msg;
    msg.time = 10;
    msg.size = 3;
    msg.bytes[0] = 0x90;
    msg.bytes[1] = 0x3C;
    msg.bytes[2] = 0x7F;

    port.write_event(msg);
    CHECK(port.n_events() == 1);

    auto read = port.get_event(0);
    CHECK(read.time == 10);
    CHECK(read.size == 3);
    CHECK(read.bytes[0] == 0x90);
    CHECK(read.bytes[1] == 0x3C);
    CHECK(read.bytes[2] == 0x7F);
}

TEST_CASE("Ports - Internal Midi - Prepare Clears", "[InternalMidiPort][ports][midi]") {
    InternalMidiPort port("dummy", 0, 0);

    MidiStorageElem msg;
    msg.time = 5;
    msg.size = 2;
    msg.bytes[0] = 0x90;
    msg.bytes[1] = 0x40;

    port.write_event(msg);
    CHECK(port.n_events() == 1);

    port.PROC_prepare(128);
    CHECK(port.n_events() == 0);
}

TEST_CASE("Ports - Internal Midi - Mute Passthrough", "[InternalMidiPort][ports][midi]") {
    InternalMidiPort port("dummy", 0, 0);
    port.set_muted(true);
    CHECK(port.get_muted());
    port.set_muted(false);
    CHECK(!port.get_muted());
}
