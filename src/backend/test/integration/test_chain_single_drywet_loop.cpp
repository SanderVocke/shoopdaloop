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

struct SingleDryWetLoopTestChain : public ModuleLoggingEnabled<"Test.SingleDryWetLoopTestChain"> {

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

    shoopdaloop_fx_chain_t *api_fx_chain;
    shoop_shared_ptr<GraphFXChain> int_fx_chain;
    shoop_shared_ptr<shoop_types::FXChain> int_custom_processing_chain;
    shoopdaloop_audio_port_t *api_fx_in;
    shoopdaloop_audio_port_t *api_fx_out;
    shoop_shared_ptr<GraphPort> int_fx_in;
    shoop_shared_ptr<GraphPort> int_fx_out;

    shoopdaloop_midi_port_t *api_fx_midi_in;
    shoop_shared_ptr<GraphPort> int_fx_midi_in;

    shoopdaloop_loop_t *api_loop;
    shoop_shared_ptr<GraphLoop> int_loop;
    shoop_shared_ptr<AudioMidiLoop> int_audiomidi_loop;

    shoopdaloop_loop_t *api_sync_loop;
    shoop_shared_ptr<GraphLoop> int_sync_loop;

    shoop_shared_ptr<ObjectPool<AudioBuffer<float>>> buffer_pool;

    shoopdaloop_loop_audio_channel_t *api_dry_chan;
    shoopdaloop_loop_audio_channel_t *api_wet_chan;
    shoop_shared_ptr<GraphLoopChannel> int_dry_chan_node;
    shoop_shared_ptr<GraphLoopChannel> int_wet_chan_node;
    shoop_shared_ptr<shoop_types::LoopAudioChannel> int_dry_audio_chan;
    shoop_shared_ptr<shoop_types::LoopAudioChannel> int_wet_audio_chan;

    shoopdaloop_loop_midi_channel_t *api_dry_midi_chan;
    shoop_shared_ptr<GraphLoopChannel> int_dry_midi_chan_node;
    shoop_shared_ptr<shoop_types::LoopMidiChannel> int_dry_midi_chan;

