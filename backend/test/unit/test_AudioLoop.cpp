#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioLoop.h"
#include "helpers.h"

using namespace boost::ut;
using namespace std::chrono_literals;

suite AudioLoop_tests = []() {
    "audioloop_1_stop"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 256);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);
        expect(loop_test_if->get_current_buffer_idx() == 0);
        expect(loop_test_if->get_position_in_current_buffer() == 0);

        loop.process(1000);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);
    };

    "audioloop_2_record"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;

        auto source_buf = create_audio_buf<int>(512, [](size_t pos) { return pos; }); 
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
        for_buf_elems<AudioBuffer<int>, int>(*loop_test_if->buffers()[0],
                      [](size_t pos, int const& val) {
                        expect(eq(val, pos)) << " @ position " << pos;
                      }, 0, 20);
        
    };
    
    "audioloop_2_1_record_beyond_external_buf"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 256);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;
        
        loop.set_recording_buffer(nullptr, 0);
        loop.plan_transition(Recording);
        loop.trigger();
        loop.update_poi();

        expect(throws([&]() { loop.process(20); }));
    };

    "audioloop_2_2_record_multiple_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;

        auto source_buf = create_audio_buf<int>(512, [](size_t pos) { return pos; }); 
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
        expect(ge(loop_test_if->buffers().size(), (512/64)));
        expect(eq(loop_test_if->get_current_buffer_idx(), 0));
        expect(eq(loop_test_if->get_position_in_current_buffer(), 0));
        for_loop_elems<AudioLoop<int>, int>(loop,
                      [](size_t pos, int const& val) {
                        expect(eq(val, pos)) << " @ position " << pos;
                      });
    };

    "audioloop_2_3_record_multiple_source_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;

        auto source_buf = create_audio_buf<int>(32, [](size_t pos) { return pos; });
        loop.plan_transition(Recording);
        loop.set_recording_buffer(source_buf.data(), source_buf.size());
        loop.trigger();
        loop.update_poi();

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 32) << loop.get_next_poi().value_or(0); // end of buffer
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        for(size_t samples_processed = 0; samples_processed < 512; ) {
            loop.process(32);
            samples_processed += 32;
            source_buf = create_audio_buf<int>(32, [&](size_t pos) { return pos + samples_processed; });
            loop.set_recording_buffer(source_buf.data(), source_buf.size());
            loop.handle_poi();
            loop.update_poi();
        }

        expect(loop.get_state() == Recording);
        expect(eq(loop.get_next_poi().value_or(999), 32)) << loop.get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 512));
        expect(eq(loop.get_position(), 0));
        expect(ge(loop_test_if->buffers().size(), (512/64)));
        expect(eq(loop_test_if->get_current_buffer_idx(), 0));
        expect(eq(loop_test_if->get_position_in_current_buffer(), 0));
        for_loop_elems<AudioLoop<int>, int>(loop,
                      [](size_t pos, int const& val) {
                        expect(eq(val, pos)) << " @ position " << pos;
                      });
    };

    "audioloop_3_playback"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;
        
        auto data = create_audio_buf<int>(64, [](size_t pos) { return pos; });
        loop.load_data(data.data(), 64, false);
        auto play_buf = std::vector<int>(64);

        loop.plan_transition(Playing);
        loop.set_playback_buffer(play_buf.data(), play_buf.size());
        loop.trigger();
        loop.update_poi();

        expect(loop.get_state() == Playing);
        expect(loop.get_next_poi() == 64) << loop.get_next_poi().value_or(0); // end of buffer
        expect(loop.get_position() == 0);
        expect(loop.get_length() == 64);

        loop.process(20);

        expect(loop.get_state() == Playing);
        expect(loop.get_next_poi() == 44) << loop.get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 20));
        expect(eq(loop_test_if->get_current_buffer_idx(), 0));
        expect(eq(loop_test_if->get_position_in_current_buffer(), 20));
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_buf[idx], data[idx])) << "@ position " << idx;
        }        
    };

    "audioloop_3_1_playback_multiple_target_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioLoop<int>loop(pool, 10, AudioLoopOutputType::Copy);
        AudioLoopTestInterface<AudioLoop<int>::Buffer> *loop_test_if =
            (AudioLoopTestInterface<AudioLoop<int>::Buffer> *)&loop;
        
        auto data = create_audio_buf<int>(512, [](size_t pos) { return pos; });
        loop.load_data(data.data(), 512, false);

        loop.plan_transition(Playing);
        loop.trigger();
        loop.update_poi();

        expect(loop.get_state() == Playing);
        expect(loop.get_next_poi() == 0) << loop.get_next_poi().value_or(0); // end of buffer
        expect(loop.get_position() == 0);
        expect(loop.get_length() == 512);

        auto play_buf = std::vector<int>(512);
        for(size_t processed = 0; processed < 512; processed+=64) {
            
            loop.set_playback_buffer(play_buf.data()+processed, 64);
            loop.update_poi();
            loop.process(64);
        }

        for (size_t idx=0; idx<512; idx++) {
            expect(eq(play_buf[idx], idx)) << "@ pos " << idx;
        }
    };
};