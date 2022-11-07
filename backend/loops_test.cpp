#include <boost/ut.hpp>
#include "shoopdaloop_loops.h"
#include "shoopdaloop_backend.h"
#include <HalideBuffer.h>

#include <cstdlib>
#include <limits>

using namespace boost::ut;
using namespace Halide::Runtime;

struct loops_buffers {
    size_t process_samples;
    size_t max_loop_length;
    Buffer<int8_t, 1> states;
    Buffer<int8_t, 1> next_states;
    Buffer<int32_t, 1> positions;
    Buffer<int32_t, 1> lengths;
    Buffer<float, 2> samples_in, samples_out;
    Buffer<float, 2> samples_out_per_loop;
    Buffer<float, 2> storage_in, storage_out;
    Buffer<int32_t, 1> loops_to_ports;
    Buffer<int32_t, 1> loops_hard_sync_mapping;
    Buffer<int32_t, 1> loops_soft_sync_mapping;
    Buffer<float, 1> passthroughs;
    Buffer<float, 1> loop_volumes;
    Buffer<float, 1> port_volumes;
};

loops_buffers setup_buffers(
    size_t n_loops,
    size_t n_ports,
    size_t storage_samples,
    size_t process_samples,
    size_t max_loop_length
) {
    loops_buffers r;

    r.process_samples = process_samples;
    r.max_loop_length = max_loop_length;

    r.states = decltype(r.states)(n_loops);
    r.next_states = decltype(r.next_states)(n_loops);
    r.positions = decltype(r.lengths)(n_loops);
    r.lengths = decltype(r.lengths)(n_loops);
    r.loop_volumes = decltype(r.loop_volumes)(n_loops);
    r.loops_hard_sync_mapping = decltype(r.loops_hard_sync_mapping)(n_loops);
    r.loops_soft_sync_mapping = decltype(r.loops_soft_sync_mapping)(n_loops);
    r.loops_to_ports = decltype(r.loops_to_ports)(n_loops);

    r.samples_in = decltype(r.samples_in)(process_samples, n_ports);
    r.samples_out = decltype(r.samples_in)(process_samples, n_ports);
    r.samples_out_per_loop = decltype(r.samples_out_per_loop)(process_samples, n_loops);
    r.storage_in = decltype(r.storage_in)(max_loop_length, n_loops);
    r.storage_out = decltype(r.storage_out)(max_loop_length, n_loops);

    r.passthroughs = decltype(r.passthroughs)(n_ports);
    r.port_volumes = decltype(r.port_volumes)(n_ports);

    for(size_t i=0; i<n_loops; i++) {
        r.loops_hard_sync_mapping(i) = -1;
        r.loops_soft_sync_mapping(i) = -1;
        r.states(i) = Stopped;
        r.next_states(i) = Stopped;
        r.positions(i) = 0;
        r.lengths(i) = 0;
        r.loop_volumes(i) = 1.0f;
        r.loops_to_ports(i) = 0;

        for (size_t s = 0; s < max_loop_length; s++) {
            r.storage_in(s, i) = r.storage_out(s, i) = (float)-s;
        }
        for (size_t s = 0; s < process_samples; s++) {
            r.samples_out_per_loop(s, i) = std::numeric_limits<float>::min();
        }
    }

    for(size_t i=0; i<n_ports; i++) {
        r.passthroughs(i) = 1.0f;
        r.port_volumes(i) = 1.0f;

        for (size_t s = 0; s < process_samples; s++) {
            r.samples_in(s, i) = (float)s;
            r.samples_out(s, i) = std::numeric_limits<float>::max();
        }
    }

    return r;
}

void run_loops(
    loops_buffers &bufs
) {
    shoopdaloop_loops(
        bufs.samples_in,
        bufs.states,
        bufs.next_states,
        bufs.positions,
        bufs.lengths,
        bufs.storage_in,
        bufs.loops_to_ports,
        bufs.loops_hard_sync_mapping,
        bufs.loops_soft_sync_mapping,
        bufs.passthroughs,
        bufs.port_volumes,
        bufs.loop_volumes,
        bufs.process_samples,
        bufs.max_loop_length,
        bufs.samples_out,
        bufs.samples_out_per_loop,
        bufs.states,
        bufs.positions,
        bufs.lengths,
        bufs.storage_out
    );
}

suite loops_tests = []() {
    srand(1235930); // Seed random generator

    "stop"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states(0) = bufs.next_states(0) = Stopped;
        bufs.positions(0) = 2;
        bufs.lengths(0) = 11;
        run_loops(bufs);
        expect(bufs.states(0) == Stopped);
        expect(bufs.positions(0) == 2_i);
        expect(bufs.lengths(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            expect(eq(((float)bufs.storage_in(i, 0)), ((float)bufs.storage_out(i, 0)))) << " at index " << i;
        }
    };

    "play"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states(0) = bufs.next_states(0) = Playing;
        bufs.positions(0) = 2;
        bufs.lengths(0) = 11;
        run_loops(bufs);
        expect(eq(((int)bufs.states(0)), Playing));
        expect(bufs.positions(0) == 10_i);
        expect(bufs.lengths(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            expect(eq(((float)bufs.storage_in(i, 0)), ((float)bufs.storage_out(i, 0)))) << " at index " << i;
        }
    };

    "play_muted"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states(0) = bufs.next_states(0) = PlayingMuted;
        bufs.positions(0) = 2;
        bufs.lengths(0) = 11;
        run_loops(bufs);
        expect(eq(((int)bufs.states(0)), PlayingMuted));
        expect(bufs.positions(0) == 10_i);
        expect(bufs.lengths(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            expect(eq(((float)bufs.storage_in(i, 0)), ((float)bufs.storage_out(i, 0)))) << " at index " << i;
        }
    };

    "record"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 32);
        bufs.states(0) = bufs.next_states(0) = Recording;
        bufs.positions(0) = 2;
        bufs.lengths(0) = 3;
        run_loops(bufs);
        expect(eq(((int)bufs.states(0)), Recording));
        expect(bufs.positions(0) == 10_i);
        expect(bufs.lengths(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            if (i >= 2 && i <= 10) {
                // Recorded
                expect(eq(((float)bufs.storage_in(i, 0)), ((float)bufs.samples_in(i-2, 0)))) << " at index " << i;
            } else {
                expect(eq(((float)bufs.storage_in(i, 0)), ((float)bufs.storage_out(i, 0)))) << " at index " << i;
            }
        }
    };

    // Test ideas:
    // - non multiple of 8 buffers
    // - different sync kinds @ start and stop
    // - recording while pos != length
};

int main() {}