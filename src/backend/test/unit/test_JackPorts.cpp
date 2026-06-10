#include "JackAudioMidiDriver.h"
#include "MidiBuffer.h"
#include "RustAudioPort.h"
#include "shoop_globals.h"
#include "catch2/catch_approx.hpp"
#include <catch2/catch_test_macros.hpp>
#include <memory>
#include "helpers.h"
#include <memory>
#include <cstdint>

std::unique_ptr<JackTestAudioMidiDriver> open_test_driver() {
    auto driver = std::make_unique<JackTestAudioMidiDriver>();
    auto api = driver->clone_jack_api();
    backend_rust::jack_test_api_reset(*api);
    JackAudioMidiDriverSettings settings;
    settings.client_name_hint = "test";
    settings.maybe_server_name_hint = std::nullopt;
    driver->start(settings);
    return driver;
}

uintptr_t port_handle(std::shared_ptr<PortInterface> const& port) {
    return reinterpret_cast<uintptr_t>(port->maybe_driver_handle());
}

void clear_test_midi_buffer(JackAudioMidiDriver const& driver, std::shared_ptr<PortInterface> const& port) {
    auto api = driver.clone_jack_api();
    REQUIRE(backend_rust::jack_test_api_clear_midi_buffer(*api, port_handle(port)));
}

void push_test_midi_event(
    JackAudioMidiDriver const& driver,
    std::shared_ptr<PortInterface> const& port,
    MidiStorageElem const& msg) {
    auto api = driver.clone_jack_api();
    REQUIRE(backend_rust::jack_test_api_push_midi_event(*api, port_handle(port), msg.time, msg.size, msg.bytes));
}

std::vector<MidiStorageElem> get_test_midi_buffer(
    JackAudioMidiDriver const& driver,
    std::shared_ptr<PortInterface> const& port) {
    auto api = driver.clone_jack_api();
    auto n = backend_rust::jack_test_api_midi_buffer_size(*api, port_handle(port));
    std::vector<MidiStorageElem> rval;
    rval.reserve(n);
    for (size_t i = 0; i < n; ++i) {
        MidiStorageElem msg;
        uint32_t time = 0;
        uint16_t size = 0;
        REQUIRE(backend_rust::jack_test_api_get_midi_event(*api, port_handle(port), i, time, size, msg.bytes));
        msg.time = time;
        msg.size = size;
        rval.push_back(msg);
    }
    return rval;
}

TEST_CASE("Ports - Jack Audio In - Properties", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Input, nullptr);

    CHECK(port->has_internal_read_access());
    CHECK(!port->has_internal_write_access());
    CHECK(port->has_implicit_input_source());
    CHECK(!port->has_implicit_output_sink());
}

TEST_CASE("Ports - Jack Audio In - Gain", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Input, nullptr);

    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port->set_gain(0.5f);

    port->PROC_prepare(3);
    auto buf = port->PROC_get_buffer(3);
    memcpy((void*) buf, (void*) samples, 3 * sizeof(audio_sample_t));
    port->PROC_process(3);
    buf = port->PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.5f));
    CHECK(buf[2] == Catch::Approx(1.0f));
}

