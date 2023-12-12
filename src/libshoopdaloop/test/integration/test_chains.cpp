#include <catch2/catch_test_macros.hpp>
#include "DummyAudioMidiDriver.h"
#include "CustomProcessingChain.h"
#include "AudioMidiLoop.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "GraphPort.h"
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
    std::shared_ptr<BackendSession> int_backend_session;

    shoop_audio_driver_t *api_driver;
    std::shared_ptr<shoop_types::_DummyAudioMidiDriver> int_driver;

    shoopdaloop_audio_port_t *api_input_port;
    std::shared_ptr<GraphPort> int_input_port;
    std::shared_ptr<DummyAudioPort> int_dummy_input_port;

    shoopdaloop_audio_port_t *api_output_port;
    std::shared_ptr<GraphPort> int_output_port;
    std::shared_ptr<DummyAudioPort> int_dummy_output_port;

    shoopdaloop_midi_port_t *api_midi_input_port;
    std::shared_ptr<GraphPort> int_midi_input_port;
    std::shared_ptr<DummyMidiPort> int_dummy_midi_input_port;

    shoopdaloop_fx_chain_t *api_fx_chain;
    std::shared_ptr<ConnectedFXChain> int_fx_chain;
    std::shared_ptr<shoop_types::FXChain> int_custom_processing_chain;
    shoopdaloop_audio_port_t *api_fx_in;
    shoopdaloop_audio_port_t *api_fx_out;
    std::shared_ptr<GraphPort> int_fx_in;
    std::shared_ptr<GraphPort> int_fx_out;

    shoopdaloop_loop_t *api_loop;
    std::shared_ptr<ConnectedLoop> int_loop;
    std::shared_ptr<AudioMidiLoop> int_audiomidi_loop;

    std::shared_ptr<ObjectPool<AudioBuffer<float>>> buffer_pool;

    shoopdaloop_loop_audio_channel_t *api_dry_chan;
    shoopdaloop_loop_audio_channel_t *api_wet_chan;
    std::shared_ptr<ConnectedChannel> int_dry_chan;
    std::shared_ptr<ConnectedChannel> int_wet_chan;
    std::shared_ptr<shoop_types::LoopAudioChannel> int_dry_audio_chan;
    std::shared_ptr<shoop_types::LoopAudioChannel> int_wet_audio_chan;

    SingleDryWetLoopTestChain() {
        api_backend_session = create_backend_session();
        int_backend_session = internal_backend_session(api_backend_session);

        api_driver = create_audio_driver(Dummy);
        int_driver = std::dynamic_pointer_cast<_DummyAudioMidiDriver>(internal_audio_driver(api_driver));

        auto settings = DummyAudioMidiDriverSettings{};
        int_driver->start(settings);
        set_audio_driver(api_backend_session, api_driver);

        api_input_port = open_audio_port(api_backend_session, api_driver, "sys_audio_in", Input);
        api_output_port = open_audio_port(api_backend_session, api_driver, "sys_audio_out", Output);
        int_input_port = internal_audio_port(api_input_port);
        int_output_port = internal_audio_port(api_output_port);
        int_dummy_input_port = std::dynamic_pointer_cast<DummyAudioPort>(int_input_port->port);
        int_dummy_output_port = std::dynamic_pointer_cast<DummyAudioPort>(int_output_port->port);

        api_fx_chain = create_fx_chain(api_backend_session, Test2x2x1, "Test");
        int_fx_chain = internal_fx_chain(api_fx_chain);
        int_custom_processing_chain = std::dynamic_pointer_cast<shoop_types::FXChain>(int_fx_chain->chain);
        api_fx_in = fx_chain_audio_input_port(api_fx_chain, 0);
        api_fx_out = fx_chain_audio_output_port(api_fx_chain, 0);
        int_fx_in = internal_audio_port(api_fx_in);
        int_fx_out = internal_audio_port(api_fx_out);

        api_loop = create_loop(api_backend_session);
        int_loop = internal_loop(api_loop);

        api_dry_chan = add_audio_channel(api_loop, ChannelMode_Dry);
        api_wet_chan = add_audio_channel(api_loop, ChannelMode_Wet);
        int_dry_chan = internal_audio_channel(api_dry_chan);
        int_wet_chan = internal_audio_channel(api_wet_chan);

        // Note: need to wait for channels to really appear
        int_driver->wait_process();

        int_dry_audio_chan = std::dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_dry_chan->channel);
        int_wet_audio_chan = std::dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_wet_chan->channel);

        if(!int_dry_audio_chan) { throw std::runtime_error("ChannelMode_Dry audio channel is null"); }
        if(!int_wet_audio_chan) { throw std::runtime_error("Wet audio channel is null"); }

        int_driver->enter_mode(DummyAudioMidiDriverMode::Controlled);

        connect_audio_input(api_dry_chan, api_input_port);
        connect_audio_output(api_dry_chan, api_fx_in);
        connect_audio_input(api_wet_chan, api_fx_out);
        connect_audio_output(api_wet_chan, api_output_port);

        add_audio_port_passthrough(api_input_port, api_fx_in);
        add_audio_port_passthrough(api_fx_out, api_output_port);

        set_audio_port_passthroughMuted(api_input_port, 0);
        set_audio_port_muted(api_input_port, 0);
        set_audio_port_volume(api_input_port, 1.0f);
        set_audio_port_passthroughMuted(api_output_port, 0);
        set_audio_port_muted(api_output_port, 0);
        set_audio_port_volume(api_fx_in, 1.0f);
        set_audio_port_passthroughMuted(api_fx_in, 0);
        set_audio_port_muted(api_fx_in, 0);
        set_audio_port_volume(api_fx_out, 1.0f);
        set_audio_port_passthroughMuted(api_fx_out, 0);
        set_audio_port_muted(api_fx_out, 0);
        set_audio_port_volume(api_output_port, 1.0f);
        set_audio_channel_volume(api_dry_chan, 1.0f);
        set_audio_channel_volume(api_wet_chan, 1.0f);        
    }
};

