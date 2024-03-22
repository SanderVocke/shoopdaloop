#include "AudioChannel.h"
#include "ObjectPool.h"

#include <catch2/catch_test_macros.hpp>

TEST_CASE("AudioChannel - adopt ringbuffer data", "[AudioChannel][audio]") {
    auto pool = std::make_shared<BufferQueue<int>::BufferPool>("Test", 10, 4);
    AudioChannel<int> chan(pool, 128, ChannelMode_Direct);
    
    // 12-sized buffers
    std::vector<int> recording_buffer = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12};
    std::vector<int> playback_buffer = {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};

    CHECK(chan.get_data(false).empty() == true);

    // Process 4 samples in stopped mode
    chan.PROC_set_recording_buffer(recording_buffer.data(), recording_buffer.size());
    chan.PROC_set_playback_buffer(playback_buffer.data(), playback_buffer.size());
    chan.PROC_process(
        LoopMode_Stopped,
        std::nullopt,
        std::nullopt,
        std::nullopt,
        4, //n_samples,
        0, //pos_before,
        0, //pos_after,
        0, //length_before,
        0 //length_after
    );
    chan.PROC_finalize_process();

    // Adopt the ringbuffer (set to 0 length so no effect)
    chan.adopt_ringbuffer_contents(4);
    CHECK(chan.get_data(false).empty() == true);

    // Now, allow a ringbuffer to build
    chan.set_ringbuffer_n_samples(8);

    // Process 12 samples in stopped mode
    chan.PROC_set_recording_buffer(recording_buffer.data(), recording_buffer.size());
    chan.PROC_set_playback_buffer(playback_buffer.data(), playback_buffer.size());
    chan.PROC_process(
        LoopMode_Stopped,
        std::nullopt,
        std::nullopt,
        std::nullopt,
        12, //n_samples,
        0, //pos_before,
        0, //pos_after,
        0, //length_before,
        0 //length_after
    );
    chan.PROC_finalize_process();

    // The ringbuffer should now have overflowed and contain 5..12.
    // Adopt its contents and check the data in the channel.
    // Also use a start offset somewhere in the middle.
    chan.adopt_ringbuffer_contents(4);
    chan.PROC_process( // need to process in order to run the command
        LoopMode_Stopped,
        std::nullopt,
        std::nullopt,
        std::nullopt,
        0, //n_samples,
        0, //pos_before,
        0, //pos_after,
        0, //length_before,
        0 //length_after
    );
    chan.PROC_finalize_process();

    CHECK(chan.get_data(false) == std::vector<int>({5, 6, 7, 8, 9, 10, 11, 12}));
    CHECK(chan.get_start_offset() == 4);
};
