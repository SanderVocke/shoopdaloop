#include <catch2/catch_test_macros.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioChannel.h"
#include "AudioMidiLoop.h"
#include "helpers.h"
#include "types.h"
#include "process_loops.h"

using namespace std::chrono_literals;

TEST_CASE("AudioMidiLoop - Audio - Stop", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 256);
    AudioMidiLoop loop;
    auto chan = loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(1000);
    chan->PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);
};

TEST_CASE("AudioMidiLoop - Audio - Record", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Dry, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Wet, false);
    std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
        {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };

    auto source_buf = create_audio_buf<int>(512, [](uint32_t position) { return position; }); 
    loop.plan_transition(LoopMode_Recording);
    for (auto &c: channels) { c->PROC_set_recording_buffer(source_buf.data(), source_buf.size()); }
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode()== LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 512); // end of buffer
    REQUIRE(loop.get_length()== 0);
    REQUIRE(loop.get_position()== 0);

    loop.PROC_process(20);
    for (auto &c: channels) { c->PROC_finalize_process(); }

    REQUIRE(loop.get_mode()== LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 492); // end of buffer
    REQUIRE(loop.get_length()== 20);
    REQUIRE(loop.get_position()== 0);
    for (auto &channel : channels) {
        for_channel_elems<AudioChannel<int>, int>(
            *channel, 
            [](uint32_t position, int const& val) {
                REQUIRE(val== position);
            },
            0,
            20
        );
    }
};
    
TEST_CASE("AudioMidiLoop - Audio - Record Beyond Ext. Buf", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 256);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);
    
    channel.PROC_set_recording_buffer(nullptr, 0);
    loop.plan_transition(LoopMode_Recording);
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE_THROWS( loop.PROC_process(20) );
};

TEST_CASE("AudioMidiLoop - Audio - Record Multiple Target", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);

    auto source_buf = create_audio_buf<int>(512, [](uint32_t position) { return position; }); 
    loop.plan_transition(LoopMode_Recording);
    channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 512); // end of buffer
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(512);
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == 0); // end of buffer
    REQUIRE(loop.get_length()== 512);
    REQUIRE(loop.get_position()== 0);
    for_channel_elems<AudioChannel<int>, int>(
        channel, 
        [](uint32_t position, int const& val) {
            REQUIRE(val== position);
        }
    );
};

TEST_CASE("AudioMidiLoop - Audio - Record Multiple Source", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);

    auto source_buf = create_audio_buf<int>(32, [](uint32_t position) { return position; });
    loop.plan_transition(LoopMode_Recording);
    loop.PROC_trigger();
    channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 32); // end of buffer
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    for(uint32_t samples_processed = 0; samples_processed < 512; ) {
        REQUIRE(loop.PROC_get_next_poi().value_or(999)== 32);
        loop.PROC_process(32);
        channel.PROC_finalize_process();
        samples_processed += 32;
        source_buf = create_audio_buf<int>(32, [&](uint32_t position) { return position + samples_processed; });
        channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
        loop.PROC_update_poi();
        loop.PROC_handle_poi();
    }

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 32); // end of buffer
    REQUIRE(loop.get_length()== 512);
    REQUIRE(loop.get_position()== 0);
    for_channel_elems<AudioChannel<int>, int>(
        channel, 
        [](uint32_t position, int const& val) {
            REQUIRE(val== position);
        }
    );
};

TEST_CASE("AudioMidiLoop - Audio - Record Onto Smaller", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);
    auto data = create_audio_buf<int>(64, [](uint32_t position) { return -((int)position); });
    channel.load_data(data.data(), 64, false);
    loop.plan_transition(LoopMode_Recording);
    loop.PROC_trigger();
    loop.set_length(128);

    auto source_buf = create_audio_buf<int>(512, [](uint32_t position) { return position; }); 
    channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode()== LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 512); // end of buffer
    REQUIRE(loop.get_length()== 128);
    REQUIRE(loop.get_position()== 0);

    loop.PROC_process(20);
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode()== LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 492); // end of buffer
    REQUIRE(loop.get_length()== 148);
    REQUIRE(loop.get_position()== 0);
    // First 64 elements should be unchanged after record
    for_channel_elems<AudioChannel<int>, int>(
        channel, 
        [](uint32_t position, int const& val) {
            REQUIRE(val== -((int)position));
        },
        0,
        64
    );
    // The missing "data gap" should be filled with zeroes
    for_channel_elems<AudioChannel<int>, int>(
        channel, 
        [](uint32_t position, int const& val) {
            REQUIRE(val== 0);
        },
        64,
        64
    );
    // The new recording should be appended at the end.
    for_channel_elems<AudioChannel<int>, int>(
        channel, 
        [](uint32_t position, int const& val) {
            REQUIRE(val== position-128);
        },
        128,
        20
    );
};