std::vector<float> zeroes(uint32_t n) { return std::vector<float>(n, 0); }

shoop_audio_channel_data_t to_api_data(std::vector<float> &vec) {
    shoop_audio_channel_data_t rval;
    rval.data = vec.data();
    rval.n_samples = vec.size();
    return rval;
}

TEST_CASE("Chains - DryWet Basic", "[Chains][audio]") {
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
    REQUIRE(result_data.size() == 4);
    REQUIRE(tst.int_dummy_input_port->get_queue_empty() == false);
    auto expect_output_1 = std::vector<float>(input_data.begin(), input_data.begin() + 4);
    for (auto &v: expect_output_1) { v /= 2.0f; }
    REQUIRE(result_data == expect_output_1);
    result_data.clear();

    tst.int_driver->resume();

    tst.int_driver->controlled_mode_request_samples(12);
    tst.int_dummy_output_port->request_data(12);
    tst.int_driver->controlled_mode_run_request();

    tst.int_driver->pause();

    result_data = tst.int_dummy_output_port->dequeue_data(12);
    REQUIRE(result_data.size() == 12);
    REQUIRE(tst.int_dummy_input_port->get_queue_empty() == true);
    auto expect_output_2 = std::vector<float>(input_data.begin() + 4, input_data.end());
    expect_output_2.insert(expect_output_2.end(), input_data2.begin(), input_data2.end());
    for (auto &v: expect_output_2) { v /= 2.0f; }
    REQUIRE(result_data == expect_output_2);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet record basic", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, 0, 0);
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    auto expect_data = input_data;
    for (auto &v: expect_data) { v /= 2.0f; }
    REQUIRE(tst.int_dry_audio_chan->get_data(true) == input_data);
    REQUIRE(tst.int_wet_audio_chan->get_data(true) == expect_data);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet record passthrough muted", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, 0, 0);
    set_audio_port_passthroughMuted(tst.api_input_port, 1);
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(8);
    REQUIRE(tst.int_dry_audio_chan->get_data(true) == input_data);
    REQUIRE(tst.int_wet_audio_chan->get_data(true) == zeroes(8));
    REQUIRE(result_data == zeroes(8));

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet record muted", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, 0, 0);
    set_audio_port_muted(tst.api_input_port, 1);
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    REQUIRE(tst.int_dry_audio_chan->get_data(true) == zeroes(8));
    REQUIRE(tst.int_wet_audio_chan->get_data(true) == zeroes(8));

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet record volume", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, 0, 0);
    set_audio_port_volume(tst.api_input_port, 0.5);
    
    std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
    std::vector<float> expected({0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4});
    auto expected_wet = expected;
    for (auto &v: expected_wet) { v /= 2.0f; }
    tst.int_dummy_input_port->queue_data(8, input_data.data());
    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    REQUIRE(tst.int_dry_audio_chan->get_data(true) == expected);
    REQUIRE(tst.int_wet_audio_chan->get_data(true) == expected_wet);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet playback basic", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, false);

    loop_transition(tst.api_loop, LoopMode_Playing, 0, 0);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    REQUIRE(result_data == wet_data);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet playback volume", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4}), half_wet({0.5, 1, 1.5, 2});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, false);
    set_audio_port_volume(tst.api_output_port, 0.5);

    loop_transition(tst.api_loop, LoopMode_Playing, 0, 0);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    REQUIRE(result_data == half_wet);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet dry playback basic", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, false);
    
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, 0, 0);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = dry_data;
    for (auto &v: expected) { v /= 2.0f; }
    REQUIRE(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet dry playback volume", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4}), half_dry({2, 1.5, 1, 0.5});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, true);

    set_audio_port_volume(tst.api_output_port, 0.5);
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, 0, 0);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = half_dry;
    for (auto &v: expected) { v /= 2.0f; }
    REQUIRE(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet dry playback input passthrough muted", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, true);

    set_audio_port_passthroughMuted(tst.api_input_port, 1);
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, 0, 0);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = dry_data;
    for (auto &v: expected) { v /= 2.0f; }
    REQUIRE(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet dry playback input muted", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, true);

    set_audio_port_muted(tst.api_input_port, 1);
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, 0, 0);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = dry_data;
    for (auto &v: expected) { v /= 2.0f; }
    REQUIRE(result_data == expected);

    tst.int_driver->close();
};