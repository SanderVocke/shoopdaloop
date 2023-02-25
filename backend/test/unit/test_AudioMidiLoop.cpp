#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioChannel.h"
#include "AudioMidiLoop.h"
#include "MidiChannel.h"
#include "helpers.h"
#include "types.h"

using namespace boost::ut;
using namespace std::chrono_literals;

template<typename MessageA, typename MessageB>
inline void check_msgs_equal(MessageA const& a, MessageB const& b, int time_offset=0) {
    expect(eq(a.time, b.time+time_offset));
    expect(eq(a.size, b.size));
    expect(eq(a.data, b.data));
}

template<typename Message>
inline Message with_time (Message const& msg, size_t time) {
    Message m = msg;
    m.time = time;
    return m;
}

suite AudioMidiLoop_audio_tests = []() {
    "al_1_stop"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 256);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(1000);

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));
    };

    "al_2_record"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);

        auto source_buf = create_audio_buf<int>(512, [](size_t position) { return position; }); 
        loop.plan_transition(Recording);
        channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 512)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 0));
        expect(eq(loop.get_position(), 0));

        loop.PROC_process(20);

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 492)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [](size_t position, int const& val) {
                expect(eq(val, position)) << " @ position " << position;
            },
            0,
            20
        );
    };
    
    "al_2_1_record_beyond_external_buf"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 256);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        
        channel.PROC_set_recording_buffer(nullptr, 0);
        loop.plan_transition(Recording);
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(throws([&]() { loop.PROC_process(20); }));
    };

    "al_2_2_record_multiple_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);

        auto source_buf = create_audio_buf<int>(512, [](size_t position) { return position; }); 
        loop.plan_transition(Recording);
        channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 512) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(512);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 0) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 512));
        expect(eq(loop.get_position(), 0));
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [](size_t position, int const& val) {
                expect(eq(val, position)) << " @ position " << position;
            }
        );
    };

    "al_2_3_record_multiple_source_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);

        auto source_buf = create_audio_buf<int>(32, [](size_t position) { return position; });
        loop.plan_transition(Recording);
        channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 32) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        for(size_t samples_processed = 0; samples_processed < 512; ) {
            loop.PROC_process(32);
            samples_processed += 32;
            source_buf = create_audio_buf<int>(32, [&](size_t position) { return position + samples_processed; });
            channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
            loop.PROC_handle_poi();
            loop.PROC_update_poi();
        }

        expect(eq(loop.get_mode() , Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 32)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 512));
        expect(eq(loop.get_position(), 0));
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [](size_t position, int const& val) {
                expect(eq(val, position)) << " @ position " << position;
            }
        );
    };

    "al_2_4_record_onto_smaller_buffer"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        auto data = create_audio_buf<int>(64, [](size_t position) { return -((int)position); });
        channel.load_data(data.data(), 64, false);
        loop.plan_transition(Recording);
        loop.PROC_trigger();
        loop.set_length(128);

        auto source_buf = create_audio_buf<int>(512, [](size_t position) { return position; }); 
        channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
        loop.PROC_update_poi();

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 512)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 128));
        expect(eq(loop.get_position(), 0));

        loop.PROC_process(20);

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 492)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 148));
        expect(eq(loop.get_position(), 0));
        // First 64 elements should be unchanged after record
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [](size_t position, int const& val) {
                expect(eq(val, -((int)position))) << " @ position " << position;
            },
            0,
            64
        );
        // The missing "data gap" should be filled with zeroes
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [](size_t position, int const& val) {
                expect(eq(val, 0)) << " @ position " << position;
            },
            64,
            64
        );
        // The new recording should be appended at the end.
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [](size_t position, int const& val) {
                expect(eq(val, position-128)) << " @ position " << position;
            },
            128,
            20
        );
    };

    "al_2_5_record_onto_larger_buffer"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        auto data = create_audio_buf<int>(128, [](size_t position) { return -((int)position); });
        channel.load_data(data.data(), 128, false);
        loop.plan_transition(Recording);
        loop.PROC_trigger();
        loop.set_length(64);

        auto source_buf = create_audio_buf<int>(512, [](size_t position) { return position; }); 
        channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
        loop.PROC_update_poi();

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 512)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 0));

        loop.PROC_process(64);

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 448)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 128));
        expect(eq(loop.get_position(), 0));
        // First 64 elements should be unchanged after record
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [](size_t position, int const& val) {
                expect(eq(val, -((int)position))) << " @ position " << position;
            },
            0,
            64
        );
        // The next samples should have been overwritten by the recording.
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [](size_t position, int const& val) {
                expect(eq(val, position-64)) << " @ position " << position;
            },
            64
        );
    };

    "al_3_playback"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        
        auto data = create_audio_buf<int>(64, [](size_t position) { return position; });
        channel.load_data(data.data(), 64, false);
        loop.set_length(64);
        auto play_buf = std::vector<int>(64);

        loop.plan_transition(Playing);
        channel.PROC_set_playback_buffer(play_buf.data(), play_buf.size());
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 64) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_position() , 0));
        expect(eq(loop.get_length() , 64));

        loop.PROC_process(20);

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 44) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 20));
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_buf[idx], data[idx])) << "@ position " << idx;
        }        
    };

    "al_3_1_playback_multiple_target_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        
        auto data = create_audio_buf<int>(512, [](size_t position) { return position; });
        channel.load_data(data.data(), 512, false);

        loop.set_length(512);
        loop.plan_transition(Playing);
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(eq(loop.get_position() , 0));
        expect(eq(loop.get_length() , 512));

        auto play_buf = std::vector<int>(512);
        for(size_t processed = 0; processed < 512; processed+=64) {
            
            channel.PROC_set_playback_buffer(play_buf.data()+processed, 64);
            expect(loop.PROC_get_next_poi() == 64) << loop.PROC_get_next_poi().value_or(0); // end of buffer
            loop.PROC_update_poi();
            loop.PROC_process(64);
        }

        for (size_t idx=0; idx<512; idx++) {
            expect(eq(play_buf[idx], idx)) << "@ position " << idx;
        }
    };

    "al_3_2_playback_shorter_data"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        
        auto data = create_audio_buf<int>(32, [](size_t position) { return position; });
        channel.load_data(data.data(), 32, false);
        loop.set_length(64);
        auto play_buf = std::vector<int>(64);

        loop.plan_transition(Playing);
        channel.PROC_set_playback_buffer(play_buf.data(), play_buf.size());
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 64) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_position() , 0));
        expect(eq(loop.get_length() , 64));

        loop.PROC_process(62);

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 2) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 62));
        for(size_t idx=0; idx<32; idx++) {
            expect(eq(play_buf[idx], data[idx])) << "@ position " << idx;
        }
        for(size_t idx=32; idx<62; idx++) {
            expect(eq(play_buf[idx], 0)) << "@ position" << idx;
        }
    };

    "al_3_3_playback_wrap"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        
        auto data = create_audio_buf<int>(64, [](size_t position) { return position; });
        channel.load_data(data.data(), 64, false);
        loop.set_length(64);
        loop.set_mode(Playing, false);
        loop.set_position(48);
        auto play_buf = std::vector<int>(64);

        expect(eq(loop.get_mode(),Playing));
        channel.PROC_set_playback_buffer(play_buf.data(), play_buf.size());
        loop.PROC_update_poi();

        expect(eq(loop.get_mode(),Playing));
        expect(loop.PROC_get_next_poi() == (64-48)) << loop.PROC_get_next_poi().value_or(0); // end of loop
        expect(eq(loop.get_position(), 48));
        expect(eq(loop.get_length(), 64));

        loop.PROC_process(16);
        loop.PROC_handle_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 48) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 0));
        
        loop.PROC_process(48);
        loop.PROC_handle_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 0) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 48));
        
        for(size_t idx=0; idx<(64-48); idx++) {
            expect(eq(play_buf[idx], data[idx+48])) << "@ position " << idx;
        }
        for(size_t idx=0; idx<48; idx++) {
            expect(eq(play_buf[idx+(64-48)], data[idx])) << "@ position " << idx;
        }
    };

    "al_3_4_playback_wrap_longer_data"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        
        auto data = create_audio_buf<int>(128, [](size_t position) { return position; });
        channel.load_data(data.data(), 128, false);
        loop.set_length(64);
        loop.set_mode(Playing, false);
        loop.set_position(48);
        auto play_buf = std::vector<int>(64);

        channel.PROC_set_playback_buffer(play_buf.data(), play_buf.size());
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == (64-48)) << loop.PROC_get_next_poi().value_or(0); // end of loop
        expect(eq(loop.get_position() , 48));
        expect(eq(loop.get_length() , 64));

        loop.PROC_process(16);
        loop.PROC_handle_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 48) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 0));
        
        loop.PROC_process(48);
        loop.PROC_handle_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 0) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 48));
        
        for(size_t idx=0; idx<(64-48); idx++) {
            expect(eq(play_buf[idx], data[idx+48])) << "@ position " << idx;
        }
        for(size_t idx=0; idx<48; idx++) {
            expect(eq(play_buf[idx+(64-48)], data[idx])) << "@ position " << idx;
        }
    };
};