TEST_CASE("AudioMidiLoop - Audio - Record Onto Larger", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);
    auto data = create_audio_buf<int>(128, [](uint32_t position) { return -((int)position); });
    channel.load_data(data.data(), 128, false);
    loop.plan_transition(LoopMode_Recording);
    loop.PROC_trigger();
    loop.set_length(64);

    auto source_buf = create_audio_buf<int>(512, [](uint32_t position) { return position; }); 
    channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode()== LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 512); // end of buffer
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 0);

    loop.PROC_process(64);
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode()== LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 448); // end of buffer
    REQUIRE(loop.get_length()== 128);
    REQUIRE(loop.get_position()== 0);
    // First 64 elements should be unchanged after record
    for_channel_elems<AudioChannel<int>, int>(
        channel, 
        [](uint32_t position, int const& val) {
            REQUIRE(val== -((int)position));
        },
        0,
        64
    );
    // The next samples should have been overwritten by the recording.
    for_channel_elems<AudioChannel<int>, int>(
        channel, 
        [](uint32_t position, int const& val) {
            REQUIRE(val== position-64);
        },
        64
    );
};

TEST_CASE("AudioMidiLoop - Audio - Playback", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Dry, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Wet, false);
    std::vector<std::shared_ptr<AudioChannel<int>>> channels =
        {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2)};
    
    auto data = create_audio_buf<int>(64, [](uint32_t position) { return position; });
    for (auto &channel : channels) { channel->load_data(data.data(), 64, false); }
    loop.set_length(64);
    std::vector<std::vector<int>> play_bufs = { std::vector<int>(64), std::vector<int>(64), std::vector<int>(64) };

    loop.plan_transition(LoopMode_Playing);
    for (uint32_t idx=0; idx<3; idx++) {
        channels[idx]->PROC_set_playback_buffer(play_bufs[idx].data(), play_bufs[idx].size());
    }
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 64); // end of buffer
    REQUIRE(loop.get_position() == 0);
    REQUIRE(loop.get_length() == 64);

    loop.PROC_process(20);
    for (auto &c: channels) { c->PROC_finalize_process(); }

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 44); // end of buffer
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 20);
    for(uint32_t idx=0; idx<20; idx++) {
        REQUIRE(play_bufs[0][idx]== data[idx]);
    }
    for(uint32_t idx=0; idx<20; idx++) {
        REQUIRE(play_bufs[1][idx]== 0); // Dry loop idle
    }  
    for(uint32_t idx=0; idx<20; idx++) {
        REQUIRE(play_bufs[2][idx]== data[idx]); // Wet loop playing
    }      
};

TEST_CASE("AudioMidiLoop - Audio - Playback Multiple Target", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);
    
    auto data = create_audio_buf<int>(512, [](uint32_t position) { return position; });
    channel.load_data(data.data(), 512, false);

    loop.set_length(512);
    loop.plan_transition(LoopMode_Playing);
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.get_position() == 0);
    REQUIRE(loop.get_length() == 512);

    auto play_buf = std::vector<int>(512);
    for(uint32_t processed = 0; processed < 512; processed+=64) {
        
        channel.PROC_set_playback_buffer(play_buf.data()+processed, 64);
        loop.PROC_update_poi();
        REQUIRE(loop.PROC_get_next_poi() == 64); // end of buffer
        loop.PROC_process(64);
        channel.PROC_finalize_process();
    }

    for (uint32_t idx=0; idx<512; idx++) {
        REQUIRE(play_buf[idx]== idx);
    }
};

