#include <boost/ut.hpp>
#include <memory>
#include "AudioLoop.h"

using namespace boost::ut;
using namespace std::chrono_literals;

suite AudioLoop_tests = []() {
    "1_stop"_test = []() {
        auto pool = std::make_shared<AudioBufferPool<int>>(10, 256, 100ms);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        loop.process(1000);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);
    };
    "2_record"_test = []() {
        auto pool = std::make_shared<AudioBufferPool<int>>(10, 256, 100ms);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        loop.set_next_state(Recording);
        loop.transition_now();

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 256) << loop.get_next_poi().value_or(0); // end of buffer
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        loop.process(20);

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 246); // end of buffer
        expect(loop.get_length() == 10);
        expect(loop.get_position() == 0);
    };
};