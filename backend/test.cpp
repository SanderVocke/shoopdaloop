#include <cmath>
#include <cstdint>
#include <cstdio>
#ifdef __SSE2__
#include <emmintrin.h>
#elif __ARM_NEON
#include <arm_neon.h>
#endif

#include "HalideBuffer.h"
#include "halide_benchmark.h"

using namespace Halide::Runtime;
using namespace Halide::Tools;

#include "shoopdaloop_loops.h"
#include "types.hpp"

int main(int argc, char **argv) {
    Buffer<uint8_t, 1> states_a(104), states_b(104);
    Buffer<uint16_t, 1> positions_a(104), positions_b(104);
    Buffer<uint16_t, 1> lengths_a(104), lengths_b(104);
    Buffer<float, 2> samples_in(104, 256), samples_out(104, 256), storage_in(104, 1024), storage_out(104, 1024);

    for(size_t i=0; i<104; i++) {
        states_a(i) = Playing;
        positions_a(i) = i;
    }

    auto t = benchmark(10, 1, [&]() {
        shoopdaloop_loops(samples_in, states_a, positions_a, lengths_a, storage_in, 256, 1024, samples_out, states_b, positions_b, lengths_b, storage_out);
    });

    printf("time: %f\n", t);
    printf("Success!\n");
    return 0;
}
