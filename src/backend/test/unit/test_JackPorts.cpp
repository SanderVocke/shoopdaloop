#include "JackAudioMidiDriver.h"
#include "MidiBuffer.h"
#include "AudioPort.h"
#include "catch2/catch_approx.hpp"
#include <catch2/catch_test_macros.hpp>
#include <jack/midiport.h>
#include <memory>
#include "helpers.h"
#include "shoop_shared_ptr.h"

std::unique_ptr<JackTestAudioMidiDriver> open_test_driver() {
    JackTestApi::internal_reset_api();
    auto driver = std::make_unique<JackTestAudioMidiDriver>();
    JackAudioMidiDriverSettings settings;
    settings.client_name_hint = "test";
    settings.maybe_server_name_hint = std::nullopt;
    driver->start(settings);
    return driver;
}

MidiStorageElem to_msg(jack_midi_event_t &m) {
    MidiStorageElem msg;
    msg.time = m.time;
    msg.size = m.size;
    memcpy(msg.bytes, m.buffer, m.size);
    return msg;
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
    auto pool = shoop_make_shared<BufferQueue<float>::UsedBufferPool>(10, 5, 4);
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
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->PROC_prepare(100);
    port->PROC_process(100);

    auto buf = port->PROC_get_read_output_data_buffer(100);
    CHECK(buf->n_events() == 0);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);

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
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->set_muted(true);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    auto buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->n_events() == 0);
}

TEST_CASE("Ports - Jack Midi In - Message Counter", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    internal_port.midi_buffer.push_back(m1);
    internal_port.midi_buffer.push_back(m2);

    port->PROC_prepare(100);
    port->PROC_process(100);

    // Verify messages are received
    auto buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->n_events() == 2);
    CHECK(buf->get_event(0).contents_equal(m1));
    CHECK(buf->get_event(1).contents_equal(m2));

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

    // When muted, messages should not be output
    buf = port->PROC_get_read_output_data_buffer(100);
    CHECK(buf->n_events() == 0);
}

TEST_CASE("Ports - Jack Midi In - Note Tracker", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());
    using Msg = MidiStorageElem;

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

TEST_CASE("Ports - Jack Midi In - get ringbuffer data", "[JackPorts][ports][midi]") {
    // This test verifies that input port correctly handles receiving MIDI messages.
    // Note: Ringbuffer snapshot behavior has changed - for JACK ports, messages are
    // received via the port's read buffer, not via ringbuffer snapshot.
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Input);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());
    using Msg = MidiStorageElem;

    std::vector<Msg> in = {
        create_noteOn<Msg>(0, 0, 100, 127),
        create_noteOn<Msg>(1, 0, 110, 127),
        create_noteOff<Msg>(2, 0, 100, 127),
        create_noteOff<Msg>(3, 0, 110, 127)
    };

    internal_port.midi_buffer = in;
    port->PROC_prepare(100);
    port->PROC_process(100);

    // Verify messages were received via the port's read buffer
    auto buf = port->PROC_get_read_output_data_buffer(100);
    REQUIRE(buf->n_events() == 4);

    // Check message contents
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
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->PROC_prepare(100);
    port->PROC_process(100);

    port->PROC_prepare(100);

    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    buf->write_event(m1);
    buf->write_event(m2);
    
    port->PROC_process(100);

    REQUIRE(internal_port.midi_buffer.size() == 2);
    CHECK(internal_port.midi_buffer[0].contents_equal(m1));
    CHECK(internal_port.midi_buffer[1].contents_equal(m2));
}

TEST_CASE("Ports - Jack Midi Out - Sort", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

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

    REQUIRE(internal_port.midi_buffer.size() == 3);
    CHECK(internal_port.midi_buffer[0].contents_equal(m2));
    CHECK(internal_port.midi_buffer[1].contents_equal(m1));
    CHECK(internal_port.midi_buffer[2].contents_equal(m3));
}

TEST_CASE("Ports - Jack Midi Out - Message Counter", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    port->PROC_prepare(100);
    auto buf = port->PROC_get_write_data_into_port_buffer(100);

    MidiStorageElem m1; m1.time = 0; m1.size = 3; m1.bytes[0] = 0; m1.bytes[1] = 1; m1.bytes[2] = 2;
    MidiStorageElem m2; m2.time = 0; m2.size = 3; m2.bytes[0] = 0; m2.bytes[1] = 1; m2.bytes[2] = 2;
    buf->write_event(m1);
    buf->write_event(m2);

    port->PROC_process(100);

    // Verify messages are actually written to the port
    REQUIRE(internal_port.midi_buffer.size() == 2);
    CHECK(internal_port.midi_buffer[0].contents_equal(m1));
    CHECK(internal_port.midi_buffer[1].contents_equal(m2));

    port->reset_n_input_events();
    port->reset_n_output_events();

    port->PROC_prepare(100);
    port->PROC_process(100);

    // No new messages written, counters should be 0
    CHECK(port->get_n_input_events() == 0);
    CHECK(port->get_n_output_events() == 0);

    port->set_muted(true);
    port->PROC_prepare(100);
    buf = port->PROC_get_write_data_into_port_buffer(100);
    buf->write_event(m1);
    buf->write_event(m2);

    port->PROC_process(100);

    // When muted, messages should not be written to internal buffer
    CHECK(internal_port.midi_buffer.size() == 0);
}

TEST_CASE("Ports - Jack Midi Out - Note Tracker", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

    auto n1_on = create_noteOn<Msg>(0, 0, 100, 127);
    auto n2_on = create_noteOn<Msg>(1, 0, 110, 127);
    auto n1_off = create_noteOff<Msg>(2, 0, 100, 127);
    auto n2_off = create_noteOff<Msg>(3, 0, 110, 127);

    CHECK(port->get_n_output_notes_active() == 0);

    // Write note-on messages
    port->PROC_prepare(10);
    auto buf = port->PROC_get_write_data_into_port_buffer(10);
    buf->write_event(n1_on);
    buf->write_event(n2_on);
    port->PROC_process(10);

    // Verify messages were written to internal port
    REQUIRE(internal_port.midi_buffer.size() == 2);
    CHECK(internal_port.midi_buffer[0].contents_equal(n1_on));
    CHECK(internal_port.midi_buffer[1].contents_equal(n2_on));

    // Clear and write note-off messages
    internal_port.midi_buffer.clear();
    port->PROC_prepare(10);
    buf = port->PROC_get_write_data_into_port_buffer(10);
    buf->write_event(n1_off);
    buf->write_event(n2_off);
    port->PROC_process(10);

    // Verify note-off messages were written
    REQUIRE(internal_port.midi_buffer.size() == 2);
    CHECK(internal_port.midi_buffer[0].contents_equal(n1_off));
    CHECK(internal_port.midi_buffer[1].contents_equal(n2_off));
}

TEST_CASE("Ports - Jack Midi Out - Mute", "[JackPorts][ports][midi]") {
    auto driver = open_test_driver();
    auto port = driver->open_midi_port("test", ShoopPortDirection_Output);
    auto &internal_port = JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle());

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

    CHECK(internal_port.midi_buffer.size() == 0);
}