    SingleDryWetLoopTestChain() {
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

        api_midi_input_port = open_driver_midi_port(api_backend_session, api_driver, "sys_midi_in", ShoopPortDirection_Input, 0);
        int_midi_input_port = internal_midi_port(api_midi_input_port);
        int_dummy_midi_input_port = dynamic_cast<DummyMidiPort*>(&int_midi_input_port->get_port());

        api_fx_chain = create_fx_chain(api_backend_session, Test2x2x1, "Test");
        int_fx_chain = internal_fx_chain(api_fx_chain);
        int_custom_processing_chain = shoop_dynamic_pointer_cast<shoop_types::FXChain>(int_fx_chain->chain);
        api_fx_in = fx_chain_audio_input_port(api_fx_chain, 0);
        api_fx_out = fx_chain_audio_output_port(api_fx_chain, 0);
        int_fx_in = internal_audio_port(api_fx_in);
        int_fx_out = internal_audio_port(api_fx_out);

        api_fx_midi_in = fx_chain_midi_input_port(api_fx_chain, 0);
        int_fx_midi_in = internal_midi_port(api_fx_midi_in);

        api_loop = create_loop(api_backend_session);
        int_loop = internal_loop(api_loop);

        api_sync_loop = create_loop(api_backend_session);
        int_sync_loop = internal_loop(api_sync_loop);

        api_dry_chan = add_audio_channel(api_loop, ChannelMode_Dry);
        api_wet_chan = add_audio_channel(api_loop, ChannelMode_Wet);
        int_dry_chan_node = internal_audio_channel(api_dry_chan);
        int_wet_chan_node = internal_audio_channel(api_wet_chan);

        api_dry_midi_chan = add_midi_channel(api_loop, ChannelMode_Dry);
        int_dry_midi_chan_node = internal_midi_channel(api_dry_midi_chan);

        // Note: need to wait for channels to really appear
        int_driver->wait_process();

        int_dry_audio_chan = shoop_dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_dry_chan_node->channel);
        int_wet_audio_chan = shoop_dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_wet_chan_node->channel);
        int_dry_midi_chan = shoop_dynamic_pointer_cast<shoop_types::LoopMidiChannel>(int_dry_midi_chan_node->channel);

        if(!int_dry_audio_chan) { throw std::runtime_error("ChannelMode_Dry audio channel is null"); }
        if(!int_wet_audio_chan) { throw std::runtime_error("Wet audio channel is null"); }
        if(!int_dry_midi_chan) { throw std::runtime_error("Dry MIDI channel is null"); }

        int_driver->enter_mode(DummyAudioMidiDriverMode::Controlled);

        set_loop_sync_source(api_loop, api_sync_loop);

        connect_audio_input(api_dry_chan, api_input_port);
        connect_audio_output(api_dry_chan, api_fx_in);
        connect_audio_input(api_wet_chan, api_fx_out);
        connect_audio_output(api_wet_chan, api_output_port);
        connect_midi_input(api_dry_midi_chan, api_midi_input_port);
        connect_midi_output(api_dry_midi_chan, api_fx_midi_in);

        connect_audio_port_internal(api_input_port, api_fx_in);
        connect_audio_port_internal(api_fx_out, api_output_port);
        connect_midi_port_internal(api_midi_input_port, api_fx_midi_in);

        set_audio_port_passthroughMuted(api_input_port, 0);
        set_audio_port_muted(api_input_port, 0);
        set_audio_port_gain(api_input_port, 1.0f);
        set_midi_port_passthroughMuted(api_midi_input_port, 0);
        set_midi_port_muted(api_midi_input_port, 0);
        set_audio_port_passthroughMuted(api_output_port, 0);
        set_audio_port_muted(api_output_port, 0);
        set_audio_port_gain(api_fx_in, 1.0f);
        set_audio_port_passthroughMuted(api_fx_in, 0);
        set_audio_port_muted(api_fx_in, 0);
        set_audio_port_gain(api_fx_out, 1.0f);
        set_audio_port_passthroughMuted(api_fx_out, 0);
        set_audio_port_muted(api_fx_out, 0);
        set_audio_port_gain(api_output_port, 1.0f);
        set_audio_channel_gain(api_dry_chan, 1.0f);
        set_audio_channel_gain(api_wet_chan, 1.0f);  

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

