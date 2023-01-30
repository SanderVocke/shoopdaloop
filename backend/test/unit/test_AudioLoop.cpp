#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioLoop.h"
#include "helpers.h"

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
        auto pool = std::make_shared<AudioBufferPool<int>>(10, 64, 100ms);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;

        auto source_buf = create_buf<int>(512, [](size_t pos) { return pos; }); 
        loop.plan_transition(Recording);
        loop.set_recording_buffer(source_buf.data(), source_buf.size());
        loop.trigger();
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
        for_buf_elems<AudioBuffer<int>, int>(*loop_test_if->buffers()[0],
                      [](size_t pos, int const& val) {
                        expect(eq(val, pos)) << " @ position " << pos;
                      });
        
    };
    
    "2_1_record_beyond_external_buf"_test = []() {
        auto pool = std::make_shared<AudioBufferPool<int>>(10, 256, 100ms);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;
        
        loop.set_recording_buffer(nullptr, 0);
        loop.plan_transition(Recording);
        loop.trigger();
        loop.update_poi();

        expect(throws([&]() { loop.process(20); }));
    };

    "2_2_record_multiple_buffers"_test = []() {
        auto pool = std::make_shared<AudioBufferPool<int>>(10, 64, 100ms);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;

        auto source_buf = create_buf<int>(512, [](size_t pos) { return pos; }); 
        loop.plan_transition(Recording);
        loop.set_recording_buffer(source_buf.data(), source_buf.size());
        loop.trigger();
        loop.update_poi();

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 512) << loop.get_next_poi().value_or(0); // end of buffer
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        loop.process(512);

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 0) << loop.get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 512));
        expect(eq(loop.get_position(), 0));
        expect(eq(loop_test_if->get_current_buffer_idx(), 0));
        expect(eq(loop_test_if->get_position_in_current_buffer(), 0));
        expect(eq(loop_test_if->buffers()[0]->head(), 64));
        for_loop_elems<AudioLoop<int>, int>(loop,
                      [](size_t pos, int const& val) {
                        expect(eq(val, pos)) << " @ position " << pos;
                      });
        expect(eq(loop_test_if->buffers().size(), 9));
    };
};