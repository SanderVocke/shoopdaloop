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

struct SingleMidiPortPassthroughTestChain : public ModuleLoggingEnabled<"Test.SingleMidiPortPassthroughTestChain"> {
    shoop_backend_session_t *api_backend_session;
    std::shared_ptr<BackendSession> int_backend_session;

    shoop_audio_driver_t *api_driver;
    std::shared_ptr<shoop_types::_DummyAudioMidiDriver> int_driver;

    shoopdaloop_midi_port_t *api_input_port;
    std::shared_ptr<GraphPort> int_input_port;
    DummyMidiPort* int_dummy_input_port;

    shoopdaloop_midi_port_t *api_output_port;
    std::shared_ptr<GraphPort> int_output_port;
    DummyMidiPort* int_dummy_output_port;

    SingleMidiPortPassthroughTestChain() {
        api_backend_session = create_backend_session();
        int_backend_session = internal_backend_session(api_backend_session);

        api_driver = create_audio_driver(Dummy);
        int_driver = std::dynamic_pointer_cast<_DummyAudioMidiDriver>(internal_audio_driver(api_driver));

        auto settings = DummyAudioMidiDriverSettings{};
        int_driver->start(settings);
        set_audio_driver(api_backend_session, api_driver);

        api_input_port = open_midi_port(api_backend_session, api_driver, "sys_audio_in", Input);
        api_output_port = open_midi_port(api_backend_session, api_driver, "sys_audio_out", Output);
        int_input_port = internal_midi_port(api_input_port);
        int_output_port = internal_midi_port(api_output_port);
        int_dummy_input_port = dynamic_cast<DummyMidiPort*>(&int_input_port->get_port());
        int_dummy_output_port = dynamic_cast<DummyMidiPort*>(&int_output_port->get_port());

        // Note: need to wait for channels to really appear
        int_driver->wait_process();
        int_driver->enter_mode(DummyAudioMidiDriverMode::Controlled);

        add_midi_port_passthrough(api_input_port, api_output_port);

        set_midi_port_passthroughMuted(api_input_port, 0);
        set_midi_port_muted(api_input_port, 0);
        set_midi_port_passthroughMuted(api_output_port, 0);
        set_midi_port_muted(api_output_port, 0);

        int_backend_session->wait_graph_up_to_date(); 
    }
};

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
    std::shared_ptr<AudioMidiLoop> int_audiomidi_loop;

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

        api_input_port = open_audio_port(api_backend_session, api_driver, "sys_audio_in", Input);
        api_output_port = open_audio_port(api_backend_session, api_driver, "sys_audio_out", Output);
        int_input_port = internal_audio_port(api_input_port);
        int_output_port = internal_audio_port(api_output_port);
        int_dummy_input_port = dynamic_cast<DummyAudioPort*>(&int_input_port->get_port());
        int_dummy_output_port = dynamic_cast<DummyAudioPort*>(&int_output_port->get_port());

        api_midi_input_port = open_midi_port(api_backend_session, api_driver, "sys_midi_in", Input);
        api_midi_output_port = open_midi_port(api_backend_session, api_driver, "sys_midi_out", Output);
        int_midi_input_port = internal_midi_port(api_midi_input_port);
        int_midi_output_port = internal_midi_port(api_midi_output_port);
        int_dummy_midi_input_port = dynamic_cast<DummyMidiPort*>(&int_midi_input_port->get_port());
        int_dummy_midi_output_port = dynamic_cast<DummyMidiPort*>(&int_midi_output_port->get_port());

        api_loop = create_loop(api_backend_session);
        int_loop = internal_loop(api_loop);

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

        connect_audio_input(api_audio_chan, api_input_port);
        connect_audio_output(api_audio_chan, api_output_port);
        connect_midi_input(api_midi_chan, api_midi_input_port);
        connect_midi_output(api_midi_chan, api_midi_output_port);

        add_audio_port_passthrough(api_input_port, api_output_port);
        add_midi_port_passthrough(api_midi_input_port, api_midi_output_port);

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

struct SingleDryWetLoopTestChain : public ModuleLoggingEnabled<"Test.SingleDryWetLoopTestChain"> {

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

    shoopdaloop_fx_chain_t *api_fx_chain;
    std::shared_ptr<GraphFXChain> int_fx_chain;
    std::shared_ptr<shoop_types::FXChain> int_custom_processing_chain;
    shoopdaloop_audio_port_t *api_fx_in;
    shoopdaloop_audio_port_t *api_fx_out;
    std::shared_ptr<GraphPort> int_fx_in;
    std::shared_ptr<GraphPort> int_fx_out;

    shoopdaloop_midi_port_t *api_fx_midi_in;
    std::shared_ptr<GraphPort> int_fx_midi_in;

    shoopdaloop_loop_t *api_loop;
    std::shared_ptr<GraphLoop> int_loop;
    std::shared_ptr<AudioMidiLoop> int_audiomidi_loop;

    std::shared_ptr<ObjectPool<AudioBuffer<float>>> buffer_pool;