TEST_CASE("Chain - DryWet Basic", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    std::vector<float> input_data2({-8, -7, -6, -5, -4, -3 , -2, -1});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_dummy_input_port->queue_data(8, input_data2.data());

    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    tst.int_driver->pause();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    CHECK(result_data.size() == 4);
    CHECK(tst.int_dummy_input_port->get_queue_empty() == false);
    auto expect_output_1 = std::vector<float>(input_data.begin(), input_data.begin() + 4);
    for (auto &v: expect_output_1) { v /= 2.0f; }
    CHECK(result_data == expect_output_1);
    result_data.clear();

    tst.int_driver->resume();

    tst.int_driver->controlled_mode_request_samples(12);
    tst.int_dummy_output_port->request_data(12);
    tst.int_driver->controlled_mode_run_request();

    tst.int_driver->pause();

    result_data = tst.int_dummy_output_port->dequeue_data(12);
    CHECK(result_data.size() == 12);
    CHECK(tst.int_dummy_input_port->get_queue_empty() == true);
    auto expect_output_2 = std::vector<float>(input_data.begin() + 4, input_data.end());
    expect_output_2.insert(expect_output_2.end(), input_data2.begin(), input_data2.end());
    for (auto &v: expect_output_2) { v /= 2.0f; }
    CHECK(result_data == expect_output_2);

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet record basic", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, -1, -1);
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    auto expect_data = input_data;
    for (auto &v: expect_data) { v /= 2.0f; }
    CHECK(tst.int_dry_audio_chan->get_data(true) == input_data);
    CHECK(tst.int_wet_audio_chan->get_data(true) == expect_data);

    CHECK(tst.int_input_port->maybe_audio_port()->get_input_peak() == Catch::Approx(8.0f));
    CHECK(tst.int_input_port->maybe_audio_port()->get_output_peak() == Catch::Approx(8.0f));
    CHECK(tst.int_fx_in->maybe_audio_port()->get_input_peak() == Catch::Approx(8.0f));
    CHECK(tst.int_fx_in->maybe_audio_port()->get_output_peak() == Catch::Approx(8.0f));

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet record passthrough muted", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, -1, -1);
    set_audio_port_passthroughMuted(tst.api_input_port, 1);
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(8);
    CHECK(tst.int_dry_audio_chan->get_data(true) == input_data);
    CHECK(tst.int_wet_audio_chan->get_data(true) == zeroes(8));
    CHECK(result_data == zeroes(8));

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet record muted", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, -1, -1);
    set_audio_port_muted(tst.api_input_port, 1);
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    CHECK(tst.int_dry_audio_chan->get_data(true) == zeroes(8));
    CHECK(tst.int_wet_audio_chan->get_data(true) == zeroes(8));

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet record gain", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, -1, -1);
    set_audio_port_gain(tst.api_input_port, 0.5);
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    std::vector<float> expected({0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4});
    auto expected_wet = expected;
    for (auto &v: expected_wet) { v /= 2.0f; }
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    CHECK(tst.int_dry_audio_chan->get_data(true) == expected);
    CHECK(tst.int_wet_audio_chan->get_data(true) == expected_wet);

    CHECK(tst.int_input_port->maybe_audio_port()->get_input_peak() == Catch::Approx(8.0f));
    CHECK(tst.int_input_port->maybe_audio_port()->get_output_peak() == Catch::Approx(4.0f));
    CHECK(tst.int_fx_in->maybe_audio_port()->get_input_peak() == Catch::Approx(4.0f));
    CHECK(tst.int_fx_in->maybe_audio_port()->get_output_peak() == Catch::Approx(4.0f));
    CHECK(tst.int_fx_out->maybe_audio_port()->get_input_peak() == Catch::Approx(2.0f));
    CHECK(tst.int_fx_out->maybe_audio_port()->get_output_peak() == Catch::Approx(2.0f));

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet playback basic", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, false);

    loop_transition(tst.api_loop, LoopMode_Playing, -1, -1);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    CHECK(result_data == wet_data);

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet playback gain", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4}), half_wet({0.5, 1, 1.5, 2});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, false);
    set_audio_port_gain(tst.api_output_port, 0.5);

    loop_transition(tst.api_loop, LoopMode_Playing, -1, -1);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    CHECK(result_data == half_wet);

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet dry playback basic", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, false);
    
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, -1, -1);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = dry_data;
    for (auto &v: expected) { v /= 2.0f; }
    CHECK(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet dry playback gain", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4}), half_dry({2, 1.5, 1, 0.5});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, true);

    set_audio_port_gain(tst.api_output_port, 0.5);
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, -1, -1);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = half_dry;
    for (auto &v: expected) { v /= 2.0f; }
    CHECK(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet dry playback input passthrough muted", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, true);

    set_audio_port_passthroughMuted(tst.api_input_port, 1);
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, -1, -1);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = dry_data;
    for (auto &v: expected) { v /= 2.0f; }
    CHECK(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet dry playback input muted", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, true);

    set_audio_port_muted(tst.api_input_port, 1);
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, -1, -1);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = dry_data;
    for (auto &v: expected) { v /= 2.0f; }
    CHECK(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet record MIDI basic", "[chain][midi]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, -1, -1);
    
    std::vector<Msg> msgs = {
        create_noteOn<Msg>(0, 1, 10, 10),
        create_noteOff<Msg>(0, 10, 10, 20),
        create_noteOn<Msg>(20, 2, 1, 1)
    };

    queue_midi_msgs(tst.api_midi_input_port, msgs);
    tst.int_driver->controlled_mode_request_samples(30);
    tst.int_dummy_output_port->request_data(30);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = convert_api_midi_msgs(get_midi_channel_data(tst.api_dry_midi_chan));
    REQUIRE(result_data.size() == 3);
    CHECK_MSGS_EQUAL(result_data[0], msgs[0]);
    CHECK_MSGS_EQUAL(result_data[1], msgs[1]);
    CHECK_MSGS_EQUAL(result_data[2], msgs[2]);

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet live playback MIDI basic", "[chain][midi]") {
    SingleDryWetLoopTestChain tst;
    
    std::vector<shoop_types::_MidiMessage> msgs = {
        create_noteOn<shoop_types::_MidiMessage>(0, 1, 10, 10),
        create_noteOff<shoop_types::_MidiMessage>(1, 10, 10, 20),
        create_noteOn<shoop_types::_MidiMessage>(3, 2, 1, 1)
    };

    auto sequence = convert_midi_msgs_to_api(msgs);
    load_midi_channel_data(tst.api_dry_midi_chan, sequence);
    destroy_midi_sequence(sequence);
    set_loop_length(tst.api_loop, 4);
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, -1, -1);

    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    float step = 1.0f / (float)0xFF;
    CHECK(result_data[0] == Catch::Approx(step * 10.0f));
    CHECK(result_data[1] == Catch::Approx(step * 20.0f));
    CHECK(result_data[2] == Catch::Approx(0.0f));
    CHECK(result_data[3] == Catch::Approx(step));

    tst.int_driver->close();
};