TEST_CASE("Ports - Jack Audio In - Mute", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Input, nullptr);

    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port->set_muted(true);

    port->PROC_prepare(3);
    auto buf = port->PROC_get_buffer(3);
    memcpy((void*) buf, (void*) samples, 3 * sizeof(audio_sample_t));
    port->PROC_process(3);
    buf = port->PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.0f));
    CHECK(buf[2] == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Jack Audio In - Peak", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Input, nullptr);

    std::vector<audio_sample_t> samples = {
        0.0f, 0.5f, 0.9f, 0.5f, 0.0f
    };

    port->PROC_prepare(5);
    auto buf = port->PROC_get_buffer(5);
    memcpy((void*) buf, (void*) samples.data(), 5 * sizeof(audio_sample_t));
    port->PROC_process(5);

    CHECK(port->get_input_peak() == Catch::Approx(0.9f));
    CHECK(port->get_output_peak() == Catch::Approx(0.9f));

    port->reset_output_peak();
    port->reset_input_peak();
    port->set_muted(true);
    port->PROC_prepare(5);
    buf = port->PROC_get_buffer(5);
    memcpy((void*) buf, (void*) samples.data(), 5 * sizeof(audio_sample_t));
    port->PROC_process(5);

    CHECK(port->get_input_peak() == Catch::Approx(0.9f));
    CHECK(port->get_output_peak() == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Jack Audio In - get ringbuffer data", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto pool = std::make_shared<shoop_types::AudioBufferPool>(10, 5, 4);
    auto port = driver->open_audio_port("test", ShoopPortDirection_Input, pool);

    // Process 4 samples
    port->PROC_prepare(4);
    auto buf = port->PROC_get_buffer(4);
    buf[0] = 0.0f;
    buf[1] = 0.1f;
    buf[2] = 0.2f;
    buf[3] = 0.3f;
    port->PROC_process(4);

    // Get the ringbuffer content
    auto s = port->PROC_get_ringbuffer_contents();
    CHECK(s.n_samples >= 4);
    CHECK(s.data->back()->at(0) == 0.0f);
    CHECK(s.data->back()->at(1) == 0.1f);
    CHECK(s.data->back()->at(2) == 0.2f);
    CHECK(s.data->back()->at(3) == 0.3f);
};

TEST_CASE("Ports - Jack Audio Out - Properties", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Output, nullptr);

    CHECK(!port->has_internal_read_access());
    CHECK(port->has_internal_write_access());
    CHECK(!port->has_implicit_input_source());
    CHECK(port->has_implicit_output_sink());
}

TEST_CASE("Ports - Jack Audio Out - Gain", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Output, nullptr);

    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port->set_gain(0.5f);

    port->PROC_prepare(3);
    auto buf = port->PROC_get_buffer(3);
    memcpy((void*) buf, (void*) samples, 3 * sizeof(audio_sample_t));
    port->PROC_process(3);
    buf = port->PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.5f));
    CHECK(buf[2] == Catch::Approx(1.0f));
}

TEST_CASE("Ports - Jack Audio Out - Mute", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Output, nullptr);

    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port->set_muted(true);

    port->PROC_prepare(3);
    auto buf = port->PROC_get_buffer(3);
    memcpy((void*) buf, (void*) samples, 3 * sizeof(audio_sample_t));
    port->PROC_process(3);
    buf = port->PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.0f));
    CHECK(buf[2] == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Jack Audio Out - Peak", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Output, nullptr);

    std::vector<audio_sample_t> samples = {
        0.0f, 0.5f, 0.9f, 0.5f, 0.0f
    };

    port->PROC_prepare(5);
    auto buf = port->PROC_get_buffer(5);
    memcpy((void*) buf, (void*) samples.data(), 5 * sizeof(audio_sample_t));
    port->PROC_process(5);

    CHECK(port->get_input_peak() == Catch::Approx(0.9f));
    CHECK(port->get_output_peak() == Catch::Approx(0.9f));

    port->reset_input_peak();
    port->reset_output_peak();
    port->set_muted(true);

    port->PROC_prepare(5);
    buf = port->PROC_get_buffer(5);
    memcpy((void*) buf, (void*) samples.data(), 5 * sizeof(audio_sample_t));
    port->PROC_process(5);

    CHECK(port->get_input_peak() == Catch::Approx(0.9f));
    CHECK(port->get_output_peak() == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Jack Audio Out - Noop Zero", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", ShoopPortDirection_Output, nullptr);

    std::vector<audio_sample_t> samples = {
        0.0f, 0.5f, 0.9f, 0.5f, 0.0f
    };

    port->PROC_prepare(5);
    auto buf = port->PROC_get_buffer(5);
    memcpy((void*) buf, (void*) samples.data(), 5 * sizeof(audio_sample_t));
    port->PROC_process(5);

    CHECK(port->get_input_peak() == Catch::Approx(0.9f));
    CHECK(port->get_output_peak() == Catch::Approx(0.9f));

    std::vector<audio_sample_t> second_round_output(5);
    port->PROC_prepare(5);
    port->PROC_process(5);
    buf = port->PROC_get_buffer(5);
    memcpy((void*) second_round_output.data(), (void*) buf, 5 * sizeof(audio_sample_t));

    CHECK(second_round_output[0] == Catch::Approx(0.0f));
    CHECK(second_round_output[1] == Catch::Approx(0.0f));
    CHECK(second_round_output[2] == Catch::Approx(0.0f));
    CHECK(second_round_output[3] == Catch::Approx(0.0f));
    CHECK(second_round_output[4] == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Jack Midi In - Properties", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);

    CHECK(port->has_internal_read_access());
    CHECK(!port->has_internal_write_access());
    CHECK(port->has_implicit_input_source());
    CHECK(!port->has_implicit_output_sink());
}

TEST_CASE("Ports - Jack Midi In - Receive", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);

    port->PROC_prepare(100);
    port->PROC_process(100);

    auto buf = port->PROC_get_read_output_data_buffer(100);
    CHECK(buf->n_events() == 0);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    push_test_midi_event(*driver, port, m1);
    push_test_midi_event(*driver, port, m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->n_events() == 2);

    auto r1 = buf->get_event(0);
    auto r2 = buf->get_event(1);
    CHECK(r1.contents_equal(m1));
    CHECK(r2.contents_equal(m2));
}