    shoopdaloop_loop_audio_channel_t *api_dry_chan;
    shoopdaloop_loop_audio_channel_t *api_wet_chan;
    std::shared_ptr<GraphLoopChannel> int_dry_chan_node;
    std::shared_ptr<GraphLoopChannel> int_wet_chan_node;
    std::shared_ptr<shoop_types::LoopAudioChannel> int_dry_audio_chan;
    std::shared_ptr<shoop_types::LoopAudioChannel> int_wet_audio_chan;

    shoopdaloop_loop_midi_channel_t *api_dry_midi_chan;
    std::shared_ptr<GraphLoopChannel> int_dry_midi_chan_node;
    std::shared_ptr<shoop_types::LoopMidiChannel> int_dry_midi_chan;

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
        int_dummy_input_port = dynamic_cast<DummyAudioPort*>(&int_input_port->get_port());
        int_dummy_output_port = dynamic_cast<DummyAudioPort*>(&int_output_port->get_port());

        api_midi_input_port = open_midi_port(api_backend_session, api_driver, "sys_midi_in", Input);
        int_midi_input_port = internal_midi_port(api_midi_input_port);
        int_dummy_midi_input_port = dynamic_cast<DummyMidiPort*>(&int_midi_input_port->get_port());

        api_fx_chain = create_fx_chain(api_backend_session, Test2x2x1, "Test");
        int_fx_chain = internal_fx_chain(api_fx_chain);
        int_custom_processing_chain = std::dynamic_pointer_cast<shoop_types::FXChain>(int_fx_chain->chain);
        api_fx_in = fx_chain_audio_input_port(api_fx_chain, 0);
        api_fx_out = fx_chain_audio_output_port(api_fx_chain, 0);
        int_fx_in = internal_audio_port(api_fx_in);
        int_fx_out = internal_audio_port(api_fx_out);

        api_fx_midi_in = fx_chain_midi_input_port(api_fx_chain, 0);
        int_fx_midi_in = internal_midi_port(api_fx_midi_in);

        api_loop = create_loop(api_backend_session);
        int_loop = internal_loop(api_loop);

        api_dry_chan = add_audio_channel(api_loop, ChannelMode_Dry);
        api_wet_chan = add_audio_channel(api_loop, ChannelMode_Wet);
        int_dry_chan_node = internal_audio_channel(api_dry_chan);
        int_wet_chan_node = internal_audio_channel(api_wet_chan);

        api_dry_midi_chan = add_midi_channel(api_loop, ChannelMode_Dry);
        int_dry_midi_chan_node = internal_midi_channel(api_dry_midi_chan);

        // Note: need to wait for channels to really appear
        int_driver->wait_process();

        int_dry_audio_chan = std::dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_dry_chan_node->channel);
        int_wet_audio_chan = std::dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_wet_chan_node->channel);
        int_dry_midi_chan = std::dynamic_pointer_cast<shoop_types::LoopMidiChannel>(int_dry_midi_chan_node->channel);

        if(!int_dry_audio_chan) { throw std::runtime_error("ChannelMode_Dry audio channel is null"); }
        if(!int_wet_audio_chan) { throw std::runtime_error("Wet audio channel is null"); }
        if(!int_dry_midi_chan) { throw std::runtime_error("Dry MIDI channel is null"); }

        int_driver->enter_mode(DummyAudioMidiDriverMode::Controlled);

        connect_audio_input(api_dry_chan, api_input_port);
        connect_audio_output(api_dry_chan, api_fx_in);
        connect_audio_input(api_wet_chan, api_fx_out);
        connect_audio_output(api_wet_chan, api_output_port);
        connect_midi_input(api_dry_midi_chan, api_midi_input_port);

        add_audio_port_passthrough(api_input_port, api_fx_in);
        add_audio_port_passthrough(api_fx_out, api_output_port);
        add_midi_port_passthrough(api_midi_input_port, api_fx_midi_in);

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
    CHECK(tst.int_dry_audio_chan->get_data(true) == input_data);
    CHECK(tst.int_wet_audio_chan->get_data(true) == expect_data);

    CHECK(tst.int_input_port->maybe_audio_port()->get_input_peak() == Catch::Approx(8.0f));
    CHECK(tst.int_input_port->maybe_audio_port()->get_output_peak() == Catch::Approx(8.0f));
    CHECK(tst.int_fx_in->maybe_audio_port()->get_input_peak() == Catch::Approx(8.0f));
    CHECK(tst.int_fx_in->maybe_audio_port()->get_output_peak() == Catch::Approx(8.0f));

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
    CHECK(tst.int_dry_audio_chan->get_data(true) == input_data);
    CHECK(tst.int_wet_audio_chan->get_data(true) == zeroes(8));
    CHECK(result_data == zeroes(8));

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

    CHECK(tst.int_dry_audio_chan->get_data(true) == zeroes(8));
    CHECK(tst.int_wet_audio_chan->get_data(true) == zeroes(8));

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet record gain", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, 0, 0);
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
    CHECK(result_data == wet_data);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet playback gain", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4}), half_wet({0.5, 1, 1.5, 2});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, false);
    set_audio_port_gain(tst.api_output_port, 0.5);

    loop_transition(tst.api_loop, LoopMode_Playing, 0, 0);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    CHECK(result_data == half_wet);

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
    CHECK(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet dry playback gain", "[Chains][audio]") {
    SingleDryWetLoopTestChain tst;

    std::vector<float> dry_data({4, 3, 2, 1}), wet_data({1, 2, 3, 4}), half_dry({2, 1.5, 1, 0.5});
    auto api_dry_data = to_api_data(dry_data);
    auto api_wet_data = to_api_data(wet_data);
    load_audio_channel_data(tst.api_wet_chan, &api_wet_data);
    load_audio_channel_data(tst.api_dry_chan, &api_dry_data);
    tst.int_loop->loop->set_length(4, true);

    set_audio_port_gain(tst.api_output_port, 0.5);
    loop_transition(tst.api_loop, LoopMode_PlayingDryThroughWet, 0, 0);
    
    tst.int_driver->controlled_mode_request_samples(4);
    tst.int_dummy_output_port->request_data(4);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->dequeue_data(4);
    auto expected = half_dry;
    for (auto &v: expected) { v /= 2.0f; }
    CHECK(result_data == expected);

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
    CHECK(result_data == expected);

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
    CHECK(result_data == expected);

    tst.int_driver->close();
};

