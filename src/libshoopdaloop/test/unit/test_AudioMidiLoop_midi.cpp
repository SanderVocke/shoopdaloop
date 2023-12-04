#include <catch2/catch_test_macros.hpp>
#include <catch2/generators/catch_generators.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioMidiLoop.h"
#include "MidiChannel.h"
#include "MidiStateTracker.h"
#include "helpers.h"
#include "types.h"
#include "process_loops.h"
#include <string>
#include <valarray>
#include "midi_helpers.h"
#include "LoggingEnabled.h"

using namespace std::chrono_literals;

template<typename Elem>
std::vector<Elem> filter_vector(std::vector<Elem> const& v, std::function<bool(Elem const&)> f) {
    std::vector<Elem> r;
    for (auto const& e: v) {
        if (f(e)) { r.push_back(e); }
    }
    return r;
}

template<typename MessageA, typename MessageB>
inline void check_msgs_equal(
    MessageA const& a,
    MessageB const& b,
    int time_offset=0,
    std::string log_info=""
    )
{
    CHECK(a.time == b.time+time_offset);
    CHECK(a.size == b.size);
    for (uint32_t i=0; i<a.size && i<b.size; i++) {
        CHECK((int)a.data[i] == (int)b.data[i]);
    }
}

template<typename MessageA, typename MessageB>
inline void check_msg_vectors_equal(
    std::vector<MessageA> const& a,
    std::vector<MessageB> const& b,
    std::string log_info = "")
{
    CHECK(a.size() == b.size());
    for (uint32_t i=0; i<a.size() && i < b.size(); i++) {
        check_msgs_equal(a[i], b[i], 0, "(msg " + std::to_string(i) + ") " + log_info);
    }
}

template<typename Message>
inline Message with_time (Message const& msg, uint32_t time) {
    Message m = msg;
    m.time = time;
    return m;
}

inline
std::vector<uint8_t> pitch_wheel_bytes (uint8_t channel, uint16_t val) {
    return std::vector<uint8_t>({
        (uint8_t)(0xE0 | channel),
        (uint8_t)(val & 0b1111111),
        (uint8_t)((val >> 7) & 0b1111111)
    });
}

inline
Msg pitch_wheel_msg(uint32_t time, uint8_t channel, uint16_t val) {
    return Msg(time, 3, pitch_wheel_bytes(channel, val));
}

inline Msg note_on_msg (uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    return Msg(time, 3, {(uint8_t)(0x90 | channel), note, velocity});
}
inline Msg note_off_msg (uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    return Msg(time, 3, {(uint8_t)(0x80 | channel), note, velocity});
};

TEST_CASE("AudioMidiLoop - Midi - Stop", "[AudioMidiLoop][midi]") {
    AudioMidiLoop loop;
    loop.add_midi_channel<uint32_t, uint16_t>(512, ChannelMode_Direct, false);
    auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(1000);

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);
};

TEST_CASE("AudioMidiLoop - Midi - Record", "[AudioMidiLoop][midi]") {
    AudioMidiLoop loop;
    loop.add_midi_channel<uint32_t, uint16_t>(512, ChannelMode_Direct, false);
    auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    auto source_buf = MidiTestBuffer();
    source_buf.read.push_back(Msg(10, 3, { 0x01, 0x02, 0x03 }));
    source_buf.read.push_back(Msg(19, 2, { 0x01, 0x02 }));
    source_buf.read.push_back(Msg(20, 1, { 0x01 }));
    
    loop.plan_transition(LoopMode_Recording);
    channel.PROC_set_recording_buffer(&source_buf, 512);
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 512); // end of buffer
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(20);

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 492); // end of buffer
    REQUIRE(loop.get_length() == 20);
    REQUIRE(loop.get_position() == 0);

    uint32_t length = loop.get_length();
    auto msgs = channel.retrieve_contents(false);
    REQUIRE(length == 20);
    REQUIRE(msgs.size() == 2);
    check_msgs_equal(msgs.at(0), source_buf.read.at(0));
    check_msgs_equal(msgs.at(1), source_buf.read.at(1));
};