TEST_CASE("AudioMidiLoop - Audio - Playback Shorter Data", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);
    
    auto data = create_audio_buf<int>(32, [](uint32_t position) { return position; });
    channel.load_data(data.data(), 32, false);
    loop.set_length(64);
    auto play_buf = std::vector<int>(64);

    loop.plan_transition(LoopMode_Playing);
    channel.PROC_set_playback_buffer(play_buf.data(), play_buf.size());
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 64); // end of buffer
    REQUIRE(loop.get_position() == 0);
    REQUIRE(loop.get_length() == 64);

    loop.PROC_process(62);
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 2); // end of buffer
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 62);
    for(uint32_t idx=0; idx<32; idx++) {
        REQUIRE(play_buf[idx]== data[idx]);
    }
    for(uint32_t idx=32; idx<62; idx++) {
        REQUIRE(play_buf[idx]== 0);
    }
};

TEST_CASE("AudioMidiLoop - Audio - Playback Wrap", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);
    
    auto data = create_audio_buf<int>(64, [](uint32_t position) { return position; });
    channel.load_data(data.data(), 64, false);
    loop.set_length(64);
    loop.set_mode(LoopMode_Playing, false);
    loop.set_position(48);
    auto play_buf = std::vector<int>(64);

    REQUIRE(loop.get_mode()==LoopMode_Playing);
    channel.PROC_set_playback_buffer(play_buf.data(), play_buf.size());
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode()==LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == (64-48)); // end of loop
    REQUIRE(loop.get_position()== 48);
    REQUIRE(loop.get_length()== 64);

    loop.PROC_process(16);
    loop.PROC_handle_poi();
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 48); // end of buffer
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 0);
    
    loop.PROC_process(48);
    loop.PROC_handle_poi();
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 0); // end of buffer
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 48);
    
    for(uint32_t idx=0; idx<(64-48); idx++) {
        REQUIRE(play_buf[idx]== data[idx+48]);
    }
    for(uint32_t idx=0; idx<48; idx++) {
        REQUIRE(play_buf[idx+(64-48)]== data[idx]);
    }
};

TEST_CASE("AudioMidiLoop - Audio - Playback Wrap Longer Data", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);
    
    auto data = create_audio_buf<int>(128, [](uint32_t position) { return position; });
    channel.load_data(data.data(), 128, false);
    loop.set_length(64);
    loop.set_mode(LoopMode_Playing, false);
    loop.set_position(48);
    auto play_buf = std::vector<int>(64);

    channel.PROC_set_playback_buffer(play_buf.data(), play_buf.size());
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == (64-48)); // end of loop
    REQUIRE(loop.get_position() == 48);
    REQUIRE(loop.get_length() == 64);

    loop.PROC_process(16);
    loop.PROC_handle_poi();
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 48); // end of buffer
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 0);
    
    loop.PROC_process(48);
    loop.PROC_handle_poi();
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi() == 0); // end of buffer
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 48);
    
    for(uint32_t idx=0; idx<(64-48); idx++) {
        REQUIRE(play_buf[idx]== data[idx+48]);
    }
    for(uint32_t idx=0; idx<48; idx++) {
        REQUIRE(play_buf[idx+(64-48)]== data[idx]);
    }
};

TEST_CASE("AudioMidiLoop - Audio - Replace", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Dry, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Wet, false);
    std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
        { loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };
    
    auto data = create_audio_buf<int>(64, [](uint32_t position) { return position; });
    for (auto &channel : channels) { channel->load_data(data.data(), 64, false); }
    loop.set_length(64);
    loop.set_mode(LoopMode_Replacing, false);
    loop.set_position(16);
    auto input_buf = create_audio_buf<int>(64, [](uint32_t position) { return -((int)position); });
    for (auto &channel: channels) { channel->PROC_set_recording_buffer(input_buf.data(), input_buf.size()); }
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Replacing);
    REQUIRE(loop.PROC_get_next_poi().value_or(0)== 64-16); // end of loop
    REQUIRE(loop.get_position() == 16);
    REQUIRE(loop.get_length() == 64);

    loop.PROC_process(32);
    for (auto &c: channels) { c->PROC_finalize_process(); }

    REQUIRE(loop.get_mode() == LoopMode_Replacing);
    REQUIRE(loop.PROC_get_next_poi().value_or(0)== 64-32-16); // end of loop
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 16+32);
    for (auto &channel : channels) {
        for_channel_elems<AudioChannel<int>, int>(
            *channel, 
            [&](uint32_t position, int const& val) {
                if(position < 16 || position >= (16+32)) {
                    //untouched
                    REQUIRE(val== data[position]);
                } else {
                    //replaced
                    REQUIRE(val== input_buf[position-16]);
                }
            });
    }
};

