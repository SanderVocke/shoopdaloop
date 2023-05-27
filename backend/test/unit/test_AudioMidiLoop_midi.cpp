#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioMidiLoop.h"
#include "MidiChannel.h"
#include "helpers.h"
#include "types.h"
#include "process_loops.h"

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

        loop.add_midi_channel<uint32_t, uint16_t>(512, Direct, false);
        auto &chan = *loop.midi_channel<uint32_t, uint16_t>(0);

        using Message = MidiChannel<uint32_t, uint16_t>::Message;
        std::vector<Message> data;
        for(size_t idx=0; idx<256; idx++) { data.push_back(Message{(unsigned)idx, 1, { (unsigned char)idx }}); }

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
        for (size_t p=0; p<118; p++) {
            check_msgs_equal(msgs.at(p), data.at(p + 10));
        }
    };
};