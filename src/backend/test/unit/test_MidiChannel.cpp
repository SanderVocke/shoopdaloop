#include "MidiChannel.h"
#include "RustMidiStorage.h"
#include "helpers.h"

#include <catch2/catch_test_macros.hpp>

TEST_CASE("MidiChannel - Set Contents - Indefinite size", "[MidiChannel]") {
    using Storage = RustMidiStorage;
    using Channel = MidiChannel;
    using Contents = Channel::Contents;

    auto c = shoop_make_shared<MidiChannel>(1, ChannelMode_Direct);

    std::vector<MidiStorageElem> data;
    {
        MidiStorageElem msg;
        msg.time = 0; msg.size = 3; msg.bytes[0] = 0; msg.bytes[1] = 1; msg.bytes[2] = 2; data.push_back(msg);
        msg.time = 1; msg.size = 3; msg.bytes[0] = 3; msg.bytes[1] = 4; msg.bytes[2] = 5; data.push_back(msg);
        msg.time = 10; msg.size = 1; msg.bytes[0] = 10; msg.bytes[1] = 0; msg.bytes[2] = 0; data.push_back(msg);
    }
    const Contents contents = { data, {} };

    c->set_contents(contents, 1000, false);
    CHECK(c->retrieve_contents(false).recorded_msgs == data);
};