TEST_CASE("AudioMidiLoop - Audio - Replace Onto Smaller", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    auto &channel = *loop.audio_channel<int>(0);
    
    auto data = create_audio_buf<int>(64, [](uint32_t position) { return position; });
    channel.load_data(data.data(), 64, false);
    loop.set_length(64);
    loop.set_mode(LoopMode_Replacing, false);
    loop.set_position(48);
    auto input_buf = create_audio_buf<int>(64, [](uint32_t position) { return -((int)position); });
    channel.PROC_set_recording_buffer(input_buf.data(), input_buf.size());
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Replacing);
    REQUIRE(loop.PROC_get_next_poi().value_or(0)== 64-48); // end of loop
    REQUIRE(loop.get_position() == 48);
    REQUIRE(loop.get_length() == 64);

    loop.PROC_process(16);
    loop.PROC_handle_poi();
    loop.PROC_update_poi();
    loop.PROC_process(16);
    channel.PROC_finalize_process();

    REQUIRE(loop.get_mode() == LoopMode_Replacing);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 48);
    REQUIRE(loop.get_length()== 64); // Wrapped around
    REQUIRE(loop.get_position()== 16);
    for_channel_elems<AudioChannel<int>, int>(
        channel, 
        [&](uint32_t position, int const& val) {
            if(position < 16) {
                // last part of replace
                REQUIRE(val== input_buf[position+16]);
            } else if(position < 48) {
                //untouched
                REQUIRE(val== data[position]);
            } else {
                //replaced
                REQUIRE(val== input_buf[position-48]);
            }
        });
};

TEST_CASE("AudioMidiLoop - Audio - Play Dry Through Wet", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Dry, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Wet, false);
    std::vector<std::shared_ptr<AudioChannel<int>>> channels =
        {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2)};
    
    auto data = create_audio_buf<int>(64, [](uint32_t position) { return position; });
    for (auto &channel : channels) { channel->load_data(data.data(), 64, false); }
    loop.set_length(64);
    std::vector<std::vector<int>> play_bufs = { std::vector<int>(64), std::vector<int>(64), std::vector<int>(64) };

    loop.plan_transition(LoopMode_PlayingDryThroughWet);
    for (uint32_t idx=0; idx<3; idx++) {
        channels[idx]->PROC_set_playback_buffer(play_bufs[idx].data(), play_bufs[idx].size());
    }
    loop.PROC_trigger();
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_PlayingDryThroughWet);
    REQUIRE(loop.PROC_get_next_poi() == 64); // end of buffer
    REQUIRE(loop.get_position() == 0);
    REQUIRE(loop.get_length() == 64);

    loop.PROC_process(20);
    for (auto &c: channels) { c->PROC_finalize_process(); }

    REQUIRE(loop.get_mode() == LoopMode_PlayingDryThroughWet);
    REQUIRE(loop.PROC_get_next_poi() == 44); // end of buffer
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 20);
    for(uint32_t idx=0; idx<20; idx++) {
        REQUIRE(play_bufs[0][idx]== data[idx]);
    }
    for(uint32_t idx=0; idx<20; idx++) {
        REQUIRE(play_bufs[1][idx]== data[idx]); // Dry loop playing
    }  
    for(uint32_t idx=0; idx<20; idx++) {
        REQUIRE(play_bufs[2][idx]== 0); // Wet loop idle
    }      
};

