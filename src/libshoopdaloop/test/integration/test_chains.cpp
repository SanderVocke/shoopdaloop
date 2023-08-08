#include <boost/ut.hpp>
#include "DummyAudioSystem.h"
#include "CustomProcessingChain.h"
#include "AudioMidiLoop.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "ConnectedPort.h"
#include "types.h"
#include <memory>
#include "libshoopdaloop_test_if.h"
#include "libshoopdaloop.h"
#include "shoop_globals.h"
#include <iostream>
#include "LoggingEnabled.h"

using namespace boost::ut;

struct SingleDryWetLoopTestChain : public ModuleLoggingEnabled {
    std::string log_module_name() const override {
        return "Test.SingleDryWetLoopTestChain";
    }

    shoopdaloop_backend_instance_t *api_backend;
    std::shared_ptr<Backend> int_backend;
    shoop_types::_DummyAudioSystem *int_dummy_audio_system;

    shoopdaloop_audio_port_t *api_input_port;
    std::shared_ptr<ConnectedPort> int_input_port;
    std::shared_ptr<DummyAudioPort> int_dummy_input_port;

    shoopdaloop_audio_port_t *api_output_port;
    std::shared_ptr<ConnectedPort> int_output_port;
    std::shared_ptr<DummyAudioPort> int_dummy_output_port;
    std::vector<float> dummy_output_port_dequeued_data;

    shoopdaloop_midi_port_t *api_midi_input_port;
    std::shared_ptr<ConnectedPort> int_midi_input_port;
    std::shared_ptr<DummyMidiPort> int_dummy_midi_input_port;

    shoopdaloop_fx_chain_t *api_fx_chain;
    std::shared_ptr<ConnectedFXChain> int_fx_chain;
    std::shared_ptr<shoop_types::FXChain> int_custom_processing_chain;
    shoopdaloop_audio_port_t *api_fx_in;
    shoopdaloop_audio_port_t *api_fx_out;
    std::shared_ptr<ConnectedPort> int_fx_in;
    std::shared_ptr<ConnectedPort> int_fx_out;

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
        log_init();
        unsigned int dummy;

        api_backend = initialize(Dummy, "backend");
        int_backend = internal_backend(api_backend);
        int_dummy_audio_system = (shoop_types::_DummyAudioSystem*)int_backend->audio_system.get();

        api_input_port = open_audio_port(api_backend, "input", Input);
        api_output_port = open_audio_port(api_backend, "output", Output);
        int_input_port = internal_audio_port(api_input_port);
        int_output_port = internal_audio_port(api_output_port);
        int_dummy_input_port = std::dynamic_pointer_cast<DummyAudioPort>(int_input_port->port);
        int_dummy_output_port = std::dynamic_pointer_cast<DummyAudioPort>(int_output_port->port);

        api_fx_chain = create_fx_chain(api_backend, Test2x2x1, "Test");
        int_fx_chain = internal_fx_chain(api_fx_chain);
        int_custom_processing_chain = std::dynamic_pointer_cast<shoop_types::FXChain>(int_fx_chain->chain);
        api_fx_in = fx_chain_audio_input_ports(api_fx_chain, &dummy)[0];
        api_fx_out = fx_chain_audio_output_ports(api_fx_chain, &dummy)[0];
        int_fx_in = internal_audio_port(api_fx_in);
        int_fx_out = internal_audio_port(api_fx_out);

        api_loop = create_loop(api_backend);
        int_loop = internal_loop(api_loop);

        api_dry_chan = add_audio_channel(api_loop, Dry);
        api_wet_chan = add_audio_channel(api_loop, Dry);
        int_dry_chan = internal_audio_channel(api_dry_chan);
        int_wet_chan = internal_audio_channel(api_wet_chan);
        int_dry_audio_chan = std::dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_dry_chan->channel);
        int_wet_audio_chan = std::dynamic_pointer_cast<shoop_types::LoopAudioChannel>(int_wet_chan->channel);

        int_dummy_audio_system->enter_mode(DummyAudioSystemMode::Controlled);
        int_dummy_audio_system->install_post_process_handler([&](size_t n, size_t to_process) {
            if (to_process > 0) {
                size_t to_dequeue = std::min(to_process, n);
                log<logging::LogLevel::debug>("Dequeueing {} samples", to_dequeue);
                auto buf = int_output_port->maybe_audio_buffer;
                dummy_output_port_dequeued_data.insert(dummy_output_port_dequeued_data.end(),
                            buf, buf + to_dequeue);
            }
        });

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

suite chains_tests = []() {
    // Regression test scenario. Build a chain as it would be in a
    // track with FX: input -> FX -> output with a loop that has dry and wet channels.
    "ch_1_passthrough_flow"_test = []() {
        SingleDryWetLoopTestChain tst;
        
        std::vector<float> input_data({1, 2, 3, 4, 5, 6, 7, 8});
        std::vector<float> input_data2({-8, -7, -6, -5, -4, -3 , -2, -1});
        tst.int_dummy_input_port->queue_data(8, input_data.data());
        tst.int_dummy_input_port->queue_data(8, input_data2.data());

        tst.int_dummy_audio_system->controlled_mode_request_samples(4);
        tst.int_dummy_audio_system->controlled_mode_run_request();

        tst.int_dummy_audio_system->pause();

        expect(eq(tst.dummy_output_port_dequeued_data.size(), 4));
        expect(eq(tst.int_dummy_input_port->get_queue_empty(), false));
        auto expect_output_1 = std::vector<float>(input_data.begin(), input_data.begin() + 4);
        expect(eq(tst.dummy_output_port_dequeued_data, expect_output_1));
        tst.dummy_output_port_dequeued_data.clear();

        tst.int_dummy_audio_system->resume();

        tst.int_dummy_audio_system->controlled_mode_request_samples(12);
        tst.int_dummy_audio_system->controlled_mode_run_request();

        tst.int_dummy_audio_system->pause();

        expect(eq(tst.dummy_output_port_dequeued_data.size(), 12));
        expect(eq(tst.int_dummy_input_port->get_queue_empty(), true));
        auto expect_output_2 = std::vector<float>(input_data.begin() + 4, input_data.end());
        expect_output_2.insert(expect_output_2.end(), input_data2.begin(), input_data2.end());
        expect(eq(tst.dummy_output_port_dequeued_data, expect_output_2));

        tst.int_dummy_audio_system->close();
    };
};