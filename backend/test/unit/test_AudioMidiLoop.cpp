#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioChannelSubloop.h"
#include "AudioMidiLoop.h"
#include "MidiChannelSubloop.h"
#include "helpers.h"

using namespace boost::ut;
using namespace std::chrono_literals;

template<typename MessageA, typename MessageB>
inline void check_msgs_equal(MessageA const& a, MessageB const& b) {
    expect(eq(a.time, b.time));
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
    "audioloop_1_stop"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 256);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, AudioOutputType::Copy);

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

    "audioloop_2_record"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, AudioOutputType::Copy);
        auto &channel = *loop.audio_channel<int>(0);

        auto source_buf = create_audio_buf<int>(512, [](size_t pos) { return pos; }); 
        loop.plan_transition(Recording);
        channel.set_recording_buffer(source_buf.data(), source_buf.size());
        loop.trigger();
        loop.update_poi();

        expect(eq(loop.get_state(), Recording));
        expect(eq(loop.get_next_poi().value_or(999), 512)) << loop.get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 0));
        expect(eq(loop.get_position(), 0));

        loop.process(20);

        expect(eq(loop.get_state(), Recording));
        expect(eq(loop.get_next_poi().value_or(999), 492)) << loop.get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));
        for_channel_elems<AudioChannelSubloop<int>, int>(
            channel, 
            [](size_t pos, int const& val) {
                expect(eq(val, pos)) << " @ position " << pos;
            },
            0,
            20
        );
    };
    
    "audioloop_2_1_record_beyond_external_buf"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 256);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, AudioOutputType::Copy);
        auto &channel = *loop.audio_channel<int>(0);
        
        channel.set_recording_buffer(nullptr, 0);
        loop.plan_transition(Recording);
        loop.trigger();
        loop.update_poi();

        expect(throws([&]() { loop.process(20); }));
    };

    "audioloop_2_2_record_multiple_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, AudioOutputType::Copy);
        auto &channel = *loop.audio_channel<int>(0);

        auto source_buf = create_audio_buf<int>(512, [](size_t pos) { return pos; }); 
        loop.plan_transition(Recording);
        channel.set_recording_buffer(source_buf.data(), source_buf.size());
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
        for_channel_elems<AudioChannelSubloop<int>, int>(
            channel, 
            [](size_t pos, int const& val) {
                expect(eq(val, pos)) << " @ position " << pos;
            }
        );
    };

    "audioloop_2_3_record_multiple_source_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, AudioOutputType::Copy);
        auto &channel = *loop.audio_channel<int>(0);

        auto source_buf = create_audio_buf<int>(32, [](size_t pos) { return pos; });
        loop.plan_transition(Recording);
        channel.set_recording_buffer(source_buf.data(), source_buf.size());
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
            channel.set_recording_buffer(source_buf.data(), source_buf.size());
            loop.handle_poi();
            loop.update_poi();
        }

        expect(loop.get_state() == Recording);
        expect(eq(loop.get_next_poi().value_or(999), 32)) << loop.get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 512));
        expect(eq(loop.get_position(), 0));
        for_channel_elems<AudioChannelSubloop<int>, int>(
            channel, 
            [](size_t pos, int const& val) {
                expect(eq(val, pos)) << " @ position " << pos;
            }
        );
    };

    "audioloop_3_playback"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, AudioOutputType::Copy);
        auto &channel = *loop.audio_channel<int>(0);
        
        auto data = create_audio_buf<int>(64, [](size_t pos) { return pos; });
        channel.load_data(data.data(), 64, false);
        loop.set_length(64);
        auto play_buf = std::vector<int>(64);

        loop.plan_transition(Playing);
        channel.set_playback_buffer(play_buf.data(), play_buf.size());
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
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_buf[idx], data[idx])) << "@ position " << idx;
        }        
    };

    "audioloop_3_1_playback_multiple_target_buffers"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, AudioOutputType::Copy);
        auto &channel = *loop.audio_channel<int>(0);
        
        auto data = create_audio_buf<int>(512, [](size_t pos) { return pos; });
        channel.load_data(data.data(), 512, false);
        loop.set_length(512);

        loop.plan_transition(Playing);
        loop.trigger();
        loop.update_poi();

        expect(loop.get_state() == Playing);
        expect(loop.get_next_poi() == 0) << loop.get_next_poi().value_or(0); // end of buffer
        expect(loop.get_position() == 0);
        expect(loop.get_length() == 512);

        auto play_buf = std::vector<int>(512);
        for(size_t processed = 0; processed < 512; processed+=64) {
            
            channel.set_playback_buffer(play_buf.data()+processed, 64);
            loop.update_poi();
            loop.process(64);
        }

        for (size_t idx=0; idx<512; idx++) {
            expect(eq(play_buf[idx], idx)) << "@ pos " << idx;
        }
    };
};