TEST_CASE("AudioMidiLoop - Audio - Record Dry Into Wet", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Dry, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Wet, false);
    std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
        { loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };
    
    auto data = create_audio_buf<int>(64, [](uint32_t position) { return position; });
    for (auto &channel : channels) { channel->load_data(data.data(), 64, false); }
    loop.set_length(64);
    loop.set_mode(LoopMode_RecordingDryIntoWet, false);
    loop.set_position(16);
    auto input_buf = create_audio_buf<int>(64, [](uint32_t position) { return -((int)position); });
    for (auto &channel: channels) { channel->PROC_set_recording_buffer(input_buf.data(), input_buf.size()); }
    std::vector<std::vector<int>> output_bufs = {std::vector<int>(32), std::vector<int>(32), std::vector<int>(32)};
    for (uint32_t idx=0; idx<3; idx++) {
        channels[idx]->PROC_set_playback_buffer(output_bufs[idx].data(), 32);
    }
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_RecordingDryIntoWet);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 32); // end of playback buffer
    REQUIRE(loop.get_position() == 16);
    REQUIRE(loop.get_length() == 64);

    loop.PROC_process(32);
    for (auto &c: channels) { c->PROC_finalize_process(); }

    REQUIRE(loop.get_mode() == LoopMode_RecordingDryIntoWet);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 0); // end of loop
    REQUIRE(loop.get_length()== 64);
    REQUIRE(loop.get_position()== 16+32);
    // ChannelMode_Direct channel does replacement
    for_channel_elems<AudioChannel<int>, int>(
        *channels[0], 
        [&](uint32_t position, int const& val) {
            if(position < 16 || position >= (16+32)) {
                //untouched
                REQUIRE(val== data[position]);
            } else {
                //replaced
                REQUIRE(val== input_buf[position-16]);
            }
        });
    for(auto &elem : output_bufs[0]) { REQUIRE(elem== 0); }
    
    // Dry channel does playback
    for_channel_elems<AudioChannel<int>, int>(
        *channels[1], 
        [&](uint32_t position, int const& val) {
            //untouched
            REQUIRE(val== data[position]);
        });
    for(uint32_t idx=0; idx<32; idx++) {
        REQUIRE(output_bufs[1][idx]== idx+16);
    }
    

    // Wet channel is replacing
    for_channel_elems<AudioChannel<int>, int>(
        *channels[2], 
        [&](uint32_t position, int const& val) {
            if(position < 16 || position >= (16+32)) {
                //untouched
                REQUIRE(val== data[position]);
            } else {
                //replaced
                REQUIRE(val== input_buf[position-16]);
            }
        });
    for(auto &elem : output_bufs[0]) { REQUIRE(elem== 0); }
};

TEST_CASE("AudioMidiLoop - Audio - Prerecord", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    AudioMidiLoop loop;
    auto sync_source = std::make_shared<AudioMidiLoop>();
    sync_source->set_length(100);
    sync_source->plan_transition(LoopMode_Playing);
    REQUIRE(sync_source->PROC_predicted_next_trigger_eta().value_or(999)== 100);

    loop.set_sync_source(sync_source); // Needed because otherwise will immediately transition
    loop.PROC_update_poi();
    loop.PROC_update_trigger_eta();
    REQUIRE(loop.PROC_predicted_next_trigger_eta().value_or(999)== 100);

    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Dry, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Wet, false);
    std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
        {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };

    auto source_buf = create_audio_buf<int>(512, [](uint32_t position) { return position; }); 
    loop.plan_transition(LoopMode_Recording); // Not triggered yet
    for (auto &c: channels) { c->PROC_set_recording_buffer(source_buf.data(), source_buf.size()); }
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode()== LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 512); // end of buffer
    REQUIRE(loop.get_length()== 0);
    REQUIRE(loop.get_position()== 0);

    loop.PROC_process(20);
    for (auto &c: channels) { c->PROC_finalize_process(); }

    // By now, we are still stopped but the channels should have pre-recorded since recording is planned.
    REQUIRE(loop.get_mode()== LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 492); // end of buffer
    REQUIRE(loop.get_length()== 0);
    REQUIRE(loop.get_position()== 0);

    loop.PROC_trigger();
    loop.PROC_update_poi();
    loop.PROC_update_trigger_eta();

    // Now, we should have moved to Recording, also meaning that the pre-recorded data should
    // now be stored into the channel and the recording appended.
    loop.PROC_process(20);
    for (auto &c: channels) { c->PROC_finalize_process(); }

    REQUIRE(loop.get_mode()== LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi().value_or(999)== 472); // end of buffer
    REQUIRE(loop.get_length()== 20);
    REQUIRE(loop.get_position()== 0);
    REQUIRE(loop.PROC_predicted_next_trigger_eta().value_or(999)== 80);

    for (auto &channel : channels) {
        CHECK(channel->get_start_offset()== 20);
        CHECK(channel->get_length() == 40);
        for_channel_elems<AudioChannel<int>, int>(
            *channel, 
            [](uint32_t position, int const& val) {
                CHECK(val == position);
            },
            0,
            40 // Note: all 40 elements checked
        );
    }

    sync_source->PROC_process(60);
    loop.PROC_update_poi();
    loop.PROC_update_trigger_eta();
    REQUIRE(loop.PROC_predicted_next_trigger_eta().value_or(999)== 40);
};

