#include <catch2/catch_test_macros.hpp>
#include "DummyAudioMidiDriver.h"
#include "CustomProcessingChain.h"
#include "AudioMidiLoop.h"
#include "InternalAudioPort.h"
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

struct PassthroughMixingTestChain : public ModuleLoggingEnabled<"Test.PassthroughMixingTestChain"> {

    shoop_backend_session_t *api_backend_session;
    shoop_shared_ptr<BackendSession> int_backend_session;

    shoop_audio_driver_t *api_driver;
    shoop_shared_ptr<shoop_types::_DummyAudioMidiDriver> int_driver;

    shoopdaloop_audio_port_t *api_input_port_1;
    shoop_shared_ptr<GraphPort> int_input_port_1;
    DummyAudioPort* int_dummy_input_port_1;

    shoopdaloop_audio_port_t *api_input_port_2;
    shoop_shared_ptr<GraphPort> int_input_port_2;
    DummyAudioPort* int_dummy_input_port_2;

    shoopdaloop_audio_port_t *api_output_port;
    shoop_shared_ptr<GraphPort> int_output_port;
    DummyAudioPort* int_dummy_output_port;

    shoopdaloop_audio_port_t *api_mixing_port;
    shoop_shared_ptr<GraphPort> int_mixing_port;
    InternalAudioPort<float>* int_internal_mixing_port;

    // TODO add MIDI

    PassthroughMixingTestChain() {
        api_backend_session = create_backend_session();
        int_backend_session = internal_backend_session(api_backend_session);

        api_driver = create_audio_driver(Dummy);
        int_driver = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(internal_audio_driver(api_driver));

        auto settings = DummyAudioMidiDriverSettings{};
        int_driver->start(settings);
        set_audio_driver(api_backend_session, api_driver);

        api_input_port_1 = open_driver_audio_port(api_backend_session, api_driver, "sys_audio_in_1", ShoopPortDirection_Input, 0);
        int_input_port_1 = internal_audio_port(api_input_port_1);
        int_dummy_input_port_1 = dynamic_cast<DummyAudioPort*>(&int_input_port_1->get_port());

        api_input_port_2 = open_driver_audio_port(api_backend_session, api_driver, "sys_audio_in_2", ShoopPortDirection_Input, 0);
        int_input_port_2 = internal_audio_port(api_input_port_2);
        int_dummy_input_port_2 = dynamic_cast<DummyAudioPort*>(&int_input_port_2->get_port());

        api_mixing_port = open_internal_audio_port(api_backend_session, "audio_mix", 0);
        int_mixing_port = internal_audio_port(api_mixing_port);
        int_internal_mixing_port = dynamic_cast<InternalAudioPort<float>*>(&int_mixing_port->get_port());
        
        api_output_port = open_driver_audio_port(api_backend_session, api_driver, "sys_audio_out", ShoopPortDirection_Output, 0);
        int_output_port = internal_audio_port(api_output_port);
        int_dummy_output_port = dynamic_cast<DummyAudioPort*>(&int_output_port->get_port());

        // Note: need to wait for channels to really appear
        int_driver->wait_process();
        int_driver->enter_mode(DummyAudioMidiDriverMode::Controlled);

        connect_audio_port_internal(api_input_port_1, api_mixing_port);
        connect_audio_port_internal(api_input_port_2, api_mixing_port);
        connect_audio_port_internal(api_mixing_port, api_output_port);

        set_audio_port_passthroughMuted(api_input_port_1, 0);
        set_audio_port_muted(api_input_port_1, 0);
        set_audio_port_gain(api_input_port_1, 1.0f);

        set_audio_port_passthroughMuted(api_input_port_2, 0);
        set_audio_port_muted(api_input_port_2, 0);
        set_audio_port_gain(api_input_port_2, 1.0f);

        set_audio_port_passthroughMuted(api_mixing_port, 0);
        set_audio_port_muted(api_mixing_port, 0);
        set_audio_port_gain(api_mixing_port, 1.0f);

        set_audio_port_passthroughMuted(api_output_port, 0);
        set_audio_port_muted(api_output_port, 0);
        set_audio_port_gain(api_output_port, 1.0f);

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

TEST_CASE("Chain - Audio mixing", "[chain][midi]") {
    PassthroughMixingTestChain tst;
    
    std::vector<float> input_data1({1,   2,   3,   4,   5,   6,   7,   8  });
    std::vector<float> input_data2({0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8});
    std::vector<float> expect_out ({1.1, 2.2, 3.3, 4.4, 5.5, 6.6, 7.7, 8.8});
    tst.int_dummy_input_port_1->queue_data(8, input_data1.data());
    tst.int_dummy_input_port_2->queue_data(8, input_data2.data());

    tst.int_driver->controlled_mode_request_samples(8);
    tst.int_dummy_output_port->request_data(8);
    tst.int_driver->controlled_mode_run_request();

    tst.int_driver->pause();

    auto result_data = tst.int_dummy_output_port->dequeue_data(8);
    CHECK(result_data.size() == 8);
    CHECK(result_data == expect_out);

    tst.int_driver->close();
};