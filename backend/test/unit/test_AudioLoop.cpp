#include <boost/ut.hpp>
#include <memory>
#include "AudioLoop.h"

using namespace boost::ut;
using namespace std::chrono_literals;

suite AudioLoop_tests = []() {
    "1_stop"_test = []() {
        auto pool = std::make_shared<AudioBufferPool<int>>(10, 256, 100ms);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);
        expect(loop_test_if->buffers()[0]->head() == 0);
        expect(loop_test_if->get_current_buffer_idx() == 0);
        expect(loop_test_if->get_position_in_current_buffer() == 0);

        loop.process(1000);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);
    };

    "2_record"_test = []() {
        auto pool = std::make_shared<AudioBufferPool<int>>(10, 256, 100ms);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;

        std::vector<int> source_buf(512); // TODO init values        
        loop.set_next_state(Recording);
        loop.set_recording_buffer(source_buf.data(), source_buf.size());
        loop.transition_now();
        loop.update_poi();

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 512) << loop.get_next_poi().value_or(0); // end of buffer
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        loop.process(20);

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 492) << loop.get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));
        expect(eq(loop_test_if->get_current_buffer_idx(), 0));
        expect(eq(loop_test_if->get_position_in_current_buffer(), 0));
        expect(eq(loop_test_if->buffers()[0]->head(), 20));
    };
    
    "2_1_record_beyond_external_buf"_test = []() {
        auto pool = std::make_shared<AudioBufferPool<int>>(10, 256, 100ms);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;
        
        loop.set_recording_buffer(nullptr, 0);
        loop.set_next_state(Recording);
        loop.transition_now();
        loop.update_poi();

        expect(throws([&]() { loop.process(20); }));
    };
};