TEST_CASE("AudioMidiLoop - Midi - Record Append Out-of-order", "[AudioMidiLoop][midi]") {
    AudioMidiLoop loop;
    loop.add_midi_channel<uint32_t, uint16_t>(512, ChannelMode_Direct, false);
    auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);
    using Message = MidiChannel<uint32_t, uint16_t>::Message;

    loop.set_mode(LoopMode_Recording, false);
    loop.set_length(100);
    auto source_buf = MidiTestBuffer();
    source_buf.read.push_back(Msg(10, 3,  { 0x01, 0x02, 0x03 }));
    source_buf.read.push_back(Msg(9,  2,  { 0x01, 0x02 }));
    source_buf.read.push_back(Msg(11, 1,  { 0x01 }));
    channel.PROC_set_recording_buffer(&source_buf, 512);
    loop.PROC_update_poi();
    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 512);
    REQUIRE(loop.get_length() == 100);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(20);

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 492); // end of buffer
    REQUIRE(loop.get_length() == 120);
    REQUIRE(loop.get_position() == 0);

    uint32_t length = loop.get_length();
    auto msgs = channel.retrieve_contents(false);
    REQUIRE(msgs.size() == 2);
    REQUIRE(length == 120);
    check_msgs_equal(msgs.at(0), with_time(source_buf.read.at(0), 110));
    check_msgs_equal(msgs.at(1), with_time(source_buf.read.at(2), 111));
};

TEST_CASE("AudioMidiLoop - Midi - Record multiple source buffers", "[AudioMidiLoop][midi]") {
    AudioMidiLoop loop;
    loop.add_midi_channel<uint32_t, uint16_t>(512, ChannelMode_Direct, false);
    auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

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
    
    loop.plan_transition(LoopMode_Recording);
    channel.PROC_set_recording_buffer(&source_bufs[0], 21);
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 21); // end of buffer
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(21);

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 0); // end of buffer
    REQUIRE(loop.get_length() == 21);
    REQUIRE(loop.get_position() == 0);

    channel.PROC_set_recording_buffer(&source_bufs[1], 9);
    loop.PROC_update_poi();
    
    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 9); // end of buffer
    REQUIRE(loop.get_length() == 21);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(9);

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 0); // end of buffer
    REQUIRE(loop.get_length() == 30);
    REQUIRE(loop.get_position() == 0);

    channel.PROC_set_recording_buffer(&source_bufs[2], 100);
    loop.PROC_update_poi();
    
    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 100); // end of buffer
    REQUIRE(loop.get_length() == 30);
    REQUIRE(loop.get_position() == 0);

    // Purposefully do not process the last message
    loop.PROC_process(5);

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 95); // end of buffer
    REQUIRE(loop.get_length() == 35);
    REQUIRE(loop.get_position() == 0);

    auto msgs = channel.retrieve_contents(false);
    REQUIRE(msgs.size() == 9);
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

TEST_CASE("AudioMidiLoop - Midi - Record onto longer buffer", "[AudioMidiLoop][midi]") {
    AudioMidiLoop loop;
    loop.add_midi_channel<uint32_t, uint16_t>(512, ChannelMode_Direct, false);
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
    loop.set_mode(LoopMode_Recording, false);
    loop.set_length(25, false);

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 0);
    REQUIRE(loop.get_length() == 25);
    REQUIRE(loop.get_position() == 0);

    auto source_buf = MidiTestBuffer();
    source_buf.read.push_back(Msg(1, 3, { 0x01, 0x02, 0x03 }));
    source_buf.read.push_back(Msg(2, 2, { 0x01, 0x02 }));
    source_buf.read.push_back(Msg(3, 1, { 0x01 }));
    channel.PROC_set_recording_buffer(&source_buf, 512);
    
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 512); // end of buffer
    REQUIRE(loop.get_length() == 25);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(20);

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 492); // end of buffer
    REQUIRE(loop.get_length() == 45);
    REQUIRE(loop.get_position() == 0);
    auto msgs = channel.retrieve_contents(false);
    REQUIRE(msgs.size() == 6);
    for (uint32_t i=0; i<3; i++) { check_msgs_equal(msgs.at(i), contents.at(i)); }
    for (uint32_t i=3; i<6; i++) { check_msgs_equal(msgs.at(i), source_buf.read.at(i-3), 25); }
};

