#include "InternalAudioPort.h"
#include "catch2/catch_approx.hpp"
#include <catch2/catch_test_macros.hpp>

TEST_CASE("Ports - Internal Audio - Properties", "[InternalPorts][ports][audio]") {
    InternalAudioPort<float> port ("dummy", Input, 10);

    CHECK(port.has_internal_read_access());
    CHECK(port.has_internal_write_access());
    CHECK(port.has_implicit_input_source());
    CHECK(port.has_implicit_output_sink());
}
