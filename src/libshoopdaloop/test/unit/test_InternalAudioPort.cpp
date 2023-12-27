#include "InternalAudioPort.h"
#include "catch2/catch_approx.hpp"
#include <catch2/catch_test_macros.hpp>

TEST_CASE("Ports - Internal Audio - Properties", "[InternalAudioPort][ports][audio]") {
    InternalAudioPort<float> port ("dummy", 10);

    CHECK(port.has_internal_read_access());
    CHECK(port.has_internal_write_access());
    CHECK(!port.has_implicit_input_source());
    CHECK(!port.has_implicit_output_sink());
}

TEST_CASE("Ports - Internal Audio - Gain", "[InternalAudioPort][ports][audio]") {
    InternalAudioPort<float> port ("dummy", 10);

    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port.set_gain(0.5f);

    port.PROC_prepare(3);
    auto buf = port.PROC_get_buffer(3);
    memcpy((void*) buf, (void*) samples, 3 * sizeof(audio_sample_t));
    port.PROC_process(3);
    buf = port.PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.5f));
    CHECK(buf[2] == Catch::Approx(1.0f));
}

TEST_CASE("Ports - Internal Audio - Mute", "[InternalAudioPort][ports][audio]") {
    InternalAudioPort<float> port ("dummy", 10);

    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port.set_muted(true);

    port.PROC_prepare(3);
    auto buf = port.PROC_get_buffer(3);
    memcpy((void*) buf, (void*) samples, 3 * sizeof(audio_sample_t));
    port.PROC_process(3);
    buf = port.PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.0f));
    CHECK(buf[2] == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Internal Audio - Peak", "[InternalAudioPort][ports][audio]") {
    InternalAudioPort<float> port ("dummy", 10);
    std::vector<audio_sample_t> samples = {
        0.0f, 0.5f, 0.9f, 0.5f, 0.0f
    };

    port.PROC_prepare(5);
    auto buf = port.PROC_get_buffer(5);
    memcpy((void*) buf, (void*) samples.data(), 5 * sizeof(audio_sample_t));
    port.PROC_process(5);

    CHECK(port.get_input_peak() == Catch::Approx(0.9f));
    CHECK(port.get_output_peak() == Catch::Approx(0.9f));

    port.set_muted(true);
    port.reset_input_peak();
    port.reset_output_peak();
    port.PROC_prepare(5);
    buf = port.PROC_get_buffer(5);
    memcpy((void*) buf, (void*) samples.data(), 5 * sizeof(audio_sample_t));
    port.PROC_process(5);

    CHECK(port.get_input_peak() == Catch::Approx(0.9f));
    CHECK(port.get_output_peak() == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Internal Audio - Noop Zero", "[InternalAudioPort][ports][audio]") {
    InternalAudioPort<float> port ("dummy", 10);
    std::vector<audio_sample_t> samples = {
        0.0f, 0.5f, 0.9f, 0.5f, 0.0f
    };

    port.PROC_prepare(5);
    auto buf = port.PROC_get_buffer(5);
    memcpy((void*) buf, (void*) samples.data(), 5 * sizeof(audio_sample_t));
    port.PROC_process(5);

    CHECK(port.get_input_peak() == Catch::Approx(0.9f));
    CHECK(port.get_output_peak() == Catch::Approx(0.9f));

    std::vector<audio_sample_t> second_round_output(5);
    port.PROC_prepare(5);
    port.PROC_process(5);
    buf = port.PROC_get_buffer(5);
    memcpy((void*) second_round_output.data(), (void*) buf, 5 * sizeof(audio_sample_t));

    CHECK(second_round_output[0] == Catch::Approx(0.0f));
    CHECK(second_round_output[1] == Catch::Approx(0.0f));
    CHECK(second_round_output[2] == Catch::Approx(0.0f));
    CHECK(second_round_output[3] == Catch::Approx(0.0f));
    CHECK(second_round_output[4] == Catch::Approx(0.0f));
}