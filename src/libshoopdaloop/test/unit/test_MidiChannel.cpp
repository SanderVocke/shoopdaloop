#include "MidiChannel.h"
#include "MidiMessage.h"

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

TEST_CASE("MidiChannel - Indefinite size", "[MidiChannel]") {
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