suite AudioMidiLoop_midi_tests = []() {
    "midiloop_1_stop"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);

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

    "midiloop_2_record"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back({.time = 10, .size = 3, .data = { 0x01, 0x02, 0x03 }});
        source_buf.read.push_back({.time = 19, .size = 2, .data = { 0x01, 0x02 }});
        source_buf.read.push_back({.time = 20, .size = 1, .data = { 0x01 }});
        
        loop.plan_transition(Recording);
        channel.set_recording_buffer(&source_buf, 512);
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

        size_t length = loop.get_length();
        auto msgs = channel.retrieve_contents(false);
        expect(eq(length, 20));
        expect(eq(msgs.size(), 2));
        check_msgs_equal(msgs.at(0), source_buf.read.at(0));
        check_msgs_equal(msgs.at(1), source_buf.read.at(1));
    };

    "midiloop_2_1_record_append_out_of_order"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);
        using Message = MidiChannelSubloop<uint32_t, uint16_t>::Message;
        std::vector<Message> contents = {
            Message{.time = 111,  .size = 4, .data = { 0x01, 0x02, 0x03, 0x04 }}
        };
        channel.set_contents(contents, false);
        loop.set_length(100);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back({.time = 10, .size = 3, .data =  { 0x01, 0x02, 0x03 }});
        source_buf.read.push_back({.time = 9,  .size = 2, .data = { 0x01, 0x02 }});
        source_buf.read.push_back({.time = 11, .size = 1, .data =  { 0x01 }});
        
        loop.plan_transition(Recording);
        channel.set_recording_buffer(&source_buf, 512);
        loop.trigger();
        loop.update_poi();

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 512) << loop.get_next_poi().value_or(0); // end of buffer
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        loop.process(20);

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == 492) << loop.get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 120));
        expect(eq(loop.get_position(), 0));

        size_t length = loop.get_length();
        auto msgs = channel.retrieve_contents(false);
        expect(eq(msgs.size(), 2));
        expect(eq(length, 120));
        check_msgs_equal(msgs.at(0), contents.at(0));
        check_msgs_equal(msgs.at(1), with_time(source_buf.read.at(2), 111));
    };

    "midiloop_3_playback"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);
        using Message = MidiChannelSubloop<uint32_t, uint16_t>::Message;
        std::vector<Message> contents = {
            Message{.time = 0,  .size = 1, .data = { 0x01 }},
            Message{.time = 10,  .size = 1, .data = { 0x02 }},
            Message{.time = 21,  .size = 1, .data = { 0x03 }},
        };
        channel.set_contents(contents, false);
        loop.set_length(100);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        auto play_buf = MidiTestBuffer();
        
        loop.plan_transition(Playing);
        channel.set_playback_buffer(&play_buf, 512);
        loop.trigger();
        loop.update_poi();

        expect(loop.get_state() == Playing);
        expect(loop.get_next_poi() == 100) << loop.get_next_poi().value_or(0); // end of loop
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        loop.process(20);

        expect(loop.get_state() == Playing);
        expect(loop.get_next_poi() == 80) << loop.get_next_poi().value_or(0); // end of loop
        expect(eq(loop.get_length(), 100));
        expect(eq(loop.get_position(), 20));

        auto msgs = play_buf.written;
        expect(eq(msgs.size(), 2));
        check_msgs_equal(msgs.at(0), contents.at(0));
        check_msgs_equal(msgs.at(1), contents.at(1));
    };

    "midiloop_4_playback_muted"_test = []() {
        AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(512);
        auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);
        using Message = MidiChannelSubloop<uint32_t, uint16_t>::Message;
        std::vector<Message> contents = {
            Message{.time = 0,  .size = 1, .data = { 0x01 }},
            Message{.time = 10,  .size = 1, .data = { 0x02 }},
            Message{.time = 21,  .size = 1, .data = { 0x03 }},
        };
        channel.set_contents(contents, false);
        loop.set_length(100);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        auto play_buf = MidiTestBuffer();
        
        loop.plan_transition(PlayingMuted);
        channel.set_playback_buffer(&play_buf, 512);
        loop.trigger();
        loop.update_poi();

        expect(loop.get_state() == PlayingMuted);
        expect(loop.get_next_poi() == 100) << loop.get_next_poi().value_or(0); // end of loop
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        loop.process(20);

        expect(loop.get_state() == PlayingMuted);
        expect(loop.get_next_poi() == 80) << loop.get_next_poi().value_or(0); // end of loop
        expect(eq(loop.get_length(), 100));
        expect(eq(loop.get_position(), 20));

        auto msgs = play_buf.written;
        expect(eq(msgs.size(), 0));
    };
};