TEST_CASE("AudioMidiLoop - Midi - Playback", "[AudioMidiLoop][midi]") {
    AudioMidiLoop loop;
    loop.add_midi_channel<uint32_t, uint16_t>(512, ChannelMode_Direct, false);
    auto &channel = *loop.midi_channel<uint32_t, uint16_t>(0);
    using Message = MidiChannel<uint32_t, uint16_t>::Message;
    std::vector<Message> contents = {
        Message(0,  1, { 0x01 }),
        Message(10, 1, { 0x02 }),
        Message(21, 1, { 0x03 }),
    };
    channel.set_contents(contents, 100, false);
    loop.set_length(100);

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 100);
    REQUIRE(loop.get_position() == 0);

    auto play_buf = MidiTestBuffer();
    
    loop.plan_transition(LoopMode_Playing);
    channel.PROC_set_playback_buffer(&play_buf, 512);
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 100); // end of loop
    REQUIRE(loop.get_length() == 100);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(20);

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 80); // end of loop
    REQUIRE(loop.get_length() == 100);
    REQUIRE(loop.get_position() == 20);

    auto msgs = play_buf.written;
    REQUIRE(msgs.size() == 2);
    check_msgs_equal(msgs.at(0), contents.at(0));
    check_msgs_equal(msgs.at(1), contents.at(1));
};

TEST_CASE("AudioMidiLoop - Midi - Prerecord", "[AudioMidiLoop][midi]") {
    AudioMidiLoop loop;
    auto sync_source = std::make_shared<AudioMidiLoop>();
    sync_source->set_length(100);
    sync_source->plan_transition(LoopMode_Playing);
    REQUIRE(sync_source->PROC_predicted_next_trigger_eta().value_or(999) == 100);

    loop.set_sync_source(sync_source); // Needed because otherwise will immediately transition
    loop.PROC_update_poi();
    loop.PROC_update_trigger_eta();
    REQUIRE(loop.PROC_predicted_next_trigger_eta().value_or(999) == 100);

    loop.add_midi_channel<uint32_t, uint16_t>(512, ChannelMode_Direct, false);
    auto &chan = *loop.midi_channel<uint32_t, uint16_t>(0);

    auto source_buf = MidiTestBuffer();
    source_buf.read.push_back(Msg(1 , 3, { 0x01, 0x02, 0x03 }));
    source_buf.read.push_back(Msg(10, 2, { 0x01, 0x02 }));
    source_buf.read.push_back(Msg(21, 1,  { 0x01 }));
    source_buf.read.push_back(Msg(39, 1,  { 0x02 }));

    loop.plan_transition(LoopMode_Recording); // Not triggered yet
    chan.PROC_set_recording_buffer(&source_buf, 512);
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 512); // end of buffer
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(20);
    chan.PROC_finalize_process();

    // By now, we are still stopped but the channels should have pre-recorded since recording is planned.
    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 492); // end of buffer
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_trigger();
    loop.PROC_update_poi();
    loop.PROC_process(20);
    chan.PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 472); // end of buffer
    REQUIRE(loop.get_length() == 20);
    REQUIRE(loop.get_position() == 0);
    REQUIRE(loop.PROC_predicted_next_trigger_eta().value_or(999) == 80);

    // Due to prerecord, should have captured all the messages.
    auto msgs = chan.retrieve_contents(false);
    REQUIRE(msgs.size() == 4);
    REQUIRE(chan.get_start_offset() == 20);
    check_msgs_equal(msgs.at(0), source_buf.read.at(0));
    check_msgs_equal(msgs.at(1), source_buf.read.at(1));
    check_msgs_equal(msgs.at(2), source_buf.read.at(2));
    check_msgs_equal(msgs.at(3), source_buf.read.at(3));

    sync_source->PROC_process(60);
    loop.PROC_update_poi();
    loop.PROC_update_trigger_eta();
    REQUIRE(loop.PROC_predicted_next_trigger_eta().value_or(999) == 40);
};