TEST_CASE("Chain - DryWet adopt audio ringbuffer - no sync loop", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

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
    auto dry_data = tst.int_dry_audio_chan->get_data(true);
    auto wet_data = tst.int_wet_audio_chan->get_data(true);
    
    CHECK(tst.int_dry_audio_chan->get_start_offset() == 0);
    CHECK(tst.int_wet_audio_chan->get_start_offset() == 0);
    CHECK(tst.int_loop->loop->get_length() == n_ringbuffer_samples);

    REQUIRE(dry_data.size() == n_ringbuffer_samples);
    REQUIRE(wet_data.size() == n_ringbuffer_samples);
    auto last_eight_dry = std::vector<float>(dry_data.end() - 8, dry_data.end());
    auto last_eight_wet = std::vector<float>(wet_data.end() - 8, wet_data.end());
    CHECK(last_eight_dry == input_data);
    CHECK(last_eight_wet == std::vector<float>({0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4}));
};

TEST_CASE("Chain - DryWet adopt audio ringbuffer - one cycle", "[chain][audio]") {
    SingleDryWetLoopTestChain tst;

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
    auto dry_data = tst.int_dry_audio_chan->get_data(true);
    auto wet_data = tst.int_wet_audio_chan->get_data(true);

    CHECK(tst.int_loop->loop->get_length() == 3);
    auto dry_so = tst.int_dry_audio_chan->get_start_offset();
    auto wet_so = tst.int_dry_audio_chan->get_start_offset();
    CHECK(dry_data.size() >= 3);
    CHECK(wet_data.size() >= 3);
    REQUIRE((int)dry_data.size() - (int)dry_so >= 3); // 3 samples available
    REQUIRE((int)wet_data.size() - (int)wet_so >= 3); // 3 samples available
    auto dry_samples_of_interest = std::vector<float>(dry_data.begin() + dry_so, dry_data.begin() + dry_so + 3);
    auto wet_samples_of_interest = std::vector<float>(wet_data.begin() + wet_so, wet_data.begin() + wet_so + 3);
    CHECK(dry_samples_of_interest == std::vector<float>({4, 5, 6}));
    CHECK(wet_samples_of_interest == std::vector<float>({2, 2.5, 3}));
};
