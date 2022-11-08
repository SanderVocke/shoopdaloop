#include <boost/ut.hpp>
#include "shoopdaloop_loops.h"
#include "shoopdaloop_loops_trace.h"
#include "shoopdaloop_loops_profile.h"
#include "shoopdaloop_backend.h"
#include <HalideBuffer.h>

#include <cstdlib>
#include <iostream>
#include <limits>
#include <string>

using namespace boost::ut;
using namespace Halide::Runtime;

backend_features_t g_backend = Default;

struct loops_buffers {
    size_t process_samples;
    size_t max_loop_length;
    Buffer<int8_t, 1> states_in, states_out;
    Buffer<int8_t, 1> next_states;
    Buffer<int32_t, 1> positions_in, positions_out;
    Buffer<int32_t, 1> lengths_in, lengths_out;
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

    r.states_in = decltype(r.states_in)(n_loops);
    r.states_out = decltype(r.states_out)(n_loops);
    r.next_states = decltype(r.next_states)(n_loops);
    r.positions_in = decltype(r.positions_in)(n_loops);
    r.positions_out = decltype(r.positions_out)(n_loops);
    r.lengths_in = decltype(r.lengths_in)(n_loops);
    r.lengths_out = decltype(r.lengths_out)(n_loops);
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
        r.states_in(i) = Stopped;
        r.next_states(i) = Stopped;
        r.positions_in(i) = 0;
        r.lengths_in(i) = 0;
        r.loop_volumes(i) = 1.0f;
        r.loops_to_ports(i) = 0;