TEST_CASE("Chains - Midi port passthrough", "[Chains][midi]") {
    SingleMidiPortPassthroughTestChain tst;

    std::vector<Msg> msgs = {
        create_noteOn<Msg>(0, 1, 10, 10),
        create_noteOff<Msg>(0, 10, 10, 20),
        create_noteOn<Msg>(20, 2, 1, 1)
    };

    queue_midi_msgs(tst.api_input_port, msgs);

    tst.int_dummy_output_port->request_data(50);
    tst.int_driver->controlled_mode_request_samples(50);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->get_written_requested_msgs();
    REQUIRE(result_data.size() == 3);
    CHECK_MSGS_EQUAL(result_data[0], msgs[0]);
    CHECK_MSGS_EQUAL(result_data[1], msgs[1]);
    CHECK_MSGS_EQUAL(result_data[2], msgs[2]);

    tst.int_driver->close();
};

TEST_CASE("Chains - Midi port passthrough - passthrough muted", "[Chains][midi]") {
    SingleMidiPortPassthroughTestChain tst;

    std::vector<Msg> msgs = {
        create_noteOn<Msg>(0, 1, 10, 10),
        create_noteOff<Msg>(0, 10, 10, 20),
        create_noteOn<Msg>(20, 2, 1, 1)
    };

    set_midi_port_passthroughMuted(tst.api_input_port, 1);
    queue_midi_msgs(tst.api_input_port, msgs);

    tst.int_dummy_output_port->request_data(50);
    tst.int_driver->controlled_mode_request_samples(50);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->get_written_requested_msgs();
    CHECK(result_data.size() == 0);

    tst.int_driver->close();
};

TEST_CASE("Chains - Midi port passthrough - output muted", "[Chains][midi]") {
    SingleMidiPortPassthroughTestChain tst;

    std::vector<Msg> msgs = {
        create_noteOn<Msg>(0, 1, 10, 10),
        create_noteOff<Msg>(0, 10, 10, 20),
        create_noteOn<Msg>(20, 2, 1, 1)
    };

    set_midi_port_muted(tst.api_output_port, 1);
    queue_midi_msgs(tst.api_input_port, msgs);

    tst.int_dummy_output_port->request_data(50);
    tst.int_driver->controlled_mode_request_samples(50);
    tst.int_driver->controlled_mode_run_request();

    auto result_data = tst.int_dummy_output_port->get_written_requested_msgs();
    CHECK(result_data.size() == 0);

    tst.int_driver->close();
};

TEST_CASE("Chains - DryWet record MIDI basic", "[Chains][midi]") {
    SingleDryWetLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, 0, 0);
    
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

TEST_CASE("Chains - Direct playback MIDI basic", "[Chains][midi]") {
    SingleDirectLoopTestChain tst;

    loop_transition(tst.api_loop, LoopMode_Recording, 0, 0);
    
    std::vector<shoop_types::_MidiMessage> msgs = {
        create_noteOn<shoop_types::_MidiMessage>(0, 1, 10, 10),
        create_noteOff<shoop_types::_MidiMessage>(0, 10, 10, 20),
        create_noteOn<shoop_types::_MidiMessage>(20, 2, 1, 1)
    };

    auto sequence = convert_midi_msgs_to_api(msgs);
    load_midi_channel_data(tst.api_midi_chan, sequence);
    destroy_midi_sequence(sequence);
    set_loop_length(tst.api_loop, 30);
    loop_transition(tst.api_loop, LoopMode_Playing, 0, false);

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