TEST_CASE("AudioMidiLoop - Audio - Preplay", "[AudioMidiLoop][audio]") {
    auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>("Test", 10, 64);
    auto loop_ptr = std::make_shared<AudioMidiLoop>();
    auto &loop = *loop_ptr;
    auto sync_source = std::make_shared<AudioMidiLoop>();

    auto process = [&](uint32_t n_samples) {
        std::set<std::shared_ptr<AudioMidiLoop>> loops ({loop_ptr, sync_source});
        process_loops<decltype(loops)::iterator>(loops.begin(), loops.end(), n_samples);
    };

    sync_source->set_length(100);
    sync_source->plan_transition(LoopMode_Playing);
    REQUIRE(sync_source->PROC_predicted_next_trigger_eta().value_or(999)== 100);

    loop.set_sync_source(sync_source); // Needed because otherwise will immediately transition
    loop.PROC_update_poi();
    loop.PROC_update_trigger_eta();
    REQUIRE(loop.PROC_predicted_next_trigger_eta().value_or(999)== 100);

    loop.add_audio_channel<int>(pool, 10, ChannelMode_Direct, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Dry, false);
    loop.add_audio_channel<int>(pool, 10, ChannelMode_Wet, false);
    std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
        {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };

    auto data = create_audio_buf<int>(256, [](uint32_t position) { return position; });
    for (auto &channel : channels) {
        channel->load_data(data.data(), 256);
        channel->set_start_offset(110);
        channel->set_pre_play_samples(90);
    }
    loop.set_length(128);

    std::vector<std::vector<int>> play_bufs = { std::vector<int>(128), std::vector<int>(128), std::vector<int>(128) };
    loop.plan_transition(LoopMode_Playing);
    for (uint32_t idx=0; idx<3; idx++) {
        channels[idx]->PROC_set_playback_buffer(play_bufs[idx].data(), play_bufs[idx].size());
    }

    // Pre-play part
    process(99);

    REQUIRE(sync_source->get_mode()== LoopMode_Playing);
    REQUIRE(loop.get_mode()== LoopMode_Stopped);

    process(1);

    sync_source->PROC_handle_poi();
    loop.PROC_handle_sync();
    REQUIRE(sync_source->get_mode()== LoopMode_Playing);
    REQUIRE(loop.get_mode()== LoopMode_Playing);

    // Play part
    process(28);

    REQUIRE(sync_source->get_mode()== LoopMode_Playing);
    REQUIRE(loop.get_mode()== LoopMode_Playing);

    // Process the channels' queued operations
    for (auto &channel : channels) {
        channel->PROC_finalize_process();
    }

    for (uint32_t idx=0; idx < 3; idx++) {
        auto &channel = channels[idx];
        auto &buf = play_bufs[idx];
        auto chan_mode = channel->get_mode();

        for (uint32_t p=0; p<128; p++) {
            if (chan_mode != ChannelMode_Dry) {
                REQUIRE(
                    buf[p] ==
                    (p < 10  ? 0 : // First 10 samples: nothing because we are before the pre_play_samples param
                    p + 10) // Rest: pre-played and normally played samples
                );
            } else {
                REQUIRE(buf[p]== 0); // Dry won't play back at all
            }
        }
    }
};