TEST_CASE("Ports - Jack Midi In - Mute", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);

    port->set_muted(true);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    push_test_midi_event(*driver, port, m1);
    push_test_midi_event(*driver, port, m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    auto buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->n_events() == 0);
}

TEST_CASE("Ports - Jack Midi In - Message Counter", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    push_test_midi_event(*driver, port, m1);
    push_test_midi_event(*driver, port, m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    auto buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->n_events() == 2);
    CHECK(buf->get_event(0).contents_equal(m1));
    CHECK(buf->get_event(1).contents_equal(m2));

    port->reset_n_input_events();
    port->reset_n_output_events();
    clear_test_midi_buffer(*driver, port);

    port->PROC_prepare(100);
    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 0);
    CHECK(port->get_n_output_events() == 0);

    port->set_muted(true);
    clear_test_midi_buffer(*driver, port);
    push_test_midi_event(*driver, port, m1);
    push_test_midi_event(*driver, port, m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    buf = port->PROC_get_read_output_data_buffer(100);
    CHECK(buf->n_events() == 0);
}

TEST_CASE("Ports - Jack Midi In - Note Tracker", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);
    using Msg = MidiStorageElem;

    auto n1_on = create_noteOn<Msg>(0, 0, 100, 127);
    auto n2_on = create_noteOn<Msg>(0, 0, 110, 127);
    auto n1_off = create_noteOff<Msg>(0, 0, 100, 127);
    auto n2_off = create_noteOff<Msg>(0, 0, 110, 127);

    CHECK(port->get_n_input_notes_active() == 0);

    clear_test_midi_buffer(*driver, port);
    push_test_midi_event(*driver, port, n1_on);
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 1);

    clear_test_midi_buffer(*driver, port);
    push_test_midi_event(*driver, port, n2_on);
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 2);

    clear_test_midi_buffer(*driver, port);
    push_test_midi_event(*driver, port, n1_on);
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 2);

    clear_test_midi_buffer(*driver, port);
    push_test_midi_event(*driver, port, n1_off);
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 1);

    clear_test_midi_buffer(*driver, port);
    push_test_midi_event(*driver, port, n1_off);
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 1);

    clear_test_midi_buffer(*driver, port);
    push_test_midi_event(*driver, port, n2_off);
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 0);
}

TEST_CASE("Ports - Jack Midi In - get ringbuffer data", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);
    using Msg = MidiStorageElem;

    std::vector<Msg> in = {
        create_noteOn<Msg>(0, 0, 100, 127),
        create_noteOn<Msg>(1, 0, 110, 127),
        create_noteOff<Msg>(2, 0, 100, 127),
        create_noteOff<Msg>(3, 0, 110, 127)
    };

    clear_test_midi_buffer(*driver, port);
    for (auto const &msg : in) {
        push_test_midi_event(*driver, port, msg);
    }
    port->PROC_prepare(100);
    port->PROC_process(100);

    auto buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->n_events() == 4);
    CHECK(buf->get_event(0).contents_equal(in[0]));
    CHECK(buf->get_event(1).contents_equal(in[1]));
    CHECK(buf->get_event(2).contents_equal(in[2]));
    CHECK(buf->get_event(3).contents_equal(in[3]));
};

