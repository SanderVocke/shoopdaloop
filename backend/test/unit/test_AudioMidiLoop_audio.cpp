#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "AudioChannel.h"
#include "AudioMidiLoop.h"
#include "helpers.h"
#include "types.h"
#include "process_loops.h"

using namespace boost::ut;
using namespace std::chrono_literals;

suite AudioMidiLoop_audio_tests = []() {
    "al_1_stop"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 256);
        AudioMidiLoop loop;
        auto chan = loop.add_audio_channel<int>(pool, 10, Direct, false);

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        loop.PROC_process(1000);
        chan->PROC_finalize_process();

        expect(eq(loop.get_mode() , Stopped));
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));
    };

    "al_2_record"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        loop.add_audio_channel<int>(pool, 10, Dry, false);
        loop.add_audio_channel<int>(pool, 10, Wet, false);
        std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
            {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };

        auto source_buf = create_audio_buf<int>(512, [](size_t position) { return position; }); 
        loop.plan_transition(Recording);
        for (auto &c: channels) { c->PROC_set_recording_buffer(source_buf.data(), source_buf.size()); }
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 512)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 0));
        expect(eq(loop.get_position(), 0));

        loop.PROC_process(20);
        for (auto &c: channels) { c->PROC_finalize_process(); }

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 492)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));
        for (auto &channel : channels) {
            for_channel_elems<AudioChannel<int>, int>(
                *channel, 
                [](size_t position, int const& val) {
                    expect(eq(val, position)) << " @ position " << position;
                },
                0,
                20
            );
        }
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
        channel.PROC_finalize_process();

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
        loop.PROC_trigger();
        channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 32)); // end of buffer
        expect(eq(loop.get_length() , 0));
        expect(eq(loop.get_position() , 0));

        for(size_t samples_processed = 0; samples_processed < 512; ) {
            expect(eq(loop.PROC_get_next_poi().value_or(999), 32));
            loop.PROC_process(32);
            channel.PROC_finalize_process();
            samples_processed += 32;
            source_buf = create_audio_buf<int>(32, [&](size_t position) { return position + samples_processed; });
            channel.PROC_set_recording_buffer(source_buf.data(), source_buf.size());
            loop.PROC_update_poi();
            loop.PROC_handle_poi();
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
        channel.PROC_finalize_process();

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
        channel.PROC_finalize_process();

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
        loop.add_audio_channel<int>(pool, 10, Dry, false);
        loop.add_audio_channel<int>(pool, 10, Wet, false);
        std::vector<std::shared_ptr<AudioChannel<int>>> channels =
            {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2)};
        
        auto data = create_audio_buf<int>(64, [](size_t position) { return position; });
        for (auto &channel : channels) { channel->load_data(data.data(), 64, false); }
        loop.set_length(64);
        std::vector<std::vector<int>> play_bufs = { std::vector<int>(64), std::vector<int>(64), std::vector<int>(64) };

        loop.plan_transition(Playing);
        for (size_t idx=0; idx<3; idx++) {
            channels[idx]->PROC_set_playback_buffer(play_bufs[idx].data(), play_bufs[idx].size());
        }
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 64) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_position() , 0));
        expect(eq(loop.get_length() , 64));

        loop.PROC_process(20);
        for (auto &c: channels) { c->PROC_finalize_process(); }

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 44) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 20));
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_bufs[0][idx], data[idx])) << "@ position " << idx;
        }
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_bufs[1][idx], 0)) << "@ position " << idx; // Dry loop idle
        }  
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_bufs[2][idx], data[idx])) << "@ position " << idx; // Wet loop playing
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
            loop.PROC_update_poi();
            expect(loop.PROC_get_next_poi() == 64) << loop.PROC_get_next_poi().value_or(0); // end of buffer
            loop.PROC_process(64);
            channel.PROC_finalize_process();
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
        channel.PROC_finalize_process();

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
        channel.PROC_finalize_process();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 48) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 0));
        
        loop.PROC_process(48);
        loop.PROC_handle_poi();
        channel.PROC_finalize_process();

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
        channel.PROC_finalize_process();

        expect(eq(loop.get_mode() , Playing));
        expect(loop.PROC_get_next_poi() == 48) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 0));
        
        loop.PROC_process(48);
        loop.PROC_handle_poi();
        channel.PROC_finalize_process();

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

    "al_4_replace"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        loop.add_audio_channel<int>(pool, 10, Dry, false);
        loop.add_audio_channel<int>(pool, 10, Wet, false);
        std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
            { loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };
        
        auto data = create_audio_buf<int>(64, [](size_t position) { return position; });
        for (auto &channel : channels) { channel->load_data(data.data(), 64, false); }
        loop.set_length(64);
        loop.set_mode(Replacing, false);
        loop.set_position(16);
        auto input_buf = create_audio_buf<int>(64, [](size_t position) { return -((int)position); });
        for (auto &channel: channels) { channel->PROC_set_recording_buffer(input_buf.data(), input_buf.size()); }
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Replacing));
        expect(eq(loop.PROC_get_next_poi().value_or(0), 64-16)); // end of loop
        expect(eq(loop.get_position() , 16));
        expect(eq(loop.get_length() , 64));

        loop.PROC_process(32);
        for (auto &c: channels) { c->PROC_finalize_process(); }

        expect(eq(loop.get_mode() , Replacing));
        expect(eq(loop.PROC_get_next_poi().value_or(0), 64-32-16)); // end of loop
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 16+32));
        for (auto &channel : channels) {
            for_channel_elems<AudioChannel<int>, int>(
                *channel, 
                [&](size_t position, int const& val) {
                    if(position < 16 || position >= (16+32)) {
                        //untouched
                        expect(eq(val, data[position])) << "@ position " << position;
                    } else {
                        //replaced
                        expect(eq(val, input_buf[position-16])) << "@ position " << position;
                    }
                });
        }
    };

    "al_4_1_replace_onto_smaller_buffer"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        auto &channel = *loop.audio_channel<int>(0);
        
        auto data = create_audio_buf<int>(64, [](size_t position) { return position; });
        channel.load_data(data.data(), 64, false);
        loop.set_length(64);
        loop.set_mode(Replacing, false);
        loop.set_position(48);
        auto input_buf = create_audio_buf<int>(64, [](size_t position) { return -((int)position); });
        channel.PROC_set_recording_buffer(input_buf.data(), input_buf.size());
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , Replacing));
        expect(eq(loop.PROC_get_next_poi().value_or(0), 64-48)); // end of loop
        expect(eq(loop.get_position() , 48));
        expect(eq(loop.get_length() , 64));

        loop.PROC_process(16);
        loop.PROC_handle_poi();
        loop.PROC_update_poi();
        loop.PROC_process(16);
        channel.PROC_finalize_process();

        expect(eq(loop.get_mode() , Replacing));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 48));
        expect(eq(loop.get_length(), 64)); // Wrapped around
        expect(eq(loop.get_position(), 16));
        for_channel_elems<AudioChannel<int>, int>(
            channel, 
            [&](size_t position, int const& val) {
                if(position < 16) {
                    // last part of replace
                    expect(eq(val, input_buf[position+16])) << "@ position " << position;
                } else if(position < 48) {
                    //untouched
                    expect(eq(val, data[position])) << "@ position " << position;
                } else {
                    //replaced
                    expect(eq(val, input_buf[position-48])) << "@ position " << position;
                }
            });
    };

    "al_5_play_dry_through_wet"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        loop.add_audio_channel<int>(pool, 10, Dry, false);
        loop.add_audio_channel<int>(pool, 10, Wet, false);
        std::vector<std::shared_ptr<AudioChannel<int>>> channels =
            {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2)};
        
        auto data = create_audio_buf<int>(64, [](size_t position) { return position; });
        for (auto &channel : channels) { channel->load_data(data.data(), 64, false); }
        loop.set_length(64);
        std::vector<std::vector<int>> play_bufs = { std::vector<int>(64), std::vector<int>(64), std::vector<int>(64) };

        loop.plan_transition(PlayingDryThroughWet);
        for (size_t idx=0; idx<3; idx++) {
            channels[idx]->PROC_set_playback_buffer(play_bufs[idx].data(), play_bufs[idx].size());
        }
        loop.PROC_trigger();
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , PlayingDryThroughWet));
        expect(loop.PROC_get_next_poi() == 64) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_position() , 0));
        expect(eq(loop.get_length() , 64));

        loop.PROC_process(20);
        for (auto &c: channels) { c->PROC_finalize_process(); }

        expect(eq(loop.get_mode() , PlayingDryThroughWet));
        expect(loop.PROC_get_next_poi() == 44) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 20));
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_bufs[0][idx], data[idx])) << "@ position " << idx;
        }
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_bufs[1][idx], data[idx])) << "@ position " << idx; // Dry loop playing
        }  
        for(size_t idx=0; idx<20; idx++) {
            expect(eq(play_bufs[2][idx], 0)) << "@ position " << idx; // Wet loop idle
        }      
    };

    "al_6_record_dry_into_wet"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        loop.add_audio_channel<int>(pool, 10, Direct, false);
        loop.add_audio_channel<int>(pool, 10, Dry, false);
        loop.add_audio_channel<int>(pool, 10, Wet, false);
        std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
            { loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };
        
        auto data = create_audio_buf<int>(64, [](size_t position) { return position; });
        for (auto &channel : channels) { channel->load_data(data.data(), 64, false); }
        loop.set_length(64);
        loop.set_mode(RecordingDryIntoWet, false);
        loop.set_position(16);
        auto input_buf = create_audio_buf<int>(64, [](size_t position) { return -((int)position); });
        for (auto &channel: channels) { channel->PROC_set_recording_buffer(input_buf.data(), input_buf.size()); }
        std::vector<std::vector<int>> output_bufs = {std::vector<int>(32), std::vector<int>(32), std::vector<int>(32)};
        for (size_t idx=0; idx<3; idx++) {
            channels[idx]->PROC_set_playback_buffer(output_bufs[idx].data(), 32);
        }
        loop.PROC_update_poi();

        expect(eq(loop.get_mode() , RecordingDryIntoWet));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 32)); // end of playback buffer
        expect(eq(loop.get_position() , 16));
        expect(eq(loop.get_length() , 64));

        loop.PROC_process(32);
        for (auto &c: channels) { c->PROC_finalize_process(); }

        expect(eq(loop.get_mode() , RecordingDryIntoWet));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 0)); // end of loop
        expect(eq(loop.get_length(), 64));
        expect(eq(loop.get_position(), 16+32));
        // Direct channel does replacement
        for_channel_elems<AudioChannel<int>, int>(
            *channels[0], 
            [&](size_t position, int const& val) {
                if(position < 16 || position >= (16+32)) {
                    //untouched
                    expect(eq(val, data[position])) << "@ position " << position;
                } else {
                    //replaced
                    expect(eq(val, input_buf[position-16])) << "@ position " << position;
                }
            });
        for(auto &elem : output_bufs[0]) { expect(eq(elem, 0)); }
        
        // Dry channel does playback
        for_channel_elems<AudioChannel<int>, int>(
            *channels[1], 
            [&](size_t position, int const& val) {
                //untouched
                expect(eq(val, data[position])) << "@ position " << position;
            });
        for(size_t idx=0; idx<32; idx++) {
            expect(eq(output_bufs[1][idx], idx+16)) << "@ position " << idx;
        }
        

        // Wet channel is replacing
        for_channel_elems<AudioChannel<int>, int>(
            *channels[2], 
            [&](size_t position, int const& val) {
                if(position < 16 || position >= (16+32)) {
                    //untouched
                    expect(eq(val, data[position])) << "@ position " << position;
                } else {
                    //replaced
                    expect(eq(val, input_buf[position-16])) << "@ position " << position;
                }
            });
        for(auto &elem : output_bufs[0]) { expect(eq(elem, 0)); }
    };

    "al_7_prerecord"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
        AudioMidiLoop loop;
        auto sync_source = std::make_shared<AudioMidiLoop>();
        sync_source->set_length(100);
        sync_source->plan_transition(Playing);
        expect(eq(sync_source->PROC_predicted_next_trigger_eta().value_or(999), 100));

        loop.set_sync_source(sync_source); // Needed because otherwise will immediately transition
        loop.PROC_update_poi();
        loop.PROC_update_trigger_eta();
        expect(eq(loop.PROC_predicted_next_trigger_eta().value_or(999), 100));

        loop.add_audio_channel<int>(pool, 10, Direct, false);
        loop.add_audio_channel<int>(pool, 10, Dry, false);
        loop.add_audio_channel<int>(pool, 10, Wet, false);
        std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
            {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };

        auto source_buf = create_audio_buf<int>(512, [](size_t position) { return position; }); 
        loop.plan_transition(Recording); // Not triggered yet
        for (auto &c: channels) { c->PROC_set_recording_buffer(source_buf.data(), source_buf.size()); }
        loop.PROC_update_poi();

        expect(eq(loop.get_mode(), Stopped));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 512)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 0));
        expect(eq(loop.get_position(), 0));

        loop.PROC_process(20);
        for (auto &c: channels) { c->PROC_finalize_process(); }

        // By now, we are still stopped but the channels should have pre-recorded since recording is planned.
        expect(eq(loop.get_mode(), Stopped));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 492)) << loop.PROC_get_next_poi().value_or(0); // end of buffer
        expect(eq(loop.get_length(), 0));
        expect(eq(loop.get_position(), 0));

        loop.PROC_trigger();
        loop.PROC_update_poi();
        loop.PROC_update_trigger_eta();
        loop.PROC_process(20);
        for (auto &c: channels) { c->PROC_finalize_process(); }

        expect(eq(loop.get_mode(), Recording));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 472)); // end of buffer
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));
        expect(eq(loop.PROC_predicted_next_trigger_eta().value_or(999), 80));

        for (auto &channel : channels) {
            expect(eq(channel->get_start_offset(), 20));
            for_channel_elems<AudioChannel<int>, int>(
                *channel, 
                [](size_t position, int const& val) {
                    expect(eq(val, position)) << " @ position " << position;
                },
                0,
                40 // Note: all 40 elements checked
            );
        }

        sync_source->PROC_process(60);
        loop.PROC_update_poi();
        loop.PROC_update_trigger_eta();
        expect(eq(loop.PROC_predicted_next_trigger_eta().value_or(999), 40));
    };

    "al_8_preplay"_test = []() {
        auto pool = std::make_shared<ObjectPool<AudioBuffer<int>>>(10, 64);
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

        loop.add_audio_channel<int>(pool, 10, Direct, false);
        loop.add_audio_channel<int>(pool, 10, Dry, false);
        loop.add_audio_channel<int>(pool, 10, Wet, false);
        std::vector<std::shared_ptr<AudioChannel<int>>> channels = 
            {loop.audio_channel<int>(0), loop.audio_channel<int>(1), loop.audio_channel<int>(2) };

        auto data = create_audio_buf<int>(256, [](size_t position) { return position; });
        for (auto &channel : channels) {
            channel->load_data(data.data(), 256);
            channel->set_start_offset(110);
            channel->set_pre_play_samples(90);
        }
        loop.set_length(128);

        std::vector<std::vector<int>> play_bufs = { std::vector<int>(128), std::vector<int>(128), std::vector<int>(128) };
        loop.plan_transition(Playing);
        for (size_t idx=0; idx<3; idx++) {
            channels[idx]->PROC_set_playback_buffer(play_bufs[idx].data(), play_bufs[idx].size());
        }

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
        for (auto &channel : channels) {
            channel->PROC_finalize_process();
        }

        for (size_t idx=0; idx < 3; idx++) {
            auto &channel = channels[idx];
            auto &buf = play_bufs[idx];
            auto chan_mode = channel->get_mode();

            for (size_t p=0; p<128; p++) {
                if (chan_mode != Dry) {
                    expect(eq(
                        buf[p],
                        p < 10  ? 0 : // First 10 samples: nothing because we are before the pre_play_samples param
                        p + 10 // Rest: pre-played and normally played samples
                    )) << " @ position " << p << ", channel mode " << chan_mode;
                } else {
                    expect(eq(buf[p], 0)); // Dry won't play back at all
                }
            }
        }
    };
};