TEST_CASE("AudioMidiLoop - Midi - Preplay", "[AudioMidiLoop][midi]") {
    auto loop_ptr = std::make_shared<AudioMidiLoop>();
    auto &loop = *loop_ptr;
    auto sync_source = std::make_shared<AudioMidiLoop>();

    auto process = [&](uint32_t n_samples) {
        process_loops<AudioMidiLoop>({loop_ptr, sync_source}, n_samples);
    };

    sync_source->set_length(100);
    sync_source->plan_transition(LoopMode_Playing);
    REQUIRE(sync_source->PROC_predicted_next_trigger_eta().value_or(999) == 100);

    loop.set_sync_source(sync_source); // Needed because otherwise will immediately transition
    loop.PROC_update_poi();
    loop.PROC_update_trigger_eta();
    REQUIRE(loop.PROC_predicted_next_trigger_eta().value_or(999) == 100);

    auto chan = loop.add_midi_channel<uint32_t, uint16_t>(100000, ChannelMode_Direct, false);

    using Message = MidiChannel<uint32_t, uint16_t>::Message;
    std::vector<Message> data;
    for(uint32_t idx=0; idx<256; idx++) {
        data.push_back(
            Message {
                (unsigned)idx,
                3,
                {
                    0xB0,
                    100,
                    (unsigned char)(idx % 128)
                }
            }
        ); 
    }

    chan->set_contents(data, 256);
    chan->set_start_offset(110);
    chan->set_pre_play_samples(90);
    loop.set_length(128);

    auto play_buf = MidiTestBuffer();
    loop.plan_transition(LoopMode_Playing);
    chan->PROC_set_playback_buffer(&play_buf, 256);

    // Pre-play part
    process(99);

    REQUIRE(sync_source->get_mode() == LoopMode_Playing);
    REQUIRE(loop.get_mode() == LoopMode_Stopped);

    process(1);

    sync_source->PROC_handle_poi();
    loop.PROC_handle_sync();
    REQUIRE(sync_source->get_mode() == LoopMode_Playing);
    REQUIRE(loop.get_mode() == LoopMode_Playing);

    // Play part
    process(28);

    REQUIRE(sync_source->get_mode() == LoopMode_Playing);
    REQUIRE(loop.get_mode() == LoopMode_Playing);

    // Process the channels' queued operations
    chan->PROC_finalize_process();

    // We expect the first 10 messages to be skipped, playback
    // starting immediately from msg 10.
    auto msgs = play_buf.written;
    REQUIRE(msgs.size() == 118);

    uint32_t input_msg_idx = 10;
    uint32_t output_msg_idx = 0;
    for (uint32_t t=0; t<128; t++) {
        if (t < 10) { input_msg_idx++; continue; }
        Message exp = data.at(input_msg_idx++);
        exp.time = t;
        check_msgs_equal(msgs.at(output_msg_idx++), exp, 0, std::to_string(t));
    }
};

struct StateTrackingTestcaseData {
    uint32_t playback_from;
    uint32_t playback_to;
    uint32_t start_offset;
    uint32_t n_preplay_samples;
    std::vector<Msg> expect_channel_0;
    std::vector<Msg> expect_channel_1;
    std::vector<Msg> expect_channel_10;
};

