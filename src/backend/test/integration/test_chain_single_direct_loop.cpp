#include "DummyAudioMidiDriver.h"
#include "CustomProcessingChain.h"
#include "AudioMidiLoop.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "GraphPort.h"
#include "helpers.h"
#include "types.h"
#include <limits>
#include <memory>
#include "libshoopdaloop_test_if.h"
#include "libshoopdaloop_backend.h"
#include "shoop_globals.h"
#include <iostream>
#include <thread>
#include "LoggingEnabled.h"

#include <catch2/catch_test_macros.hpp>
#include "catch2/catch_approx.hpp"

struct SingleDirectLoopTestChain : public ModuleLoggingEnabled<"Test.SingleDirectLoopTestChain"> {

    shoop_backend_session_t *api_backend_session;
    shoop_shared_ptr<BackendSession> int_backend_session;

    shoop_audio_driver_t *api_driver;
    shoop_shared_ptr<shoop_types::_DummyAudioMidiDriver> int_driver;

    shoopdaloop_audio_port_t *api_input_port;
    shoop_shared_ptr<GraphPort> int_input_port;
    DummyAudioPort* int_dummy_input_port;

    shoopdaloop_audio_port_t *api_output_port;
    shoop_shared_ptr<GraphPort> int_output_port;
    DummyAudioPort* int_dummy_output_port;

    shoopdaloop_midi_port_t *api_midi_input_port;
    shoop_shared_ptr<GraphPort> int_midi_input_port;
    DummyMidiPort* int_dummy_midi_input_port;

    shoopdaloop_midi_port_t *api_midi_output_port;
    shoop_shared_ptr<GraphPort> int_midi_output_port;
    DummyMidiPort* int_dummy_midi_output_port;

    shoopdaloop_loop_t *api_loop;
    shoop_shared_ptr<GraphLoop> int_loop;

    shoopdaloop_loop_t *api_sync_loop;
    shoop_shared_ptr<GraphLoop> int_sync_loop;

    shoop_shared_ptr<ObjectPool<AudioBuffer<float>>> buffer_pool;

    shoopdaloop_loop_audio_channel_t *api_audio_chan;
    shoopdaloop_loop_midi_channel_t *api_midi_chan;
    shoop_shared_ptr<GraphLoopChannel> int_audio_chan_node;
    shoop_shared_ptr<GraphLoopChannel> int_midi_chan_node;
    shoop_shared_ptr<shoop_types::LoopAudioChannel> int_audio_chan;
    shoop_shared_ptr<shoop_types::LoopMidiChannel> int_midi_chan;

    SingleDirectLoopTestChain() {
        api_backend_session = create_backend_session();
        int_backend_session = internal_backend_session(api_backend_session);

        api_driver = create_audio_driver(Dummy);
        int_driver = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(internal_audio_driver(api_driver));

        auto settings = DummyAudioMidiDriverSettings{};
        int_driver->start(settings);
        set_audio_driver(api_backend_session, api_driver);

        api_input_port = open_driver_audio_port(api_backend_session, api_driver, "sys_audio_in", ShoopPortDirection_Input, 1);
        api_output_port = open_driver_audio_port(api_backend_session, api_driver, "sys_audio_out", ShoopPortDirection_Output, 0);
        int_input_port = internal_audio_port(api_input_port);
        int_output_port = internal_audio_port(api_output_port);
        int_dummy_input_port = dynamic_cast<DummyAudioPort*>(&int_input_port->get_port());
        int_dummy_output_port = dynamic_cast<DummyAudioPort*>(&int_output_port->get_port());

        api_midi_input_port = open_driver_midi_port(api_backend_session, api_driver, "sys_midi_in", ShoopPortDirection_Input, 1);
        api_midi_output_port = open_driver_midi_port(api_backend_session, api_driver, "sys_midi_out", ShoopPortDirection_Output, 0);
        int_midi_input_port = internal_midi_port(api_midi_input_port);
        int_midi_output_port = internal_midi_port(api_midi_output_port);
        int_dummy_midi_input_port = dynamic_cast<DummyMidiPort*>(&int_midi_input_port->get_port());
        int_dummy_midi_output_port = dynamic_cast<DummyMidiPort*>(&int_midi_output_port->get_port());

        api_loop = create_loop(api_backend_session);
        int_loop = internal_loop(api_loop);

        api_sync_loop = create_loop(api_backend_session);
        int_sync_loop = internal_loop(api_sync_loop);

        api_audio_chan = add_audio_channel(api_loop, ChannelMode_Direct);
        api_midi_chan = add_midi_channel(api_loop, ChannelMode_Direct);
        int_audio_chan_node = internal_audio_channel(api_audio_chan);
        int_midi_chan_node = internal_midi_channel(api_midi_chan);

        // Note: need to wait for channels to really appear
        int_driver->wait_process();

        int_audio_chan = shoop_dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_audio_chan_node->channel);
        int_midi_chan = shoop_dynamic_pointer_cast<shoop_types::LoopMidiChannel>(int_midi_chan_node->channel);

