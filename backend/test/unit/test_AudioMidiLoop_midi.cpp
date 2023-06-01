#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioMidiLoop.h"
#include "MidiChannel.h"
#include "MidiStateTracker.h"
#include "helpers.h"
#include "types.h"
#include "process_loops.h"
#include <ranges>
#include <string>
#include <valarray>
#include "midi_helpers.h"

using namespace boost::ut;
using namespace std::chrono_literals;

template<typename MessageA, typename MessageB>
inline void check_msgs_equal(
    MessageA const& a,
    MessageB const& b,
    int time_offset=0,
    std::string info="",
    const boost::ut::reflection::source_location sl = boost::ut::reflection::source_location::current()
    )
{
    expect(eq(a.time, b.time+time_offset), sl) << " (time) " << info;
    expect(eq(a.size, b.size), sl) << " (size) " << info;
    for (size_t i=0; i<a.size && i<b.size; i++) {
        expect(eq((int)a.data[i], (int)b.data[i]), sl) << " (data [" << i << "]) " << info;
    }
}

template<typename MessageA, typename MessageB>
inline void check_msg_vectors_equal(
    std::vector<MessageA> const& a,
    std::vector<MessageB> const& b,
    std::string info = "",
    const boost::ut::reflection::source_location sl = boost::ut::reflection::source_location::current())
{
    expect(eq(a.size(), b.size()), sl) << " (size) " << info;
    for (size_t i=0; i<a.size() && i < b.size(); i++) {
        check_msgs_equal(a[i], b[i], 0, "(msg " + std::to_string(i) + ") " + info, sl);
    }
}