const StateTrackingTestcaseData pb_from_first_sample = {
    .playback_from = 0,
    .playback_to   = 99,
    .start_offset  = 0,
    .n_preplay_samples = 0,
    .expect_channel_0 = {
        pitch_wheel_msg(0, 0, 0x2000), // Reset pitch on channel 0
        // ChannelMode_Direct playback of input messages
        pitch_wheel_msg(0, 0, 10),
        pitch_wheel_msg(1, 0, 11),
        pitch_wheel_msg(2, 0, 12),
        note_on_msg(2, 0, 50, 100),
        pitch_wheel_msg(3, 0, 13),
        pitch_wheel_msg(4, 0, 14),
        pitch_wheel_msg(5, 0, 15),
        note_off_msg(5, 0, 50, 100),
        pitch_wheel_msg(6, 0, 16),
        pitch_wheel_msg(7, 0, 17),
        pitch_wheel_msg(8, 0, 18),
        pitch_wheel_msg(9, 0, 19),
        pitch_wheel_msg(10, 0, 20),
        pitch_wheel_msg(11, 0, 21),
        pitch_wheel_msg(12, 0, 22),
        note_on_msg(12, 0, 50, 100),
        pitch_wheel_msg(13, 0, 23),
        pitch_wheel_msg(14, 0, 24),
        pitch_wheel_msg(15, 0, 25),
        note_off_msg(15, 0, 50, 100),
        pitch_wheel_msg(16, 0, 26),
        pitch_wheel_msg(17, 0, 27),
        pitch_wheel_msg(18, 0, 28),
        pitch_wheel_msg(19, 0, 29),
        pitch_wheel_msg(20, 0, 30),
        pitch_wheel_msg(21, 0, 31),
        pitch_wheel_msg(22, 0, 32),
        note_on_msg(22, 0, 50, 100),
        pitch_wheel_msg(23, 0, 33),
        pitch_wheel_msg(24, 0, 34),
        pitch_wheel_msg(25, 0, 35),
        note_off_msg(25, 0, 50, 100),
        pitch_wheel_msg(26, 0, 36),
        pitch_wheel_msg(27, 0, 37),
        pitch_wheel_msg(28, 0, 38),
        pitch_wheel_msg(29, 0, 39),
        pitch_wheel_msg(30, 0, 40),
        pitch_wheel_msg(31, 0, 41),
        pitch_wheel_msg(32, 0, 42),
        note_on_msg(32, 0, 50, 100),
        pitch_wheel_msg(33, 0, 43),
        pitch_wheel_msg(34, 0, 44),
        pitch_wheel_msg(35, 0, 45),
        note_off_msg(35, 0, 50, 100),
        pitch_wheel_msg(36, 0, 46),
        pitch_wheel_msg(37, 0, 47),
        pitch_wheel_msg(38, 0, 48),
        pitch_wheel_msg(39, 0, 49),
        pitch_wheel_msg(40, 0, 50),
        pitch_wheel_msg(41, 0, 51),
        pitch_wheel_msg(42, 0, 52),
        note_on_msg(42, 0, 50, 100),
        pitch_wheel_msg(43, 0, 53),
        pitch_wheel_msg(44, 0, 54),
        pitch_wheel_msg(45, 0, 55),
        note_off_msg(45, 0, 50, 100),
        pitch_wheel_msg(46, 0, 56),
        pitch_wheel_msg(47, 0, 57),
        pitch_wheel_msg(48, 0, 58),
        pitch_wheel_msg(49, 0, 59),
        pitch_wheel_msg(50, 0, 60),
        pitch_wheel_msg(51, 0, 61),
        pitch_wheel_msg(52, 0, 62),
        note_on_msg(52, 0, 50, 100),
        pitch_wheel_msg(53, 0, 63),
        pitch_wheel_msg(54, 0, 64),
        pitch_wheel_msg(55, 0, 65),
        note_off_msg(55, 0, 50, 100),
        pitch_wheel_msg(56, 0, 66),
        pitch_wheel_msg(57, 0, 67),
        pitch_wheel_msg(58, 0, 68),
        pitch_wheel_msg(59, 0, 69),
        pitch_wheel_msg(60, 0, 70),
        pitch_wheel_msg(61, 0, 71),
        pitch_wheel_msg(62, 0, 72),
        note_on_msg(62, 0, 50, 100),
        pitch_wheel_msg(63, 0, 73),
        pitch_wheel_msg(64, 0, 74),
        pitch_wheel_msg(65, 0, 75),
        note_off_msg(65, 0, 50, 100),
        pitch_wheel_msg(66, 0, 76),
        pitch_wheel_msg(67, 0, 77),
        pitch_wheel_msg(68, 0, 78),
        pitch_wheel_msg(69, 0, 79),
        pitch_wheel_msg(70, 0, 80),
        pitch_wheel_msg(71, 0, 81),
        pitch_wheel_msg(72, 0, 82),
        note_on_msg(72, 0, 50, 100),
        pitch_wheel_msg(73, 0, 83),
        pitch_wheel_msg(74, 0, 84),
        pitch_wheel_msg(75, 0, 85),
        note_off_msg(75, 0, 50, 100),
        pitch_wheel_msg(76, 0, 86),
        pitch_wheel_msg(77, 0, 87),
        pitch_wheel_msg(78, 0, 88),
        pitch_wheel_msg(79, 0, 89),
        pitch_wheel_msg(80, 0, 90),
        pitch_wheel_msg(81, 0, 91),
        pitch_wheel_msg(82, 0, 92),
        note_on_msg(82, 0, 50, 100),
        pitch_wheel_msg(83, 0, 93),
        pitch_wheel_msg(84, 0, 94),
        pitch_wheel_msg(85, 0, 95),
        note_off_msg(85, 0, 50, 100),
        pitch_wheel_msg(86, 0, 96),
        pitch_wheel_msg(87, 0, 97),
        pitch_wheel_msg(88, 0, 98),
        pitch_wheel_msg(89, 0, 99),
        pitch_wheel_msg(90, 0, 100),
        pitch_wheel_msg(91, 0, 101),
        pitch_wheel_msg(92, 0, 102),
        note_on_msg(92, 0, 50, 100),
        pitch_wheel_msg(93, 0, 103),
        pitch_wheel_msg(94, 0, 104),
        pitch_wheel_msg(95, 0, 105),
        note_off_msg(95, 0, 50, 100),
        pitch_wheel_msg(96, 0, 106),
        pitch_wheel_msg(97, 0, 107),
        pitch_wheel_msg(98, 0, 108)
    },
    .expect_channel_1 = { Msg(0, 3, {0xB1, 64, 0 }) }, // Reset hold pedal on channel 1
    .expect_channel_10 = { pitch_wheel_msg(0, 10, 0x2000) } // Reset pitch on channel 10
};

