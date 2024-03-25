#include "DummyAudioMidiDriver.h"
#include "catch2/catch_approx.hpp"
#include <catch2/catch_test_macros.hpp>

TEST_CASE("Ports - Dummy Audio In - Properties", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Input, nullptr);

    CHECK(port.has_internal_read_access());
    CHECK(!port.has_internal_write_access());
    CHECK(port.has_implicit_input_source());
    CHECK(!port.has_implicit_output_sink());
}

TEST_CASE("Ports - Dummy Audio In - Buffers", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Input, nullptr);

    port.PROC_prepare(10);
    port.PROC_process(10);
    auto buf = port.PROC_get_buffer(10);
    CHECK(buf != nullptr);
    auto buf2 = port.PROC_get_buffer(10);
    CHECK(buf == buf2);

}

TEST_CASE("Ports - Dummy Audio In - Queue", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Input, nullptr);
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

TEST_CASE("Ports - Dummy Audio In - Gain", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Input, nullptr);
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

TEST_CASE("Ports - Dummy Audio In - Mute", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Input, nullptr);
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

TEST_CASE("Ports - Dummy Audio In - Peak", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Input, nullptr);
    audio_sample_t samples[6] = {
        5.0f, 4.0f, 3.0f, 2.0f, 1.0f, 0.0f
    };

    port.queue_data(6, samples);

    port.PROC_prepare(2);
    port.PROC_process(2);

    CHECK(port.get_input_peak() == Catch::Approx(5.0f));
    CHECK(port.get_output_peak() == Catch::Approx(5.0f));

    port.PROC_prepare(1);
    port.PROC_process(1);

    CHECK(port.get_input_peak() == Catch::Approx(5.0f));
    CHECK(port.get_output_peak() == Catch::Approx(5.0f));

    port.reset_input_peak();
    port.reset_output_peak();

    port.PROC_prepare(3);
    port.PROC_process(3);

    CHECK(port.get_input_peak() == Catch::Approx(2.0f));
    CHECK(port.get_output_peak() == Catch::Approx(2.0f));
}

TEST_CASE("Ports - Dummy Audio In - get ringbuffer data", "[DummyPorts][ports][audio]") {
    auto pool = std::make_shared<BufferQueue<float>::BufferPool>("Test", 10, 4);
    auto port = DummyAudioPort("test", ShoopPortDirection_Input, pool);

    // Process 4 samples
    port.PROC_prepare(4);
    auto buf = port.PROC_get_buffer(4);
    buf[0] = 0.0f;
    buf[1] = 0.1f;
    buf[2] = 0.2f;
    buf[3] = 0.3f;
    port.PROC_process(4);

    // Get the ringbuffer content
    auto s = port.PROC_get_ringbuffer_contents();
    CHECK(s.n_samples >= 4);
    CHECK(s.data->back()->at(0) == 0.0f);
    CHECK(s.data->back()->at(1) == 0.1f);
    CHECK(s.data->back()->at(2) == 0.2f);
    CHECK(s.data->back()->at(3) == 0.3f);
};

TEST_CASE("Ports - Dummy Audio Out - Properties", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Output, nullptr);

    CHECK(!port.has_internal_read_access());
    CHECK(port.has_internal_write_access());
    CHECK(!port.has_implicit_input_source());
    CHECK(port.has_implicit_output_sink());
}

TEST_CASE("Ports - Dummy Audio Out - Buffers", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Output, nullptr);

    port.PROC_prepare(10);
    port.PROC_process(10);
    auto buf = port.PROC_get_buffer(10);
    CHECK(buf != nullptr);
    auto buf2 = port.PROC_get_buffer(10);
    CHECK(buf == buf2);

}

TEST_CASE("Ports - Dummy Audio Out - Queue", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Output, nullptr);
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

TEST_CASE("Ports - Dummy Audio Out - Gain", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Output, nullptr);
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

TEST_CASE("Ports - Dummy Audio Out - Mute", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Output, nullptr);
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

TEST_CASE("Ports - Dummy Audio Out - Peak", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Output, nullptr);
    std::vector<audio_sample_t> samples = {
        0.0f, 1.0f, 2.0f
    };

    port.PROC_prepare(3);
    auto buf = port.PROC_get_buffer(3);
    memcpy((void*)buf, (void*)samples.data(), 3 * sizeof(audio_sample_t));
    port.PROC_process(3);

    CHECK(port.get_input_peak() == Catch::Approx(2.0f));
    CHECK(port.get_output_peak() == Catch::Approx(2.0f));

    port.reset_output_peak();
    port.reset_input_peak();
    port.set_muted(true);

    port.PROC_prepare(3);
    buf = port.PROC_get_buffer(3);
    memcpy((void*)buf, (void*)samples.data(), 3 * sizeof(audio_sample_t));
    port.PROC_process(3);

    CHECK(port.get_input_peak() == Catch::Approx(2.0f));
    CHECK(port.get_output_peak() == Catch::Approx(0.0f));
}

TEST_CASE("Ports - Dummy Audio Out - Noop Zero", "[DummyPorts][ports][audio]") {
    DummyAudioPort port ("dummy", ShoopPortDirection_Output, nullptr);
    std::vector<audio_sample_t> samples = {
        0.0f, 1.0f, 2.0f
    };

    port.request_data(6);

    port.PROC_prepare(3);
    auto buf = port.PROC_get_buffer(3);
    memcpy((void*)buf, (void*)samples.data(), 3 * sizeof(audio_sample_t));
    port.PROC_process(3);

    auto dequeued = port.dequeue_data(3);
    CHECK(dequeued[0] == Catch::Approx(0.0f));
    CHECK(dequeued[1] == Catch::Approx(1.0f));
    CHECK(dequeued[2] == Catch::Approx(2.0f));

    port.PROC_prepare(3);
    port.PROC_process(3);

    dequeued = port.dequeue_data(3);
    CHECK(dequeued[0] == Catch::Approx(0.0f));
    CHECK(dequeued[1] == Catch::Approx(0.0f));
    CHECK(dequeued[2] == Catch::Approx(0.0f));
}