        if(!int_audio_chan) { throw std::runtime_error("audio channel is null"); }
        if(!int_midi_chan) { throw std::runtime_error("midi channel is null"); }

        int_driver->enter_mode(DummyAudioMidiDriverMode::Controlled);

        set_loop_sync_source(api_loop, api_sync_loop);

        connect_audio_input(api_audio_chan, api_input_port);
        connect_audio_output(api_audio_chan, api_output_port);
        connect_midi_input(api_midi_chan, api_midi_input_port);
        connect_midi_output(api_midi_chan, api_midi_output_port);

        connect_audio_port_internal(api_input_port, api_output_port);
        connect_midi_port_internal(api_midi_input_port, api_midi_output_port);

        set_audio_port_passthroughMuted(api_input_port, 0);
        set_audio_port_muted(api_input_port, 0);
        set_audio_port_gain(api_input_port, 1.0f);
        set_midi_port_passthroughMuted(api_midi_input_port, 0);
        set_midi_port_muted(api_midi_input_port, 0);
        set_audio_port_passthroughMuted(api_output_port, 0);
        set_audio_port_muted(api_output_port, 0);
        set_audio_port_gain(api_output_port, 1.0f);
        set_midi_port_passthroughMuted(api_midi_output_port, 0);
        set_midi_port_muted(api_midi_output_port, 0);

        int_backend_session->wait_graph_up_to_date();
    }
};

inline std::vector<float> zeroes(uint32_t n) { return std::vector<float>(n, 0); }

inline shoop_audio_channel_data_t to_api_data(std::vector<float> &vec) {
    shoop_audio_channel_data_t rval;
    rval.data = vec.data();
    rval.n_samples = vec.size();
    return rval;
}

template<typename Msg>
inline Msg at_time(Msg const& m, uint32_t time) {
    auto _m = m;
    _m.time = time;
    return _m;
}

TEST_CASE("Chain - Direct playback MIDI basic", "[chain][midi]") {
    SingleDirectLoopTestChain tst;

    std::vector<shoop_types::_MidiMessage> msgs = {
        create_noteOn<shoop_types::_MidiMessage>(0, 1, 10, 10),
        create_noteOff<shoop_types::_MidiMessage>(0, 10, 10, 20),
        create_noteOn<shoop_types::_MidiMessage>(20, 2, 1, 1)
    };

    auto sequence = convert_midi_msgs_to_api(msgs);
    load_midi_channel_data(tst.api_midi_chan, sequence);
    destroy_midi_sequence(sequence);
    set_loop_length(tst.api_loop, 30);
    loop_transition(tst.api_loop, LoopMode_Playing, -1, -1);

    tst.int_dummy_midi_output_port->request_data(50);
    tst.int_driver->controlled_mode_request_samples(30);
    tst.int_dummy_output_port->request_data(30);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_midi_output_port->get_written_requested_msgs();
    REQUIRE(result_data.size() == 3);
    CHECK_MSGS_EQUAL(result_data[0], msgs[0]);
    CHECK_MSGS_EQUAL(result_data[1], msgs[1]);
    CHECK_MSGS_EQUAL(result_data[2], msgs[2]);

    tst.int_driver->close();
};