const StateTrackingTestcaseData pb_from_40th_to_50th = {
    .playback_from = 40,
    .playback_to = 50,
    .start_offset = 0,
    .n_preplay_samples = 0,
    .expect_channel_0 = {
        pitch_wheel_msg(0, 0, 49), // Reset pitch on channel 0
        pitch_wheel_msg(0, 0, 50),
        pitch_wheel_msg(1, 0, 51),
        pitch_wheel_msg(2, 0, 52),
        note_on_msg(2, 0, 50, 100),
        pitch_wheel_msg(3, 0, 53),
        pitch_wheel_msg(4, 0, 54),
        pitch_wheel_msg(5, 0, 55),
        note_off_msg(5, 0, 50, 100),
        pitch_wheel_msg(6, 0, 56),
        pitch_wheel_msg(7, 0, 57),
        pitch_wheel_msg(8, 0, 58),
        pitch_wheel_msg(9, 0, 59),
    },
    .expect_channel_1 = { Msg(0, 3, {0xB1, 64, 0 }) }, // Reset hold pedal on channel 1
    .expect_channel_10 = { pitch_wheel_msg(0, 10, 0x2000) } // Reset pitch on channel 10
};


TEST_CASE("AudioMidiLoop - Midi - State tracking", "[AudioMidiLoop][midi]") {
    StateTrackingTestcaseData testdata =
        GENERATE(pb_from_first_sample, pb_from_40th_to_50th);

    AudioMidiLoop loop;
    loop.add_midi_channel<uint32_t, uint16_t>(100000, ChannelMode_Direct, false);
    auto &chan = *loop.midi_channel<uint32_t, uint16_t>(0);
    using Message = MidiChannel<uint32_t, uint16_t>::Message;

    // Set up a sequence where notes are played every 10 ticks,
    // with a linearly changing pitch wheel.
    // The sequence will be split up into multiple read buffers.
    // Sequence:
    // - pitch wheel goes up by 1 per tick, starting at 10
    // - note on every 10 ticks @ offset 2
    // - note off every 10 ticks @ offset 5
    // - sequence divided over 10 buffers of 10 ticks each
    std::vector<MidiTestBuffer> input_buffers;
    for(uint32_t i=0; i<10; i++) {
        input_buffers.push_back(MidiTestBuffer());
        auto &buf = input_buffers.back();
        for(uint32_t j=0; j<10; j++) {
            buf.read.push_back( pitch_wheel_msg(j, 0, 10 + i*10+j) );
            if (j == 2) {
                buf.read.push_back( note_on_msg(j, 0, 50, 100) );
            }
            if (j == 5) {
                buf.read.push_back( note_off_msg(j, 0, 50, 100) );
            }
        }
    }

    // Record the prepared data.
    loop.plan_transition(LoopMode_Recording);
    loop.PROC_trigger();
    loop.PROC_update_poi();
    REQUIRE(loop.get_mode() == LoopMode_Recording);
    for (auto &buf: input_buffers) {
        chan.PROC_set_recording_buffer(&buf, 10);
        loop.PROC_update_poi();
        loop.PROC_process(10);
    }
    REQUIRE(loop.get_length() == 100);

    // Now, stop recording but also make some changes on the input.
    loop.plan_transition(LoopMode_Stopped, 0, false, false);
    loop.PROC_update_poi();
    MidiTestBuffer another;
    another.read.push_back(pitch_wheel_msg(0, 0, 150));
    // Also an unrelated pitch wheel change on other channel
    another.read.push_back(pitch_wheel_msg(0, 10, 100));
    // Also press the hold pedal
    another.read.push_back(Msg(0, 3, {0xB1, 64, 127 }));
    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    chan.PROC_set_recording_buffer(&another, 10);
    loop.PROC_update_poi();
    loop.PROC_process(10);

    REQUIRE(loop.get_length() == 100);

    // Now, play back the first 10 samples.
    MidiTestBuffer play_buf;
    uint32_t n_samples = testdata.playback_to - testdata.playback_from;
    loop.plan_transition(LoopMode_Playing, 0, false, false);
    chan.set_start_offset(testdata.start_offset);
    chan.set_pre_play_samples(testdata.n_preplay_samples);
    loop.set_position(testdata.playback_from, false);
    chan.PROC_set_playback_buffer(&play_buf, n_samples);
    loop.PROC_update_poi();
    loop.PROC_process(n_samples);

    REQUIRE(loop.get_position() == testdata.playback_to);
    REQUIRE(loop.get_mode() == LoopMode_Playing);

    // In terms of pitch wheel changes, we expect:
    // - first a message to revert the pitch back to the original state (0),
    //   as part of state tracking/restore functionality
    // - then subsequently the same sequence of pitch wheel changes we put in
    //   (10 + position).
    auto &msgs = play_buf.written;
    auto channel_0 = filter_vector<Msg>(msgs,
        [](Msg const& msg) {
            return msg.size == 3 && channel(msg.data.data()) == 0;
        }
    );
    check_msg_vectors_equal(
        channel_0, testdata.expect_channel_0,
        "(samples " + std::to_string(testdata.playback_from) + " -> " + std::to_string(testdata.playback_to) + ")"
        );

    auto channel_10 = filter_vector<Msg>(msgs,
        [](Msg const& msg) {
            return msg.size == 3 && channel(msg.data.data()) == 10;
        }
    );
    check_msg_vectors_equal(channel_10, testdata.expect_channel_10, 
    "(samples " + std::to_string(testdata.playback_from) + " -> " + std::to_string(testdata.playback_to) + ")"
    );

    auto channel_1 = filter_vector<Msg>(msgs,
        [](Msg const& msg) {
            return msg.size == 3 && channel(msg.data.data()) == 1;
        }
    );
    check_msg_vectors_equal(channel_1, testdata.expect_channel_1,
    "(samples " + std::to_string(testdata.playback_from) + " -> " + std::to_string(testdata.playback_to) + ")"
    );
}