template<typename Message>
inline Message with_time (Message const& msg, size_t time) {
    Message m = msg;
    m.time = time;
    return m;
}

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
        source_buf.read.push_back(Msg(10, 3, { 0x01, 0x02, 0x03 }));
        source_buf.read.push_back(Msg(19, 2, { 0x01, 0x02 }));
        source_buf.read.push_back(Msg(20, 1, { 0x01 }));
        
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
        source_buf.read.push_back(Msg(10, 3,  { 0x01, 0x02, 0x03 }));
        source_buf.read.push_back(Msg(9,  2,  { 0x01, 0x02 }));
        source_buf.read.push_back(Msg(11, 1,  { 0x01 }));
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
        source_bufs[0].read.push_back(Msg(10, 3, { 0x01, 0x02, 0x03 }));
        source_bufs[0].read.push_back(Msg(19, 2, { 0x01, 0x02 }));
        source_bufs[0].read.push_back(Msg(20, 1, { 0x01 }));
        source_bufs[1].read.push_back(Msg(21-21, 3, { 0x01, 0x02, 0x03 }));
        source_bufs[1].read.push_back(Msg(26-21, 2, { 0x01, 0x02 }));
        source_bufs[1].read.push_back(Msg(29-21, 1, { 0x01 }));
        source_bufs[2].read.push_back(Msg(30-21-9, 3, { 0x01, 0x02, 0x03 }));
        source_bufs[2].read.push_back(Msg(30-21-9, 2, { 0x01, 0x02 }));
        source_bufs[2].read.push_back(Msg(31-21-9, 1, { 0x01 }));
        source_bufs[2].read.push_back(Msg(40-21-9, 1, { 0x01 }));
        
        loop.plan_transition(Recording);
        channel.PROC_set_recording_buffer(&source_bufs[0], 21);
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 21)); // end of buffer
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(21);

        expect(eq(loop.get_mode() , Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 0)); // end of buffer
        expect(eq(loop.get_length(), 21));
        expect(eq(loop.get_position(), 0));

        channel.PROC_set_recording_buffer(&source_bufs[1], 9);
        loop.PROC_update_poi();
        
        expect(eq(loop.get_mode() , Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 9)); // end of buffer
        expect(eq(loop.get_length(), 21));
        expect(eq(loop.get_position(), 0));

        loop.PROC_process(9);

        expect(eq(loop.get_mode() , Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 0)); // end of buffer
        expect(eq(loop.get_length(), 30));
        expect(eq(loop.get_position(), 0));

        channel.PROC_set_recording_buffer(&source_bufs[2], 100);
        loop.PROC_update_poi();
        
        expect(eq(loop.get_mode() , Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 100)); // end of buffer
        expect(eq(loop.get_length(), 30));
        expect(eq(loop.get_position(), 0));

        // Purposefully do not process the last message
        loop.PROC_process(5);

        expect(eq(loop.get_mode() , Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 95)); // end of buffer
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
            Message(0,  1, { 0x01 }),
            Message(10, 1, { 0x02 }),
            Message(21, 1, { 0x03 }),
            Message(30, 1, { 0x02 }),
            Message(50, 1, { 0x03 }),
        };
        channel.set_contents(contents, 100, false);
        loop.set_mode(Recording, false);
        loop.set_length(25, false);

        expect(eq(loop.get_mode() , Recording));
        expect(loop.PROC_get_next_poi() == 0) << loop.PROC_get_next_poi().value_or(0);
        expect(eq(loop.get_length() , 25));
        expect(eq(loop.get_position() , 0));

        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back(Msg(1, 3, { 0x01, 0x02, 0x03 }));
        source_buf.read.push_back(Msg(2, 2, { 0x01, 0x02 }));
        source_buf.read.push_back(Msg(3, 1, { 0x01 }));
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
            Message(0,  1, { 0x01 }),
            Message(10, 1, { 0x02 }),
            Message(21, 1, { 0x03 }),
        };
        channel.set_contents(contents, 100, false);
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

    "ml_4_prerecord"_test = []() {
        AudioMidiLoop loop;
        auto sync_source = std::make_shared<AudioMidiLoop>();
        sync_source->set_length(100);
        sync_source->plan_transition(Playing);
        expect(eq(sync_source->PROC_predicted_next_trigger_eta().value_or(999), 100));

        loop.set_sync_source(sync_source); // Needed because otherwise will immediately transition
        loop.PROC_update_poi();
        loop.PROC_update_trigger_eta();
        expect(eq(loop.PROC_predicted_next_trigger_eta().value_or(999), 100));

        loop.add_midi_channel<uint32_t, uint16_t>(512, Direct, false);
        auto &chan = *loop.midi_channel<uint32_t, uint16_t>(0);

        auto source_buf = MidiTestBuffer();
        source_buf.read.push_back(Msg(1 , 3, { 0x01, 0x02, 0x03 }));
        source_buf.read.push_back(Msg(10, 2, { 0x01, 0x02 }));
        source_buf.read.push_back(Msg(21, 1,  { 0x01 }));
        source_buf.read.push_back(Msg(39, 1,  { 0x02 }));

        loop.plan_transition(Recording); // Not triggered yet
        chan.PROC_set_recording_buffer(&source_buf, 512);
        loop.PROC_update_poi();

        expect(eq(loop.get_mode(), Stopped));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 512)); // end of buffer
        expect(eq(loop.get_length(), 0));
        expect(eq(loop.get_position(), 0));

        loop.PROC_process(20);
        chan.PROC_finalize_process();

        // By now, we are still stopped but the channels should have pre-recorded since recording is planned.
        expect(eq(loop.get_mode(), Stopped));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 492)); // end of buffer
        expect(eq(loop.get_length(), 0));
        expect(eq(loop.get_position(), 0));

        loop.PROC_trigger();
        loop.PROC_update_poi();
        loop.PROC_process(20);
        chan.PROC_finalize_process();

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 472)); // end of buffer
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));
        expect(eq(loop.PROC_predicted_next_trigger_eta().value_or(999), 80));

        // Due to prerecord, should have captured all the messages.
        auto msgs = chan.retrieve_contents(false);
        expect(eq(msgs.size(), 4));
        expect(eq(chan.get_start_offset(), 20));
        check_msgs_equal(msgs.at(0), source_buf.read.at(0));
        check_msgs_equal(msgs.at(1), source_buf.read.at(1));
        check_msgs_equal(msgs.at(2), source_buf.read.at(2));
        check_msgs_equal(msgs.at(3), source_buf.read.at(3));

        sync_source->PROC_process(60);
        loop.PROC_update_poi();
        loop.PROC_update_trigger_eta();
        expect(eq(loop.PROC_predicted_next_trigger_eta().value_or(999), 40));
    };

    "ml_5_preplay"_test = []() {
        auto loop_ptr = std::make_shared<AudioMidiLoop>();
        auto &loop = *loop_ptr;
        auto sync_source = std::make_shared<AudioMidiLoop>();

        auto process = [&](size_t n_samples) {
            process_loops<AudioMidiLoop>({loop_ptr, sync_source}, n_samples);
        };

        sync_source->set_length(100);
        sync_source->plan_transition(Playing);
        expect(eq(sync_source->PROC_predicted_next_trigger_eta().value_or(999), 100));

        loop.set_sync_source(sync_source); // Needed because otherwise will immediately transition
        loop.PROC_update_poi();
        loop.PROC_update_trigger_eta();
        expect(eq(loop.PROC_predicted_next_trigger_eta().value_or(999), 100));

        loop.add_midi_channel<uint32_t, uint16_t>(100000, Direct, false);
        auto &chan = *loop.midi_channel<uint32_t, uint16_t>(0);

        using Message = MidiChannel<uint32_t, uint16_t>::Message;
        std::vector<Message> data;
        for(size_t idx=0; idx<256; idx++) { data.push_back(Message{(unsigned)idx, 1, { (unsigned char)'a' + (unsigned char) idx }}); }

        chan.set_contents(data, 256);
        chan.set_start_offset(110);
        chan.set_pre_play_samples(90);
        loop.set_length(128);

        auto play_buf = MidiTestBuffer();
        loop.plan_transition(Playing);
        chan.PROC_set_playback_buffer(&play_buf, 256);

        // Pre-play part
        process(99);

        expect(eq(sync_source->get_mode(), Playing));
        expect(eq(loop.get_mode(), Stopped));

        process(1);

        sync_source->PROC_handle_poi();
        loop.PROC_handle_sync();
        expect(eq(sync_source->get_mode(), Playing));
        expect(eq(loop.get_mode(), Playing));

        // Play part
        process(28);

        expect(eq(sync_source->get_mode(), Playing));
        expect(eq(loop.get_mode(), Playing));

        // Process the channels' queued operations
        chan.PROC_finalize_process();

        // We expect the first 10 messages to be skipped, playback
        // starting immediately from msg 10.
        auto msgs = play_buf.written;
        expect(eq(msgs.size(), 118));

        size_t input_msg_idx = 10;
        size_t output_msg_idx = 0;
        for (size_t t=0; t<128; t++) {
            if (t < 10) { input_msg_idx++; continue; }
            Message exp = data.at(input_msg_idx++);
            exp.time = t;
            check_msgs_equal(msgs.at(output_msg_idx++), exp, 0, std::to_string(t));
        }
    };

    "ml_6_state_tracking"_test = []() {
            AudioMidiLoop loop;
        loop.add_midi_channel<uint32_t, uint16_t>(100000, Direct, false);
        auto &chan = *loop.midi_channel<uint32_t, uint16_t>(0);
        using Message = MidiChannel<uint32_t, uint16_t>::Message;

        auto pitch_wheel_bytes = [](uint8_t channel, uint16_t val) {
            return std::vector<uint8_t>({
                (uint8_t)(0xE0 | channel),
                (uint8_t)(val & 0b1111111),
                (uint8_t)((val >> 7) & 0b1111111)
            });
        };
        auto pitch_wheel = [&](size_t time, uint8_t channel, uint16_t val) {
            return Msg(time, 3, pitch_wheel_bytes(channel, val));
        };
        auto note_on = [](size_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
            return Msg(time, 3, {(uint8_t)(0x90 | channel), note, velocity});
        };
        auto note_off = [](size_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
            return Msg(time, 3, {(uint8_t)(0x80 | channel), note, velocity});
        };

        // Set up a sequence where notes are played every 10 ticks,
        // with a linearly changing pitch wheel.
        // The sequence will be split up into multiple read buffers.
        // Sequence:
        // - pitch wheel goes up by 1 per tick, starting at 10
        // - note on every 10 ticks @ offset 2
        // - note off every 10 ticks @ offset 5
        // - sequence divided over 10 buffers of 10 ticks each
        std::vector<MidiTestBuffer> input_buffers;
        for(size_t i=0; i<10; i++) {
            input_buffers.push_back(MidiTestBuffer());
            auto &buf = input_buffers.back();
            for(size_t j=0; j<10; j++) {
                buf.read.push_back( pitch_wheel(j, 0, 10 + i*10+j) );
                if (j == 2) {
                    buf.read.push_back( note_on(j, 0, 50, 100) );
                }
                if (j == 5) {
                    buf.read.push_back( note_off(j, 0, 50, 100) );
                }
            }
        }

        auto get_expected_output_msgs = [&](
            std::vector<Msg> prepend,
            size_t from_time,
            size_t to_time
        ) {
            std::vector<Msg> r = prepend;
            for(size_t i=0; i<input_buffers.size(); i++) {
                for(auto msg : input_buffers[i].read) {
                    msg.time += 10*i;
                    if (msg.time >= from_time && msg.time < to_time) {
                        r.push_back(msg);
                    }
                }
            }
            return r;
        };

        // Record the prepared data.
        loop.plan_transition(Recording);
        loop.PROC_trigger();
        loop.PROC_update_poi();
        expect(eq(loop.get_mode() , Recording));
        for (auto &buf: input_buffers) {
            chan.PROC_set_recording_buffer(&buf, 10);
            loop.PROC_update_poi();
            loop.PROC_process(10);
        }
        expect(eq(loop.get_length(), 100));

        // Now, stop recording but also make some changes on the input.
        loop.plan_transition(Stopped, 0, false, false);
        loop.PROC_update_poi();
        MidiTestBuffer another;
        another.read.push_back(pitch_wheel(0, 0, 150));
        // Also an unrelated pitch wheel change on other channel
        another.read.push_back(pitch_wheel(0, 10, 100));
        // Also press the hold pedal
        another.read.push_back(Msg(0, 3, {0xB1, 64, 127 }));
        expect(eq(loop.get_mode(), Stopped));
        chan.PROC_set_recording_buffer(&another, 10);
        loop.PROC_update_poi();
        loop.PROC_process(10);

        expect(eq(loop.get_length(), 100));

        // Now, play back the first 10 samples.
        MidiTestBuffer play_buf;
        loop.plan_transition(Playing, 0, false, false);
        chan.PROC_set_playback_buffer(&play_buf, 100);
        loop.PROC_update_poi();
        loop.PROC_process(99);

        expect(eq(loop.get_position(), 99));
        expect(eq(loop.get_mode(), Playing));

        // In terms of pitch wheel changes, we expect:
        // - first a message to revert the pitch back to the original state (0),
        //   as part of state tracking/restore functionality
        // - then subsequently the same sequence of pitch wheel changes we put in
        //   (10 + position).
        auto &msgs = play_buf.written;
        auto channel_0_view = play_buf.written |
            std::ranges::views::filter([](Msg &msg) {
                return msg.size == 3 && channel(msg.data.data()) == 0;
            });
        auto channel_0 = std::vector<Msg>(channel_0_view.begin(), channel_0_view.end());
        check_msg_vectors_equal(
            channel_0,
            get_expected_output_msgs(
                { pitch_wheel(0, 0, 0x2000) }, // State restore message
                0,
                99
            ));

        auto channel_10_view = play_buf.written |
            std::ranges::views::filter([](Msg &msg) {
                return msg.size == 3 && channel(msg.data.data()) == 10;
            });
        auto channel_10 = std::vector<Msg>(channel_10_view.begin(), channel_10_view.end());
        check_msg_vectors_equal(channel_10, std::vector<Msg>({
            pitch_wheel(0, 10, 0x2000),
        }));

        auto channel_1_view = play_buf.written |
            std::ranges::views::filter([](Msg &msg) {
                return msg.size == 3 && channel(msg.data.data()) == 1;
            });
        auto channel_1 = std::vector<Msg>(channel_1_view.begin(), channel_1_view.end());
        check_msg_vectors_equal(channel_1, std::vector<Msg>({
            Msg(0, 3, {0xB1, 64, 0 })
        }));
    };
};