        for (size_t s = 0; s < max_loop_length; s++) {
            r.storage_in(s, i) = r.storage_out(s, i) = -((float)s);
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
    auto call_backend = [](auto ...args) {
        switch (g_backend) {
            case Default: return shoopdaloop_loops(args...);
            case Tracing: return shoopdaloop_loops_trace(args...);
            case Profiling: return shoopdaloop_loops_profile(args...);
            default: throw std::runtime_error("Unsupported back-end");
        }
    };
    call_backend(
        bufs.samples_in,
        bufs.states_in,
        bufs.next_states,
        bufs.positions_in,
        bufs.lengths_in,
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
        bufs.states_out,
        bufs.positions_out,
        bufs.lengths_out,
        bufs.storage_out
    );
}

suite loops_tests = []() {
    srand(1235930); // Seed random generator

    "1_stop"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = bufs.next_states(0) = Stopped;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        run_loops(bufs);
        expect(bufs.states_out(0) == Stopped);
        expect(bufs.positions_out(0) == 2_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out, sample_in)) << " at index " << i;
        }
    };

    "2_play"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = bufs.next_states(0) = Playing;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
        };
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out, storage)) << " at index " << i;
        }
    };

    "3_play_muted"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = bufs.next_states(0) = PlayingMuted;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), PlayingMuted));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out, sample_in)) << " at index " << i;
        }
    };

    "4_record"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 32);
        bufs.states_in(0) = bufs.next_states(0) = Recording;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 3;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);


        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            if (i >= 2 && i < 10) {
                // Recorded
                float sample_in = bufs.samples_in(i-2, 0);
                expect(eq(storage_out, sample_in)) << " at index " << i;
            } else {
                expect(eq(storage_out, storage_in)) << " at index " << i;
            }
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out, sample_in)) << " at index " << i;
        }
    };

    "5_passthrough"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = bufs.next_states(0) = Stopped;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        bufs.passthroughs(0) = 0.5f;
        run_loops(bufs);
        expect(bufs.states_out(0) == Stopped);
        expect(bufs.positions_out(0) == 2_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out, sample_in*0.5f)) << " at index " << i;
        }
    };

    "6_loop_volume"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = bufs.next_states(0) = Playing;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        bufs.loop_volumes(0) = 0.5f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out, sample_in + storage * 0.5f)) << " at index " << i;
        }
    };

    "7_soft_sync_play_to_stop_self"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = Playing;
        bufs.next_states(0) = Stopped;
        bufs.positions_in(0) = 10;
        bufs.lengths_in(0) = 16;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Stopped));
        expect(bufs.positions_out(0) == 0_i);
        expect(bufs.lengths_out(0) == 16);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            if(i < 6) {
                float storage = bufs.storage_in(i+10, 0);
                // Still playing back
                expect(eq(sample_out, storage)) << " at index " << i;
            } else {
                // Went to stopped
                expect(sample_out == 0.0_f) << " at index " << i;
            }
        }
    };

    "8_soft_sync_play_to_record_self"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = Playing;
        bufs.next_states(0) = Recording;
        bufs.positions_in(0) = 10;
        bufs.lengths_in(0) = 16;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 2_i);
        expect(bufs.lengths_out(0) == 2_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            if (i < 2) {
                // Recording starts from 0, two samples recorded
                float sample_in = bufs.samples_in(i+6, 0);
                expect(eq(storage_out, sample_in)) << " at index " << i;
            } else {
                // Rest was untouched during playback
                expect(eq(storage_out, storage_in)) << " at index " << i;
            }
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            if (i < 6) {
                // Still playing back
                float storage = bufs.storage_in(i+10, 0);
                expect(eq(sample_out, storage)) << " at index " << i;
            } else {
                // Recording, just passthrough
                expect(eq(sample_out, 0.0_f)) << " at index " << i;
            }
        }
    };

    "9_soft_sync_stop_to_record_self"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = Stopped;
        bufs.next_states(0) = Recording;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 0;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 8_i);
        expect(bufs.lengths_out(0) == 8_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            if (i < 8) {
                // Recording starts from 0, two samples recorded
                float sample_in = bufs.samples_in(i, 0);
                expect(eq(storage_out, sample_in)) << " at index " << i;
            } else {
                // Rest was untouched
                expect(eq(storage_out, storage_in)) << " at index " << i;
            }
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_out = bufs.samples_out(i,0);
            // Recording, just passthrough
            expect(eq(sample_out, 0.0_f)) << " at index " << i;
        }
    };

    "10_soft_sync_record_to_play_self"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = Recording;
        bufs.next_states(0) = Playing;
        bufs.positions_in(0) = 5;
        bufs.lengths_in(0) = 5;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 3_i);
        expect(bufs.lengths_out(0) == 5_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            // Recording should have stopped immediately
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            // Playback should have started immediately
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+5, 0);
            // Recording, just passthrough
            expect(eq(sample_out, storage)) << " at index " << i;
        }
    };

    "11_soft_sync_play_to_record_self"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 16, 8, 16);
        bufs.states_in(0) = Playing;
        bufs.next_states(0) = Recording;
        bufs.positions_in(0) = 5;
        bufs.lengths_in(0) = 10;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 3_i);
        expect(bufs.lengths_out(0) == 3_i);

        for(size_t i=0; i<bufs.max_loop_length; i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            if(i<3) {
                float sample_in = bufs.samples_in(i+5,0);
                expect(eq(storage_out, sample_in)) << " at index " << i;
            } else {
                expect(eq(storage_out, storage_in)) << " at index " << i;
            }
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_out = bufs.samples_out(i,0);
            if(i<5) {
                float storage = bufs.storage_in(i+5, 0);
                expect(eq(sample_out, storage)) << " at index " << i;
            } else {
                expect(eq(sample_out, 0.0_f)) << " at index " << i;
            }            
        }
    };

    // Test ideas:
    // - non multiple of 8 buffers
    // - different sync kinds @ start and stop
    // - recording while pos != length
};

void usage(std::string name) {
    std::cout << "Usage: " << name << " [trace] [profile] [filter=FILTER]" << std::endl;
}

int main(int argc, const char *argv[]) {
    std::string name(argv[0]);

    for(size_t i = 1; i<(size_t) argc; i++) {
        std::string arg(argv[i]);
        auto filter_arg = std::string("filter=");
        if(arg == "trace") {
            g_backend = Tracing;
        } else if (arg == "profile") {
            g_backend = Profiling;
        } else if (arg.substr(0, filter_arg.size()) == filter_arg) {
            static auto filter = arg.substr(filter_arg.size());
            std::cout << "Filtering on: \"" << filter << "\"" << std::endl;
            
            cfg<override> = {.filter = filter};
        } else {
            g_backend = Default;
        }
    }
}