suite AudioMidiLoop_midi_tests = []() {
    "ml_1_stop"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512, Direct, false);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(1000);

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));
    };

    "ml_2_record"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512, Direct, false);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back({.time = 10, .size = 3, .data = { 0x01, 0x02, 0x03 }});
        source_buf.read.push_back({.time = 19, .size = 2, .data = { 0x01, 0x02 }});
        source_buf.read.push_back({.time = 20, .size = 1, .data = { 0x01 }});
        
        loop.plan_transition(Recording);
        channel.PROC_set_recording_buffer(&source_buf, 512);
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 512) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(20);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 492) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));

        size_t length = loop.get_length();
        auto msgs = channel.retrieve_contents(false);
        expect(eq(length, 20));
        expect(eq(msgs.size(), 2));
        check_msgs_equal(msgs.at(0), source_buf.read.at(0));
        check_msgs_equal(msgs.at(1), source_buf.read.at(1));
    };

    "ml_2_1_record_append_out_of_order"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512, Direct, false);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);
        using Message = MidiChannel<uint32_t, uint16_t>::Message;

        loop.set_mode(Recording, false);
        loop.set_length(100);
        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back({.time = 10, .size = 3, .data =  { 0x01, 0x02, 0x03 }});
        source_buf.read.push_back({.time = 9,  .size = 2, .data = { 0x01, 0x02 }});
        source_buf.read.push_back({.time = 11, .size = 1, .data =  { 0x01 }});
        channel.PROC_set_recording_buffer(&source_buf, 512);
        loop.PROC_update_poi();
        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 512);
        expect(eq(loop.get_length() , 100));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(20);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 492) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 120));
        expect(eq(loop.get_position(), 0));

        size_t length = loop.get_length();
        auto msgs = channel.retrieve_contents(false);
        expect(eq(msgs.size(), 2));
        expect(eq(length, 120));
        check_msgs_equal(msgs.at(0), with_time(source_buf.read.at(0), 110));
        check_msgs_equal(msgs.at(1), with_time(source_buf.read.at(2), 111));
    };

    "ml_2_2_record_multiple_source_buffers"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512, Direct, false);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        std::vector<MidiTestBuffer> source_bufs = { MidiTestBuffer(), MidiTestBuffer(), MidiTestBuffer() };
        source_bufs[0].read.push_back({.time = 10, .size = 3, .data = { 0x01, 0x02, 0x03 }});
        source_bufs[0].read.push_back({.time = 19, .size = 2, .data = { 0x01, 0x02 }});
        source_bufs[0].read.push_back({.time = 20, .size = 1, .data = { 0x01 }});
        source_bufs[1].read.push_back({.time = 21-21, .size = 3, .data = { 0x01, 0x02, 0x03 }});
        source_bufs[1].read.push_back({.time = 26-21, .size = 2, .data = { 0x01, 0x02 }});
        source_bufs[1].read.push_back({.time = 29-21, .size = 1, .data = { 0x01 }});
        source_bufs[2].read.push_back({.time = 30-21-9, .size = 3, .data = { 0x01, 0x02, 0x03 }});
        source_bufs[2].read.push_back({.time = 30-21-9, .size = 2, .data = { 0x01, 0x02 }});
        source_bufs[2].read.push_back({.time = 31-21-9, .size = 1, .data = { 0x01 }});
        source_bufs[2].read.push_back({.time = 40-21-9, .size = 1, .data = { 0x01 }});
        
        loop.plan_transition(Recording);
        channel.PROC_set_recording_buffer(&source_bufs[0], 21);
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 21) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(21);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 0) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 21));
        expect(eq(loop.get_position(), 0));

        channel.PROC_set_recording_buffer(&source_bufs[1], 9);
        
        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 9) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 21));
        expect(eq(loop.get_position(), 0));

        loop.PROC_process(9);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 0) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 30));
        expect(eq(loop.get_position(), 0));

        channel.PROC_set_recording_buffer(&source_bufs[2], 100);
        
        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 100) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 30));
        expect(eq(loop.get_position(), 0));

        // Purposefully do not process the last message
        loop.PROC_process(5);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 95) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 35));
        expect(eq(loop.get_position(), 0));

        auto msgs = channel.retrieve_contents(false);
        expect(eq(msgs.size(), 9));
        check_msgs_equal(msgs.at(0), source_bufs[0].read.at(0));
        check_msgs_equal(msgs.at(1), source_bufs[0].read.at(1));
        check_msgs_equal(msgs.at(2), source_bufs[0].read.at(2));
        check_msgs_equal(msgs.at(3), source_bufs[1].read.at(0), 21);
        check_msgs_equal(msgs.at(4), source_bufs[1].read.at(1), 21);
        check_msgs_equal(msgs.at(5), source_bufs[1].read.at(2), 21);
        check_msgs_equal(msgs.at(6), source_bufs[2].read.at(0), 30);
        check_msgs_equal(msgs.at(7), source_bufs[2].read.at(1), 30);
        check_msgs_equal(msgs.at(8), source_bufs[2].read.at(2), 30);
    };

    "ml_2_3_record_onto_longer_buffer"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512, Direct, false);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);
        using Message = MidiChannel<uint32_t, uint16_t>::Message;
        std::vector<Message> contents = {
            Message{.time = 0,  .size = 1, .data = { 0x01 }},
            Message{.time = 10,  .size = 1, .data = { 0x02 }},
            Message{.time = 21,  .size = 1, .data = { 0x03 }},
            Message{.time = 30,  .size = 1, .data = { 0x02 }},
            Message{.time = 50,  .size = 1, .data = { 0x03 }},
        };
        channel.set_contents(contents);
        loop.set_mode(Recording, false);
        loop.set_length(25, false);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 0) << loop.PROC_get_next_poi().value_or(0);
        expect(eq(loop.get_length() , 25));
        expect(eq(loop.get_position() , 0));

        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back({.time = 1, .size = 3, .data = { 0x01, 0x02, 0x03 }});
        source_buf.read.push_back({.time = 2, .size = 2, .data = { 0x01, 0x02 }});
        source_buf.read.push_back({.time = 3, .size = 1, .data = { 0x01 }});
        channel.PROC_set_recording_buffer(&source_buf, 512);
        
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 512) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length() , 25));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(20);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 492) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 45));
        expect(eq(loop.get_position(), 0));
        auto msgs = channel.retrieve_contents(false);
        expect(eq(msgs.size(), 6));
        for (size_t i=0; i<3; i++) { check_msgs_equal(msgs.at(i), contents.at(i)); }
        for (size_t i=3; i<6; i++) { check_msgs_equal(msgs.at(i), source_buf.read.at(i-3), 25); }
    };

    "ml_3_playback"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512, Direct, false);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);
        using Message = MidiChannel<uint32_t, uint16_t>::Message;
        std::vector<Message> contents = {
            Message{.time = 0,  .size = 1, .data = { 0x01 }},
            Message{.time = 10,  .size = 1, .data = { 0x02 }},
            Message{.time = 21,  .size = 1, .data = { 0x03 }},
        };
        channel.set_contents(contents, false);
        loop.set_length(100);

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 100));
        expect(eq(loop.get_position() , 0));

        auto play_buf = MidiTestBuffer();
        
        loop.plan_transition(Playing);
        channel.PROC_set_playback_buffer(&play_buf, 512);
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 100) << loop.PROC_get_next_poi().value_or(0); // end of loop
        expect(eq(loop.get_length() , 100));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(20);

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 80) << loop.PROC_get_next_poi().value_or(0); // end of loop
        expect(eq(loop.get_length(), 100));
        expect(eq(loop.get_position(), 20));

        auto msgs = play_buf.written;
        expect(eq(msgs.size(), 2));
        check_msgs_equal(msgs.at(0), contents.at(0));
        check_msgs_equal(msgs.at(1), contents.at(1));
    };
};