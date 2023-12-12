#include "DummyAudioMidiDriver.h"
#include "catch2/catch_approx.hpp"
#include <catch2/catch_test_macros.hpp>

TEST_CASE("Ports - Dummy Audio In - Properties", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Input);

    CHECK(port.has_internal_read_access());
    CHECK(!port.has_internal_write_access());
    CHECK(port.has_implicit_input_source());
    CHECK(!port.has_implicit_output_sink());
}

TEST_CASE("Ports - Dummy Audio In - Buffers", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Input);

    port.PROC_prepare(10);
    port.PROC_process(10);
    auto buf = port.PROC_get_buffer(10);
    CHECK(buf != nullptr);
    auto buf2 = port.PROC_get_buffer(10);
    CHECK(buf == buf2);

}

TEST_CASE("Ports - Dummy Audio In - Queue", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Input);
    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port.queue_data(6, samples);

    port.PROC_prepare(3);
    port.PROC_process(3);
    auto buf = port.PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(1.0f));
    CHECK(buf[2] == Catch::Approx(2.0f));

    port.PROC_prepare(3);
    port.PROC_process(3);
    buf = port.PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(3.0f));
    CHECK(buf[1] == Catch::Approx(4.0f));
    CHECK(buf[2] == Catch::Approx(5.0f));

    port.PROC_prepare(3);
    port.PROC_process(3);

    buf = port.PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.0f));
    CHECK(buf[2] == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Dummy Audio In - Gain", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Input);
    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port.queue_data(6, samples);
    port.set_gain(0.5f);

    port.PROC_prepare(3);
    port.PROC_process(3);
    auto buf = port.PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.5f));
    CHECK(buf[2] == Catch::Approx(1.0f));
}

TEST_CASE("Ports - Dummy Audio In - Mute", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Input);
    audio_sample_t samples[6] = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port.queue_data(6, samples);
    port.set_muted(true);

    port.PROC_prepare(3);
    port.PROC_process(3);
    auto buf = port.PROC_get_buffer(3);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(0.0f));
    CHECK(buf[2] == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Dummy Audio In - Peak", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Input);
    audio_sample_t samples[6] = {
        5.0f, 4.0f, 3.0f, 2.0f, 1.0f, 0.0f
    };

    port.queue_data(6, samples);

    port.PROC_prepare(2);
    port.PROC_process(2);

    CHECK(port.get_peak() == Catch::Approx(5.0f));

    port.PROC_prepare(1);
    port.PROC_process(1);

    CHECK(port.get_peak() == Catch::Approx(5.0f));

    port.reset_peak();

    port.PROC_prepare(3);
    port.PROC_process(3);

    CHECK(port.get_peak() == Catch::Approx(2.0f));
}

TEST_CASE("Ports - Dummy Audio Out - Properties", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Output);

    CHECK(!port.has_internal_read_access());
    CHECK(port.has_internal_write_access());
    CHECK(!port.has_implicit_input_source());
    CHECK(port.has_implicit_output_sink());
}

TEST_CASE("Ports - Dummy Audio Out - Buffers", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Output);

    port.PROC_prepare(10);
    port.PROC_process(10);
    auto buf = port.PROC_get_buffer(10);
    CHECK(buf != nullptr);
    auto buf2 = port.PROC_get_buffer(10);
    CHECK(buf == buf2);

}

TEST_CASE("Ports - Dummy Audio Out - Queue", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Output);
    std::vector<audio_sample_t> samples = {
        0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f
    };

    port.request_data(6);

    port.PROC_prepare(6);
    auto buf = port.PROC_get_buffer(6);
    memcpy((void*)buf, (void*)samples.data(), 6 * sizeof(audio_sample_t));
    port.PROC_process(6);

    auto dequeued = port.dequeue_data(6);
    CHECK(dequeued == samples);
}

TEST_CASE("Ports - Dummy Audio Out - Gain", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Output);
    std::vector<audio_sample_t> samples = {
        0.0f, 1.0f, 2.0f
    };

    port.request_data(3);
    port.set_gain(0.5f);

    port.PROC_prepare(3);
    auto buf = port.PROC_get_buffer(3);
    memcpy((void*)buf, (void*)samples.data(), 3 * sizeof(audio_sample_t));
    port.PROC_process(3);

    auto dequeued = port.dequeue_data(3);
    CHECK(dequeued[0] == Catch::Approx(0.0f));
    CHECK(dequeued[1] == Catch::Approx(0.5f));
    CHECK(dequeued[2] == Catch::Approx(1.0f));
}

TEST_CASE("Ports - Dummy Audio Out - Mute", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Output);
    std::vector<audio_sample_t> samples = {
        0.0f, 1.0f, 2.0f
    };

    port.request_data(3);
    port.set_muted(true);

    port.PROC_prepare(3);
    auto buf = port.PROC_get_buffer(3);
    memcpy((void*)buf, (void*)samples.data(), 3 * sizeof(audio_sample_t));
    port.PROC_process(3);

    auto dequeued = port.dequeue_data(3);
    CHECK(dequeued[0] == Catch::Approx(0.0f));
    CHECK(dequeued[1] == Catch::Approx(0.0f));
    CHECK(dequeued[2] == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Dummy Audio Out - Peak", "[Ports][audio]") {
    DummyAudioPort port ("dummy", Output);
    std::vector<audio_sample_t> samples = {
        0.0f, 1.0f, 2.0f
    };

    port.PROC_prepare(3);
    auto buf = port.PROC_get_buffer(3);
    memcpy((void*)buf, (void*)samples.data(), 3 * sizeof(audio_sample_t));
    port.PROC_process(3);

    CHECK(port.get_peak() == Catch::Approx(2.0f));
}