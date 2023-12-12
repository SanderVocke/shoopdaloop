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

    port.PROC_prepare(6);
    port.PROC_process(6);

    auto buf = port.PROC_get_buffer(6);

    CHECK(buf[0] == Catch::Approx(0.0f));
    CHECK(buf[1] == Catch::Approx(1.0f));
    CHECK(buf[2] == Catch::Approx(2.0f));
    CHECK(buf[3] == Catch::Approx(3.0f));
    CHECK(buf[4] == Catch::Approx(4.0f));
    CHECK(buf[5] == Catch::Approx(5.0f));
}