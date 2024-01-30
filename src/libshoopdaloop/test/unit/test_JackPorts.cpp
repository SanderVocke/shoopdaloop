#include "JackAudioMidiDriver.h"
#include "MidiMessage.h"
#include "catch2/catch_approx.hpp"
#include <catch2/catch_test_macros.hpp>
#include <jack/midiport.h>
#include <memory>
#include "helpers.h"

std::unique_ptr<JackTestAudioMidiDriver> open_test_driver() {
    JackTestApi::internal_reset_api();
    auto driver = std::make_unique<JackTestAudioMidiDriver>();
    JackAudioMidiDriverSettings settings;
    settings.client_name_hint = "test";
    settings.maybe_server_name_hint = std::nullopt;
    driver->start(settings);
    return driver;
}

MidiMessage<uint32_t, uint32_t> to_msg(jack_midi_event_t &m) {
    MidiMessage<uint32_t, uint32_t> msg;
    msg.time = m.time;
    msg.size = m.size;
    msg.data.resize(m.size);
    memcpy((void*) msg.data.data(), (void*) m.buffer, m.size);
    return msg;
}

TEST_CASE("Ports - Jack Audio In - Properties", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", Input);

    CHECK(port->has_internal_read_access());
    CHECK(!port->has_internal_write_access());
    CHECK(port->has_implicit_input_source());
    CHECK(!port->has_implicit_output_sink());
}

TEST_CASE("Ports - Jack Audio In - Gain", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", Input);

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
    auto port = driver->open_audio_port("test", Input);

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
    auto port = driver->open_audio_port("test", Input);

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

TEST_CASE("Ports - Jack Audio Out - Properties", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", Output);

    CHECK(!port->has_internal_read_access());
    CHECK(port->has_internal_write_access());
    CHECK(!port->has_implicit_input_source());
    CHECK(port->has_implicit_output_sink());
}

TEST_CASE("Ports - Jack Audio Out - Gain", "[JackPorts][ports][audio]") {
    auto driver = open_test_driver();
    auto port = driver->open_audio_port("test", Output);

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
    auto port = driver->open_audio_port("test", Output);

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
    auto port = driver->open_audio_port("test", Output);

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
    auto port = driver->open_audio_port("test", Output);

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
    auto port = driver->open_midi_port("test", Input);

    CHECK(port->has_internal_read_access());
    CHECK(!port->has_internal_write_access());
    CHECK(port->has_implicit_input_source());
    CHECK(!port->has_implicit_output_sink());
}

TEST_CASE("Ports - Jack Midi In - Receive", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Input);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->PROC_prepare(100);
    port->PROC_process(100);

    auto buf = port->PROC_get_read_output_data_buffer(100);
    CHECK(buf->PROC_get_n_events() == 0);

    MidiMessage<uint32_t, uint32_t> m1(0, 3, {0, 1, 2});
    MidiMessage<uint32_t, uint32_t> m2(0, 3, {0, 1, 2});
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->PROC_get_n_events() == 2);

    auto &r1 = buf->PROC_get_event_reference(0);
    auto &r2 = buf->PROC_get_event_reference(1);
    CHECK(r1.contents_equal(m1));
    CHECK(r2.contents_equal(m2));
}

TEST_CASE("Ports - Jack Midi In - Mute", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Input);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->set_muted(true);

    MidiMessage<uint32_t, uint32_t> m1(0, 3, {0, 1, 2});
    MidiMessage<uint32_t, uint32_t> m2(0, 3, {0, 1, 2});
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    auto buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->PROC_get_n_events() == 0);
}

TEST_CASE("Ports - Jack Midi In - Message Counter", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Input);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    MidiMessage<uint32_t, uint32_t> m1(0, 3, {0, 1, 2});
    MidiMessage<uint32_t, uint32_t> m2(0, 3, {0, 1, 2});
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 2);
    CHECK(port->get_n_output_events() == 2);

    internal_port.midi_buffer.clear();
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 6);
    CHECK(port->get_n_output_events() == 6);

    port->reset_n_input_events();
    port->reset_n_output_events();
    internal_port.midi_buffer.clear();

    port->PROC_prepare(100);
    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 0);
    CHECK(port->get_n_output_events() == 0);

    port->set_muted(true);
    internal_port.midi_buffer.clear();
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 2);
    CHECK(port->get_n_output_events() == 0);
}

TEST_CASE("Ports - Jack Midi In - Note Tracker", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Input);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());
    using Msg = MidiMessage<uint32_t, uint32_t>;

    auto n1_on = create_noteOn<Msg>(0, 0, 100, 127);
    auto n2_on = create_noteOn<Msg>(0, 0, 110, 127);
    auto n1_off = create_noteOff<Msg>(0, 0, 100, 127);
    auto n2_off = create_noteOff<Msg>(0, 0, 110, 127);

    CHECK(port->get_n_input_notes_active() == 0);

    internal_port.midi_buffer = {n1_on};
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 1);

    internal_port.midi_buffer = {n2_on};
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 2);

    internal_port.midi_buffer = {n1_on}; // duplicate
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 2);

    internal_port.midi_buffer = {n1_off};
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 1);

    internal_port.midi_buffer = {n1_off}; // duplicate
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 1);

    internal_port.midi_buffer = {n2_off};
    port->PROC_prepare(1);
    port->PROC_process(1);
    CHECK(port->get_n_input_notes_active() == 0);
}