TEST_CASE("Chain - Direct adopt audio ringbuffer - no sync loop", "[chain][audio]") {
    SingleDirectLoopTestChain tst;

    // Process 8 samples in stopped mode
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 0, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // Since the sync loop is empty, the fallback behavior here should be that the
    // whole ringbuffer is adopted and start offset is 0.
    auto n_ringbuffer_samples = tst.int_dummy_input_port->get_ringbuffer_n_samples();
    auto data = tst.int_audio_chan->get_data(true);

    CHECK(tst.int_audio_chan->get_start_offset() == 0);
    CHECK(tst.int_loop->loop->get_length() == n_ringbuffer_samples);

    REQUIRE(data.size() == n_ringbuffer_samples);
    auto last_eight = std::vector<float>(data.end() - 8, data.end());
    CHECK(last_eight == input_data);
};

TEST_CASE("Chain - Direct adopt MIDI ringbuffer - no sync loop - 0 samples", "[chain][midi]") {
    SingleDirectLoopTestChain tst;

    // Queue some messages
    std::vector<shoop_types::_MidiMessage> msgs = {
        create_noteOn <shoop_types::_MidiMessage>(0, 1,  10, 10),
        create_noteOff<shoop_types::_MidiMessage>(5, 10, 10, 20),
        create_noteOn <shoop_types::_MidiMessage>(9, 2,  1,  1 )
    };
    for (auto &msg : msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Process 8 samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 0, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // The ringbuffer size is 0 by default, so all messages should be discarded
    auto n_ringbuffer_samples = tst.int_dummy_input_port->get_ringbuffer_n_samples();
    auto contents = tst.int_midi_chan->retrieve_contents(false);
    CHECK(contents.recorded_msgs.size() == 0);
};

TEST_CASE("Chain - Direct adopt MIDI ringbuffer - no sync loop", "[chain][midi]") {
    SingleDirectLoopTestChain tst;
    const size_t n_ringbuffer_samples = 20;
    tst.int_dummy_midi_input_port->set_ringbuffer_n_samples(n_ringbuffer_samples);

    // Process some samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_driver->controlled_mode_run_request();

    // Queue some messages
    std::vector<shoop_types::_MidiMessage> msgs = {
        create_noteOn <shoop_types::_MidiMessage>(0, 1,  10, 10),
        create_noteOff<shoop_types::_MidiMessage>(5, 10, 10, 20),
        create_noteOn <shoop_types::_MidiMessage>(9, 2,  1,  1 )
    };
    for (auto &msg : msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Process 8 samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 0, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // Since the sync loop is empty, the fallback behavior here should be that the
    // whole ringbuffer is adopted and start offset is 0.
    // That means that the messages are at the very end.
    auto const t_base = n_ringbuffer_samples - 8;
    auto contents = tst.int_midi_chan->retrieve_contents(false);

    CHECK(tst.int_dummy_midi_input_port->get_ringbuffer_n_samples() == n_ringbuffer_samples);
    CHECK(contents.recorded_msgs.size() == 2);
    CHECK(contents.recorded_msgs[0] == at_time(msgs[0], t_base + 0));
    CHECK(contents.recorded_msgs[1] == at_time(msgs[1], t_base + 5));
};

TEST_CASE("Chain - Direct adopt MIDI ringbuffer - no sync loop - integer time overflow", "[chain][midi]") {
    SingleDirectLoopTestChain tst;
    const size_t n_ringbuffer_samples = 20;
    tst.int_dummy_midi_input_port->set_ringbuffer_n_samples(n_ringbuffer_samples);

    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_driver->controlled_mode_run_request();

    // Process samples such that the midi ringbuffer is exactly 4 samples removed
    // from having integer overflow on its time values.
    auto const target = std::numeric_limits<uint32_t>::max() - 4;
    auto ringbuffer = MidiPortTestHelper::get_ringbuffer(*tst.int_dummy_midi_input_port);
    while (ringbuffer->get_current_end_time() < target) {
        auto const end_time = ringbuffer->get_current_end_time();
        auto const process = std::min(512, (int)(target - end_time));
        ringbuffer->next_buffer(process);
    }

    // Queue some messages
    std::vector<shoop_types::_MidiMessage> msgs = {
        create_noteOn <shoop_types::_MidiMessage>(0, 1,  10, 10),
        create_noteOff<shoop_types::_MidiMessage>(6, 10, 10, 20),
        create_noteOn <shoop_types::_MidiMessage>(9, 2,  1,  1 )
    };
    for (auto &msg : msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Process 8 samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 0, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // Since the sync loop is empty, the fallback behavior here should be that the
    // whole ringbuffer is adopted and start offset is 0.
    // That means that the messages are at the very end.
    auto const t_base = n_ringbuffer_samples - 8;
    auto contents = tst.int_midi_chan->retrieve_contents(false);

    CHECK(tst.int_dummy_midi_input_port->get_ringbuffer_n_samples() == n_ringbuffer_samples);
    CHECK(contents.recorded_msgs.size() == 2);
    CHECK(contents.recorded_msgs[0] == at_time(msgs[0], t_base + 0));
    CHECK(contents.recorded_msgs[1] == at_time(msgs[1], t_base + 6));
};

TEST_CASE("Chain - Direct adopt audio ringbuffer - one cycle", "[chain][audio]") {
    SingleDirectLoopTestChain tst;

    // Process 8 samples in stopped mode
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_sync_loop->loop->set_length(3, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    tst.int_driver->controlled_mode_request_samples(7); // Two cycles and 1 sample
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer. Offset 1 means last completed cycle.
    adopt_ringbuffer_contents(tst.api_loop, 1, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should have grabbed the full last completed cycle, which is samples [4, 5, 6].
    auto n_ringbuffer_samples = tst.int_dummy_input_port->get_ringbuffer_n_samples();
    auto data = tst.int_audio_chan->get_data(true);

    CHECK(tst.int_loop->loop->get_length() == 3);
    auto so = tst.int_audio_chan->get_start_offset();
    CHECK(data.size() >= 3);
    REQUIRE((int)data.size() - (int)so >= 3); // 3 samples available
    auto samples_of_interest = std::vector<float>(data.begin() + so, data.begin() + so + 3);
    CHECK(samples_of_interest == std::vector<float>({4, 5, 6}));
};

TEST_CASE("Chain - Direct adopt MIDI ringbuffer - one cycle", "[chain][midi]") {
    SingleDirectLoopTestChain tst;
    const size_t n_ringbuffer_samples = 20;
    tst.int_dummy_midi_input_port->set_ringbuffer_n_samples(n_ringbuffer_samples);
    tst.int_sync_loop->loop->set_length(4, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    // Process ringbuffer samples such that the midi ringbuffer is exactly 6 samples removed
    // from having integer overflow on its time values.
    auto const target = std::numeric_limits<uint32_t>::max() - 6;
    auto ringbuffer = MidiPortTestHelper::get_ringbuffer(*tst.int_dummy_midi_input_port);
    while (ringbuffer->get_current_end_time() < target) {
        auto const end_time = ringbuffer->get_current_end_time();
        auto const process = std::min(512, (int)(target - end_time));
        ringbuffer->next_buffer(process);
    }

    // Queue some messages
    std::vector<shoop_types::_MidiMessage> msgs = {
        // note: cycle R should be recorded
        create_noteOn <shoop_types::_MidiMessage>(0, 1,  10, 10), // 1st frame of cycle R - 1
        create_noteOff<shoop_types::_MidiMessage>(3, 10, 10, 20), // 4th frame of cycle R - 1
        create_noteOn <shoop_types::_MidiMessage>(4, 2,  20, 20), // 1st frame of cycle R
        create_noteOff<shoop_types::_MidiMessage>(7, 20, 20, 20), // 4th frame of cycle R
        create_noteOn <shoop_types::_MidiMessage>(8, 3,  30, 30), // 1st frame of cycle R + 1
    };
    for (auto &msg : msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Process 8 samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(10); // Process cycles R-1, R, 2 frames of R+1
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 1, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should now have captured the messages from cycle R.
    auto const so = tst.int_midi_chan->get_start_offset();
    auto contents = tst.int_midi_chan->retrieve_contents(false);
    CHECK(tst.int_dummy_midi_input_port->get_ringbuffer_n_samples() == n_ringbuffer_samples);
    CHECK(so == 14);
    std::vector<shoop_types::_MidiMessage> expect = {
        at_time(msgs[0], so - 4),
        at_time(msgs[1], so - 1),
        at_time(msgs[2], so + 0),
        at_time(msgs[3], so + 3),
        at_time(msgs[4], so + 4)
    };
    CHECK(contents.recorded_msgs == expect);
};

TEST_CASE("Chain - Direct adopt MIDI ringbuffer - one cycle with state restore", "[chain][midi]") {
    SingleDirectLoopTestChain tst;
    const size_t n_ringbuffer_samples = 20;
    tst.int_dummy_midi_input_port->set_ringbuffer_n_samples(n_ringbuffer_samples);
    tst.int_sync_loop->loop->set_length(4, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    // Play some note on's and set some control values.
    std::vector<shoop_types::_MidiMessage> prearm_msgs = {
        create_noteOn <shoop_types::_MidiMessage>(0, 0,  10, 10), // note 10 played
        create_noteOn <shoop_types::_MidiMessage>(0, 0,  20, 20), // note 20 played
        create_noteOn <shoop_types::_MidiMessage>(0, 0,  30, 30), // note 30 played
        create_cc     <shoop_types::_MidiMessage>(0, 0,  30, 40),  // CC 30 set to 40
        create_cc     <shoop_types::_MidiMessage>(0, 0,  70, 80),  // CC 70 set to 80
        create_cc     <shoop_types::_MidiMessage>(0, 0,  90, 100), // CC 90 set to 100
    };
    for (auto &msg : prearm_msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Run these changes all the way through the ringbuffer. They should now be tracked.
    tst.int_driver->controlled_mode_request_samples(n_ringbuffer_samples * 2);
    tst.int_driver->controlled_mode_run_request();

    // Process additional ringbuffer samples such that the midi ringbuffer is exactly 6 samples removed
    // from having integer overflow on its time values.
    auto const target = std::numeric_limits<uint32_t>::max() - 6;
    auto ringbuffer = MidiPortTestHelper::get_ringbuffer(*tst.int_dummy_midi_input_port);
    while (ringbuffer->get_current_end_time() < target) {
        auto const end_time = ringbuffer->get_current_end_time();
        auto const process = std::min(512, (int)(target - end_time));
        ringbuffer->next_buffer(process);
    }

    // Queue some messages which affect the tracked state during recording.
    std::vector<shoop_types::_MidiMessage> msgs = {
        // note: cycle R should be recorded
        create_noteOff <shoop_types::_MidiMessage>(0, 0,  10, 10), // note 10 off in 1st frame of cycle R - 1
        create_cc      <shoop_types::_MidiMessage>(3, 0,  30, 20), // CC 30 to 20 in 4th frame of cycle R - 1
        create_noteOff <shoop_types::_MidiMessage>(4, 0,  20, 20), // note 20 off in 1st frame of cycle R
        create_cc      <shoop_types::_MidiMessage>(7, 0,  70, 60), // cc 70 to 60 in 4th frame of cycle R
        create_noteOff <shoop_types::_MidiMessage>(8, 0,  30, 30), // note 30 off in 1st frame of cycle R + 1
        create_cc      <shoop_types::_MidiMessage>(9, 0,  90, 80), // cc 90 to 80 in 2nd frame of cycle R + 1
    };
    for (auto &msg : msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Process 8 samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(10); // Process cycles R-1, R, 2 frames of R+1
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 1, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should now have captured the messages from cycle R.
    auto const so = tst.int_midi_chan->get_start_offset();
    auto contents = tst.int_midi_chan->retrieve_contents(false);
    CHECK(tst.int_dummy_midi_input_port->get_ringbuffer_n_samples() == n_ringbuffer_samples);
    CHECK(so == 14);
    std::vector<shoop_types::_MidiMessage> expect = {
        at_time(msgs[0], so - 4),
        at_time(msgs[1], so - 1),
        at_time(msgs[2], so + 0),
        at_time(msgs[3], so + 3),
        at_time(msgs[4], so + 4),
        at_time(msgs[5], so + 5)
    };
    CHECK(contents.recorded_msgs == expect);

    // Now play the recording back. The channel should first restore the state from the prearm messages
    // before playing back the recorded ones.
    loop_transition(tst.api_loop, LoopMode_Playing, -1, -1);
    tst.int_dummy_midi_output_port->request_data(4);
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_midi_output_port->get_written_requested_msgs();
    // Split the result into the state-restoring part and the playback part.
    size_t sz = result_data.size();
    std::vector<shoop_types::_MidiMessage> state_restore, playback;
    for(size_t i=0; i<sz; i++) {
        if (i >= (sz - 2)) {
            playback.push_back(result_data[i]);
        } else {
            state_restore.push_back(result_data[i]);
        }
    }
    std::vector<shoop_types::_MidiMessage> expect_playback = {
        at_time(msgs[2], 0),
        at_time(msgs[3], 3)
    };
    CHECK(playback == expect_playback);

    // Check the state restore messages regardless of their order.
    // We expect:
    //    - note 20 on
    //    - note 30 on
    //    - CC 70 to 80
    //    - CC 90 to 100
    CHECK(state_restore.size() == 4);
    CHECK(std::find(state_restore.begin(), state_restore.end(), prearm_msgs[1]) != state_restore.end());
    CHECK(std::find(state_restore.begin(), state_restore.end(), prearm_msgs[2]) != state_restore.end());
    CHECK(std::find(state_restore.begin(), state_restore.end(), prearm_msgs[4]) != state_restore.end());
    CHECK(std::find(state_restore.begin(), state_restore.end(), prearm_msgs[5]) != state_restore.end());
};

TEST_CASE("Chain - Direct adopt audio ringbuffer - current cycle", "[chain][audio]") {
    SingleDirectLoopTestChain tst;

    // Process 8 samples in stopped mode
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_sync_loop->loop->set_length(3, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    tst.int_driver->controlled_mode_request_samples(8); // Two cycles and 2 samples
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer. Offset 0 means currently running cycle (which has 2 samples so far)
    adopt_ringbuffer_contents(tst.api_loop, 0, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should have grabbed the partially completed current cycle.
    auto n_ringbuffer_samples = tst.int_dummy_input_port->get_ringbuffer_n_samples();
    auto data = tst.int_audio_chan->get_data(true);

    CHECK(tst.int_loop->loop->get_length() == 2);
    auto so = tst.int_audio_chan->get_start_offset();
    CHECK(data.size() >= 2);
    REQUIRE((int)data.size() - (int)so >= 2); // 2 samples available
    auto samples_of_interest = std::vector<float>(data.begin() + so, data.begin() + so + 2);
    CHECK(samples_of_interest == std::vector<float>({7, 8}));
};

TEST_CASE("Chain - Direct adopt MIDI ringbuffer - current cycle", "[chain][midi]") {
    SingleDirectLoopTestChain tst;
    const size_t n_ringbuffer_samples = 20;
    tst.int_dummy_midi_input_port->set_ringbuffer_n_samples(n_ringbuffer_samples);
    tst.int_sync_loop->loop->set_length(4, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    // Process ringbuffer samples such that the midi ringbuffer is exactly 6 samples removed
    // from having integer overflow on its time values.
    auto const target = std::numeric_limits<uint32_t>::max() - 6;
    auto ringbuffer = MidiPortTestHelper::get_ringbuffer(*tst.int_dummy_midi_input_port);
    while (ringbuffer->get_current_end_time() < target) {
        auto const end_time = ringbuffer->get_current_end_time();
        auto const process = std::min(512, (int)(target - end_time));
        ringbuffer->next_buffer(process);
    }

    // Queue some messages
    std::vector<shoop_types::_MidiMessage> msgs = {
        // note: cycle R should be recorded.
        // messages from cycle R - 1 should be kept but before the start offset.
        // messages from cycle R - 2 should be truncated away.
        create_noteOn <shoop_types::_MidiMessage>(0, 1,  10, 10), // 1st frame of cycle R - 2
        create_noteOff<shoop_types::_MidiMessage>(3, 10, 10, 20), // 4th frame of cycle R - 2
        create_noteOn <shoop_types::_MidiMessage>(4, 2,  20, 20), // 1st frame of cycle R - 1
        create_noteOff<shoop_types::_MidiMessage>(7, 20, 20, 20), // 4th frame of cycle R - 1
        create_noteOn <shoop_types::_MidiMessage>(8, 3,  30, 30), // 1st frame of cycle R
    };
    for (auto &msg : msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Process 8 samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(10); // Process cycles R-2, R-1, 2 frames of R
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 0, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should now have captured the message from cycle R.
    auto const so = tst.int_midi_chan->get_start_offset();
    auto contents = tst.int_midi_chan->retrieve_contents(false);
    CHECK(tst.int_dummy_midi_input_port->get_ringbuffer_n_samples() == n_ringbuffer_samples);
    CHECK(so == 18);
    std::vector<shoop_types::_MidiMessage> expect = {
        at_time(msgs[2], so - 4),
        at_time(msgs[3], so - 1),
        at_time(msgs[4], so)
    };
    CHECK(contents.recorded_msgs == expect);
};

TEST_CASE("Chain - Direct adopt audio ringbuffer - prev cycle", "[chain][audio]") {
    SingleDirectLoopTestChain tst;

    // Process 8 samples in stopped mode
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_sync_loop->loop->set_length(2, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    tst.int_driver->controlled_mode_request_samples(7); // Three cycles and 1 sample
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer.
    adopt_ringbuffer_contents(tst.api_loop, 2, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should have grabbed the cycle with samples [3,4].
    auto n_ringbuffer_samples = tst.int_dummy_input_port->get_ringbuffer_n_samples();
    auto data = tst.int_audio_chan->get_data(true);

    CHECK(tst.int_loop->loop->get_length() == 2);
    auto so = tst.int_audio_chan->get_start_offset();
    CHECK(data.size() >= 2);
    REQUIRE((int)data.size() - (int)so >= 2); // 2 samples available
    auto samples_of_interest = std::vector<float>(data.begin() + so, data.begin() + so + 2);
    CHECK(samples_of_interest == std::vector<float>({3, 4}));
};

TEST_CASE("Chain - Direct adopt MIDI ringbuffer - prev cycle", "[chain][midi]") {
    SingleDirectLoopTestChain tst;
    const size_t n_ringbuffer_samples = 20;
    tst.int_dummy_midi_input_port->set_ringbuffer_n_samples(n_ringbuffer_samples);
    tst.int_sync_loop->loop->set_length(4, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    // Process ringbuffer samples such that the midi ringbuffer is exactly 6 samples removed
    // from having integer overflow on its time values.
    auto const target = std::numeric_limits<uint32_t>::max() - 6;
    auto ringbuffer = MidiPortTestHelper::get_ringbuffer(*tst.int_dummy_midi_input_port);
    while (ringbuffer->get_current_end_time() < target) {
        auto const end_time = ringbuffer->get_current_end_time();
        auto const process = std::min(512, (int)(target - end_time));
        ringbuffer->next_buffer(process);
    }

    // Queue some messages
    std::vector<shoop_types::_MidiMessage> msgs = {
        // note: cycle R should be recorded
        create_noteOn <shoop_types::_MidiMessage>(0, 1,  10, 10), // 1st frame of cycle R
        create_noteOff<shoop_types::_MidiMessage>(3, 10, 10, 20), // 4th frame of cycle R
        create_noteOn <shoop_types::_MidiMessage>(4, 2,  20, 20), // 1st frame of cycle R + 1
        create_noteOff<shoop_types::_MidiMessage>(7, 20, 20, 20), // 4th frame of cycle R + 1
        create_noteOn <shoop_types::_MidiMessage>(8, 3,  30, 30), // 1st frame of cycle R + 2
    };
    for (auto &msg : msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Process 8 samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(10); // Process cycles R, R+1, 2 frames of R+2
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 2, 1, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should now have captured the message from cycle R.
    auto const so = tst.int_midi_chan->get_start_offset();
    auto contents = tst.int_midi_chan->retrieve_contents(false);
    CHECK(tst.int_dummy_midi_input_port->get_ringbuffer_n_samples() == n_ringbuffer_samples);
    CHECK(so == 10);
    std::vector<shoop_types::_MidiMessage> expect = {
        at_time(msgs[0], so),
        at_time(msgs[1], so + 3),
        at_time(msgs[2], so + 4),
        at_time(msgs[3], so + 7),
        at_time(msgs[4], so + 8)
    };
    CHECK(contents.recorded_msgs == expect);
};

TEST_CASE("Chain - Direct adopt audio ringbuffer - prev 2 cycles", "[chain][audio]") {
    SingleDirectLoopTestChain tst;

    // Process 8 samples in stopped mode
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_sync_loop->loop->set_length(2, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    tst.int_driver->controlled_mode_request_samples(7); // Three cycles and 1 sample
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer.
    adopt_ringbuffer_contents(tst.api_loop, 2, 2, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should have grabbed the cycles with samples [3,4], [5,6].
    auto n_ringbuffer_samples = tst.int_dummy_input_port->get_ringbuffer_n_samples();
    auto data = tst.int_audio_chan->get_data(true);

    CHECK(tst.int_loop->loop->get_length() == 4);
    auto so = tst.int_audio_chan->get_start_offset();
    CHECK(data.size() >= 4);
    REQUIRE((int)data.size() - (int)so >= 4); // 2 samples available
    auto samples_of_interest = std::vector<float>(data.begin() + so, data.begin() + so + 4);
    CHECK(samples_of_interest == std::vector<float>({3, 4, 5, 6}));
};

TEST_CASE("Chain - Direct adopt MIDI ringbuffer - prev 2 cycles", "[chain][midi]") {
    SingleDirectLoopTestChain tst;
    const size_t n_ringbuffer_samples = 20;
    tst.int_dummy_midi_input_port->set_ringbuffer_n_samples(n_ringbuffer_samples);
    tst.int_sync_loop->loop->set_length(4, true);
    loop_transition(tst.api_sync_loop, LoopMode_Playing, -1, -1);

    // Process ringbuffer samples such that the midi ringbuffer is exactly 6 samples removed
    // from having integer overflow on its time values.
    auto const target = std::numeric_limits<uint32_t>::max() - 6;
    auto ringbuffer = MidiPortTestHelper::get_ringbuffer(*tst.int_dummy_midi_input_port);
    while (ringbuffer->get_current_end_time() < target) {
        auto const end_time = ringbuffer->get_current_end_time();
        auto const process = std::min(512, (int)(target - end_time));
        ringbuffer->next_buffer(process);
    }

    // Queue some messages
    std::vector<shoop_types::_MidiMessage> msgs = {
        // note: cycle R should be recorded
        create_noteOn <shoop_types::_MidiMessage>(0, 1,  10, 10), // 1st frame of cycle R
        create_noteOff<shoop_types::_MidiMessage>(3, 10, 10, 20), // 4th frame of cycle R
        create_noteOn <shoop_types::_MidiMessage>(4, 2,  20, 20), // 1st frame of cycle R + 1
        create_noteOff<shoop_types::_MidiMessage>(7, 20, 20, 20), // 4th frame of cycle R + 1
        create_noteOn <shoop_types::_MidiMessage>(8, 3,  30, 30), // 1st frame of cycle R + 2
    };
    for (auto &msg : msgs) {
        tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.data.data());
    }
    // Process 8 samples in stopped mode
    tst.int_driver->controlled_mode_request_samples(10); // Process cycles R, R+1, 2 frames of R+2
    tst.int_driver->controlled_mode_run_request();

    // Grab the ringbuffer
    adopt_ringbuffer_contents(tst.api_loop, 2, 2, 0, LoopMode_Unknown);
    tst.int_driver->controlled_mode_run_request();

    // We should now have captured the message from cycle R.
    auto const so = tst.int_midi_chan->get_start_offset();
    auto contents = tst.int_midi_chan->retrieve_contents(false);
    CHECK(tst.int_dummy_midi_input_port->get_ringbuffer_n_samples() == n_ringbuffer_samples);
    CHECK(so == 10);
    std::vector<shoop_types::_MidiMessage> expect = {
        at_time(msgs[0], so),
        at_time(msgs[1], so + 3),
        at_time(msgs[2], so + 4),
        at_time(msgs[3], so + 7),
        at_time(msgs[4], so + 8)
    };
    CHECK(contents.recorded_msgs == expect);
};