TEST_CASE("Ports - Jack Midi Out - Properties", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);

    CHECK(!port->has_internal_read_access());
    CHECK(port->has_internal_write_access());
    CHECK(!port->has_implicit_input_source());
    CHECK(port->has_implicit_output_sink());
}

TEST_CASE("Ports - Jack Midi Out - Send", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);

    port->PROC_prepare(100);
    port->PROC_process(100);

    port->PROC_prepare(100);

    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    buf->write_event(m1);
    buf->write_event(m2);

    port->PROC_process(100);

    auto internal = get_test_midi_buffer(*driver, port);
    REQUIRE(internal.size() == 2);
    CHECK(internal[0].contents_equal(m1));
    CHECK(internal[1].contents_equal(m2));
}

TEST_CASE("Ports - Jack Midi Out - Sort", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);

    port->PROC_prepare(100);
    port->PROC_process(100);

    port->PROC_prepare(100);

    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiStorageElem m1; m1.time = 1; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    MidiStorageElem m3; m3.time = 10; m3.size = 3; m3.bytes[0] = 0; m3.bytes[1] = 1; m3.bytes[2] = 2;
    buf->write_event(m1);
    buf->write_event(m2);
    buf->write_event(m3);

    port->PROC_process(100);

    auto internal = get_test_midi_buffer(*driver, port);
    REQUIRE(internal.size() == 3);
    CHECK(internal[0].contents_equal(m2));
    CHECK(internal[1].contents_equal(m1));
    CHECK(internal[2].contents_equal(m3));
}

TEST_CASE("Ports - Jack Midi Out - Message Counter", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);

    port->PROC_prepare(100);
    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    buf->write_event(m1);
    buf->write_event(m2);

    port->PROC_process(100);

    auto internal = get_test_midi_buffer(*driver, port);
    REQUIRE(internal.size() == 2);
    CHECK(internal[0].contents_equal(m1));
    CHECK(internal[1].contents_equal(m2));

    port->reset_n_input_events();
    port->reset_n_output_events();

    port->PROC_prepare(100);
    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 0);
    CHECK(port->get_n_output_events() == 0);

    port->set_muted(true);
    port->PROC_prepare(100);
    buf = port->PROC_get_write_data_into_port_buffer(100);
    buf->write_event(m1);
    buf->write_event(m2);

    port->PROC_process(100);

    CHECK(get_test_midi_buffer(*driver, port).size() == 0);
}

TEST_CASE("Ports - Jack Midi Out - Note Tracker", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);
    using Msg = MidiStorageElem;

    auto n1_on = create_noteOn<Msg>(0, 0, 100, 127);
    auto n2_on = create_noteOn<Msg>(1, 0, 110, 127);
    auto n1_off = create_noteOff<Msg>(2, 0, 100, 127);
    auto n2_off = create_noteOff<Msg>(3, 0, 110, 127);

    CHECK(port->get_n_output_notes_active() == 0);

    port->PROC_prepare(10);
    auto buf = port->PROC_get_write_data_into_port_buffer(10);
    buf->write_event(n1_on);
    buf->write_event(n2_on);
    port->PROC_process(10);

    auto internal = get_test_midi_buffer(*driver, port);
    REQUIRE(internal.size() == 2);
    CHECK(internal[0].contents_equal(n1_on));
    CHECK(internal[1].contents_equal(n2_on));

    clear_test_midi_buffer(*driver, port);
    port->PROC_prepare(10);
    buf = port->PROC_get_write_data_into_port_buffer(10);
    buf->write_event(n1_off);
    buf->write_event(n2_off);
    port->PROC_process(10);

    internal = get_test_midi_buffer(*driver, port);
    REQUIRE(internal.size() == 2);
    CHECK(internal[0].contents_equal(n1_off));
    CHECK(internal[1].contents_equal(n2_off));
}

TEST_CASE("Ports - Jack Midi Out - Mute", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);

    port->set_muted(true);

    port->PROC_prepare(100);
    port->PROC_process(100);

    port->PROC_prepare(100);

    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    buf->write_event(m1);
    buf->write_event(m2);

    port->PROC_process(100);

    CHECK(get_test_midi_buffer(*driver, port).size() == 0);
}