TEST_CASE("Ports - Jack Midi Out - Properties", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Output);

    CHECK(!port->has_internal_read_access());
    CHECK(port->has_internal_write_access());
    CHECK(!port->has_implicit_input_source());
    CHECK(port->has_implicit_output_sink());
}

TEST_CASE("Ports - Jack Midi Out - Send", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->PROC_prepare(100);
    port->PROC_process(100);

    port->PROC_prepare(100);

    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiMessage<uint32_t, uint32_t> m1(0, 3, {0, 1, 2});
    MidiMessage<uint32_t, uint32_t> m2(0, 3, {0, 1, 2});
    buf->PROC_write_event_reference(m1);
    buf->PROC_write_event_reference(m2);
    
    port->PROC_process(100);

    REQUIRE(internal_port.midi_buffer.size() == 2);
    CHECK(internal_port.midi_buffer[0].contents_equal(m1));
    CHECK(internal_port.midi_buffer[1].contents_equal(m2));
}

TEST_CASE("Ports - Jack Midi Out - Sort", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->PROC_prepare(100);
    port->PROC_process(100);

    port->PROC_prepare(100);

    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiMessage<uint32_t, uint32_t> m1(1, 3, {0, 1, 2});
    MidiMessage<uint32_t, uint32_t> m2(0, 3, {0, 1, 2});
    MidiMessage<uint32_t, uint32_t> m3(10, 3, {0, 1, 2});
    buf->PROC_write_event_reference(m1);
    buf->PROC_write_event_reference(m2);
    buf->PROC_write_event_reference(m3);
    
    port->PROC_process(100);

    REQUIRE(internal_port.midi_buffer.size() == 3);
    CHECK(internal_port.midi_buffer[0].contents_equal(m2));
    CHECK(internal_port.midi_buffer[1].contents_equal(m1));
    CHECK(internal_port.midi_buffer[2].contents_equal(m3));
}

TEST_CASE("Ports - Jack Midi Out - Message Counter", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->PROC_prepare(100);
    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiMessage<uint32_t, uint32_t> m1(0, 3, {0, 1, 2});
    MidiMessage<uint32_t, uint32_t> m2(0, 3, {0, 1, 2});
    buf->PROC_write_event_reference(m1);
    buf->PROC_write_event_reference(m2);

    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 2);
    CHECK(port->get_n_output_events() == 2);

    port->PROC_prepare(100);
    buf = port->PROC_get_write_data_into_port_buffer(100);
    buf->PROC_write_event_reference(m1);
    buf->PROC_write_event_reference(m2);
    buf->PROC_write_event_reference(m1);
    buf->PROC_write_event_reference(m2);

    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 6);
    CHECK(port->get_n_output_events() == 6);

    port->reset_n_input_events();
    port->reset_n_output_events();

    port->PROC_prepare(100);
    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 0);
    CHECK(port->get_n_output_events() == 0);

    port->set_muted(true);
    port->PROC_prepare(100);
    buf = port->PROC_get_write_data_into_port_buffer(100);
    buf->PROC_write_event_reference(m1);
    buf->PROC_write_event_reference(m2);

    port->PROC_process(100);

    CHECK(port->get_n_input_events() == 2);
    CHECK(port->get_n_output_events() == 0);
}

TEST_CASE("Ports - Jack Midi Out - Note Tracker", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    auto n1_on = create_noteOn<Msg>(0, 0, 100, 127);
    auto n2_on = create_noteOn<Msg>(0, 0, 110, 127);
    auto n1_off = create_noteOff<Msg>(0, 0, 100, 127);
    auto n2_off = create_noteOff<Msg>(0, 0, 110, 127);

    CHECK(port->get_n_output_notes_active() == 0);

    port->PROC_prepare(1);
    auto buf = port->PROC_get_write_data_into_port_buffer(1);
    buf->PROC_write_event_reference(n1_on);
    port->PROC_process(1);

    CHECK(port->get_n_output_notes_active() == 1);

    port->PROC_prepare(1);
    buf = port->PROC_get_write_data_into_port_buffer(1);
    buf->PROC_write_event_reference(n1_on); // duplicate
    port->PROC_process(1);

    CHECK(port->get_n_output_notes_active() == 1);

    port->PROC_prepare(1);
    buf = port->PROC_get_write_data_into_port_buffer(1);
    buf->PROC_write_event_reference(n2_on);
    port->PROC_process(1);

    CHECK(port->get_n_output_notes_active() == 2);

    port->PROC_prepare(1);
    buf = port->PROC_get_write_data_into_port_buffer(1);
    buf->PROC_write_event_reference(n1_off);
    port->PROC_process(1);

    CHECK(port->get_n_output_notes_active() == 1);

    port->PROC_prepare(1);
    buf = port->PROC_get_write_data_into_port_buffer(1);
    buf->PROC_write_event_reference(n2_off);
    port->PROC_process(1);

    CHECK(port->get_n_output_notes_active() == 0);
}

TEST_CASE("Ports - Jack Midi Out - Mute", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->set_muted(true);

    port->PROC_prepare(100);
    port->PROC_process(100);

    port->PROC_prepare(100);

    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiMessage<uint32_t, uint32_t> m1(0, 3, {0, 1, 2});
    MidiMessage<uint32_t, uint32_t> m2(0, 3, {0, 1, 2});
    buf->PROC_write_event_reference(m1);
    buf->PROC_write_event_reference(m2);
    
    port->PROC_process(100);

    CHECK(internal_port.midi_buffer.size() == 0);
}