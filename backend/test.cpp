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
    const size_t storage_size = 48000*60;
    const size_t buf_size = 4096;
    const size_t n_loops = 96;
    Buffer<int8_t, 1> states_a(n_loops), states_b(n_loops);
    Buffer<int32_t, 1> positions_a(n_loops), positions_b(n_loops);
    Buffer<int32_t, 1> lengths_a(n_loops), lengths_b(n_loops);
    Buffer<int32_t, 1> input_buffer_map(n_loops);
    Buffer<float, 2> samples_in(buf_size, n_loops), samples_out(buf_size, n_loops), storage_in(storage_size, n_loops), storage_out(storage_size, n_loops);
    Buffer<float, 2> samples_out_per_loop(buf_size, n_loops);
    Buffer<float, 1> passthroughs(n_loops);

    for(size_t i=0; i<n_loops; i++) {
        states_a(i) = Recording;
        positions_a(i) = i;
        input_buffer_map(i) = i;
    }

    auto t = benchmark(10, 1, [&]() {
        shoopdaloop_loops(
            samples_in,
            states_a,
            positions_a,
            lengths_a,
            storage_in,
            input_buffer_map,
            passthroughs,
            buf_size,
            storage_size,
            samples_out,
            samples_out_per_loop,
            states_b,
            positions_b,
            lengths_b,
            storage_out
            );
    });

    printf("time: %f\n", t);
    printf("Success!\n");
    return 0;
}
