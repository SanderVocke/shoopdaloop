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

        api_input_port = open_driver_midi_port(api_backend_session, api_driver, "sys_audio_in", ShoopPortDirection_Input, 0);
        api_output_port = open_driver_midi_port(api_backend_session, api_driver, "sys_audio_out", ShoopPortDirection_Output, 0);
        int_input_port = internal_midi_port(api_input_port);
        int_output_port = internal_midi_port(api_output_port);
        int_dummy_input_port = dynamic_cast<DummyMidiPort*>(&int_input_port->get_port());
        int_dummy_output_port = dynamic_cast<DummyMidiPort*>(&int_output_port->get_port());

        // Note: need to wait for channels to really appear
        int_driver->wait_process();
        int_driver->enter_mode(DummyAudioMidiDriverMode::Controlled);

        connect_midi_port_internal(api_input_port, api_output_port);

        set_midi_port_passthroughMuted(api_input_port, 0);
        set_midi_port_muted(api_input_port, 0);
        set_midi_port_passthroughMuted(api_output_port, 0);
        set_midi_port_muted(api_output_port, 0);

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

TEST_CASE("Chain - Midi port passthrough", "[chain][midi]") {
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

TEST_CASE("Chain - Midi port passthrough - passthrough muted", "[chain][midi]") {
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

TEST_CASE("Chain - Midi port passthrough - output muted", "[chain][midi]") {
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