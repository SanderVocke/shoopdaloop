#include "MidiChannel.h"
#include "MidiMessage.h"
#include "helpers.h"

#include <catch2/catch_test_macros.hpp>

TEST_CASE("MidiChannel - Set Contents - Indefinite size", "[MidiChannel]") {
    using Msg = MidiMessage<uint32_t, uint16_t>;
    using Storage = MidiStorage;
    using Channel = MidiChannel;
    using Contents = Channel::Contents;

    auto c = shoop_make_shared<MidiChannel>(1, ChannelMode_Direct);

    const std::vector<Msg> data = {
        Msg(0, 3, {0, 1, 2}),
        Msg(1, 3, {3, 4, 5}),
        Msg(10, 1, {10})
    };
    const Contents contents = { data, {} };

    c->set_contents(contents, 1000, false);
    CHECK(c->retrieve_contents(false).recorded_msgs == data);
};
