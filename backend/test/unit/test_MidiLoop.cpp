#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "MidiLoop.h"
#include "helpers.h"

using namespace boost::ut;

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

suite MidiLoop_tests = []() {
    "midiloop_1_stop"_test = []() {
        MidiLoop<uint32_t, uint16_t> loop(512);

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
        MidiLoop<uint32_t, uint16_t> loop(512);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back({.time = 10, .size = 3, .data = { 0x01, 0x02, 0x03 }});
        source_buf.read.push_back({.time = 19, .size = 2, .data = { 0x01, 0x02 }});
        source_buf.read.push_back({.time = 20, .size = 1, .data = { 0x01 }});
        
        loop.plan_transition(Recording);
        loop.set_recording_buffer(&source_buf, 512);
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

        size_t length;
        auto msgs = loop.retrieve_contents(length, false);
        expect(eq(length, 20));
        expect(eq(msgs.size(), 2));
        check_msgs_equal(msgs.at(0), source_buf.read.at(0));
        check_msgs_equal(msgs.at(1), source_buf.read.at(1));
    };

    "midiloop_2_1_record_append_out_of_order"_test = []() {
        using Loop = MidiLoop<uint32_t, uint16_t>;
        using Message = Loop::Message;
        Loop loop(512);
        std::vector<Message> contents = {
            Message{.time = 111,  .size = 4, .data = { 0x01, 0x02, 0x03, 0x04 }}
        };
        loop.set_contents(contents, 100, false);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back({.time = 10, .size = 3, .data =  { 0x01, 0x02, 0x03 }});
        source_buf.read.push_back({.time = 9,  .size = 2, .data = { 0x01, 0x02 }});
        source_buf.read.push_back({.time = 11, .size = 1, .data =  { 0x01 }});
        
        loop.plan_transition(Recording);
        loop.set_recording_buffer(&source_buf, 512);
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

        size_t length;
        auto msgs = loop.retrieve_contents(length, false);
        expect(eq(msgs.size(), 2));
        expect(eq(length, 120));
        check_msgs_equal(msgs.at(0), contents.at(0));
        check_msgs_equal(msgs.at(1), with_time(source_buf.read.at(2), 111));
    };

    "midiloop_3_playback"_test = []() {
        using Loop = MidiLoop<uint32_t, uint16_t>;
        using Message = Loop::Message;
        Loop loop(512);
        std::vector<Message> contents = {
            Message{.time = 0,  .size = 1, .data = { 0x01 }},
            Message{.time = 10,  .size = 1, .data = { 0x02 }},
            Message{.time = 21,  .size = 1, .data = { 0x03 }},
        };
        loop.set_contents(contents, 100, false);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        auto play_buf = MidiTestBuffer();
        
        loop.plan_transition(Playing);
        loop.set_playback_buffer(&play_buf, 512);
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
        using Loop = MidiLoop<uint32_t, uint16_t>;
        using Message = Loop::Message;
        Loop loop(512);
        std::vector<Message> contents = {
            Message{.time = 0,  .size = 1, .data = { 0x01 }},
            Message{.time = 10,  .size = 1, .data = { 0x02 }},
            Message{.time = 21,  .size = 1, .data = { 0x03 }},
        };
        loop.set_contents(contents, 100, false);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 100);
        expect(loop.get_position() == 0);

        auto play_buf = MidiTestBuffer();
        
        loop.plan_transition(PlayingMuted);
        loop.set_playback_buffer(&play_buf, 512);
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