#include <catch2/catch_test_macros.hpp>
#include "DummyAudioMidiDriver.h"
#include "CustomProcessingChain.h"
#include "AudioMidiLoop.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "GraphPort.h"
#include "catch2/catch_approx.hpp"
#include "helpers.h"
#include "types.h"
#include <memory>
#include "libshoopdaloop_test_if.h"
#include "libshoopdaloop.h"
#include "shoop_globals.h"
#include <iostream>
#include <thread>
#include "LoggingEnabled.h"

struct SingleDirectLoopTestChain : public ModuleLoggingEnabled<"Test.SingleDirectLoopTestChain"> {

    shoop_backend_session_t *api_backend_session;
    std::shared_ptr<BackendSession> int_backend_session;

    shoop_audio_driver_t *api_driver;
    std::shared_ptr<shoop_types::_DummyAudioMidiDriver> int_driver;

    shoopdaloop_audio_port_t *api_input_port;
    std::shared_ptr<GraphPort> int_input_port;
    DummyAudioPort* int_dummy_input_port;

    shoopdaloop_audio_port_t *api_output_port;
    std::shared_ptr<GraphPort> int_output_port;
    DummyAudioPort* int_dummy_output_port;

    shoopdaloop_midi_port_t *api_midi_input_port;
    std::shared_ptr<GraphPort> int_midi_input_port;
    DummyMidiPort* int_dummy_midi_input_port;

    shoopdaloop_midi_port_t *api_midi_output_port;
    std::shared_ptr<GraphPort> int_midi_output_port;
    DummyMidiPort* int_dummy_midi_output_port;

    shoopdaloop_loop_t *api_loop;
    std::shared_ptr<GraphLoop> int_loop;

    shoopdaloop_loop_t *api_sync_loop;
    std::shared_ptr<GraphLoop> int_sync_loop;

    std::shared_ptr<ObjectPool<AudioBuffer<float>>> buffer_pool;

    shoopdaloop_loop_audio_channel_t *api_audio_chan;
    shoopdaloop_loop_midi_channel_t *api_midi_chan;
    std::shared_ptr<GraphLoopChannel> int_audio_chan_node;
    std::shared_ptr<GraphLoopChannel> int_midi_chan_node;
    std::shared_ptr<shoop_types::LoopAudioChannel> int_audio_chan;
    std::shared_ptr<shoop_types::LoopMidiChannel> int_midi_chan;

    SingleDirectLoopTestChain() {
        api_backend_session = create_backend_session();
        int_backend_session = internal_backend_session(api_backend_session);

        api_driver = create_audio_driver(Dummy);
        int_driver = std::dynamic_pointer_cast<_DummyAudioMidiDriver>(internal_audio_driver(api_driver));

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

        int_audio_chan = std::dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_audio_chan_node->channel);
        int_midi_chan = std::dynamic_pointer_cast<shoop_types::LoopMidiChannel>(int_midi_chan_node->channel);
        
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