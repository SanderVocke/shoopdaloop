#include <algorithm>
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
    size_t latency_buf_size;
    size_t max_n_events;
    size_t n_mixed_output_ports;
    int8_t storage_lock;
    Buffer<int32_t, 0> latency_buf_write_pos;
    Buffer<int8_t, 1> states_in, states_out;
    Buffer<int8_t, 2> next_states_in, next_states_out;
    Buffer<int8_t, 2> next_states_countdown_in, next_states_countdown_out;
    Buffer<int32_t, 2> port_event_timestamps_in;
    Buffer<int32_t, 2> event_recording_timestamps_out;
    Buffer<int32_t, 1> n_port_events;
    Buffer<int32_t, 1> positions_in, positions_out;
    Buffer<int32_t, 1> lengths_in, lengths_out;
    Buffer<float, 2> samples_in, samples_out;
    Buffer<float, 2> mixed_samples_out;
    Buffer<float, 2> latency_buf;
    Buffer<int32_t, 1> port_recording_latencies;
    Buffer<float, 2> samples_out_per_loop;
    Buffer<float, 2> storage_in, storage_out;
    Buffer<int32_t, 1> loops_to_ports;
    Buffer<int32_t, 1> loops_hard_sync_mapping;
    Buffer<int32_t, 1> loops_soft_sync_mapping;
    Buffer<int32_t, 1> port_input_override_map;
    Buffer<int32_t, 1> ports_to_mixed_outputs_map;
    Buffer<float, 1> passthroughs;
    Buffer<float, 1> loop_volumes;
    Buffer<float, 1> port_volumes;
    Buffer<int8_t, 1> ports_muted;
    Buffer<int8_t, 1> port_inputs_muted;
    Buffer<float, 1> port_input_peaks;
    Buffer<float, 1> port_output_peaks;
    Buffer<float, 1> loop_output_peaks;
};

loops_buffers setup_buffers(
    size_t n_loops,
    size_t n_ports,
    size_t n_mixed_output_ports,
    size_t storage_samples,
    size_t process_samples
) {
    loops_buffers r;

    r.max_n_events = 10;
    r.process_samples = process_samples;
    r.latency_buf_size = process_samples * 4;
    r.n_mixed_output_ports = n_mixed_output_ports;

    r.states_in = decltype(r.states_in)(n_loops);
    r.states_out = decltype(r.states_out)(n_loops);
    r.next_states_in = decltype(r.next_states_in)(2, n_loops);
    r.next_states_out = decltype(r.next_states_out)(2, n_loops);
    r.next_states_countdown_in = decltype(r.next_states_countdown_in)(2, n_loops);
    r.next_states_countdown_out = decltype(r.next_states_countdown_out)(2, n_loops);
    r.positions_in = decltype(r.positions_in)(n_loops);
    r.positions_out = decltype(r.positions_out)(n_loops);
    r.lengths_in = decltype(r.lengths_in)(n_loops);
    r.lengths_out = decltype(r.lengths_out)(n_loops);
    r.loop_volumes = decltype(r.loop_volumes)(n_loops);
    r.loops_hard_sync_mapping = decltype(r.loops_hard_sync_mapping)(n_loops);
    r.loops_soft_sync_mapping = decltype(r.loops_soft_sync_mapping)(n_loops);
    r.port_input_override_map = decltype(r.port_input_override_map)(n_ports);
    r.ports_to_mixed_outputs_map = decltype(r.ports_to_mixed_outputs_map)(n_ports);
    r.loops_to_ports = decltype(r.loops_to_ports)(n_loops);
    r.loop_output_peaks = decltype(r.loop_output_peaks)(n_loops);
    r.storage_lock = 0;
    
    r.samples_in = decltype(r.samples_in)(process_samples, n_ports);
    r.samples_out = decltype(r.samples_in)(process_samples, n_ports);
    r.mixed_samples_out = decltype(r.mixed_samples_out)(process_samples, n_mixed_output_ports);
    r.samples_out_per_loop = decltype(r.samples_out_per_loop)(process_samples, n_loops);
    r.storage_in = decltype(r.storage_in)(storage_samples, n_loops);
    r.storage_out = decltype(r.storage_out)(storage_samples, n_loops);
    r.event_recording_timestamps_out = decltype(r.event_recording_timestamps_out)(r.max_n_events, n_loops);

    r.passthroughs = decltype(r.passthroughs)(n_ports);
    r.port_volumes = decltype(r.port_volumes)(n_ports);
    r.ports_muted = decltype(r.ports_muted)(n_ports);
    r.port_inputs_muted = decltype(r.port_inputs_muted)(n_ports);
    r.port_input_peaks = decltype(r.port_input_peaks)(n_ports);
    r.port_output_peaks = decltype(r.port_output_peaks)(n_ports);
    r.n_port_events = decltype(r.n_port_events)(n_ports);
    r.port_event_timestamps_in = decltype(r.port_event_timestamps_in)(r.max_n_events, n_ports);

    r.latency_buf = decltype(r.latency_buf)(r.latency_buf_size, n_ports);
    r.latency_buf_write_pos = Buffer<int32_t>::make_scalar();
    r.latency_buf_write_pos() = 0;
    r.port_recording_latencies = decltype(r.port_recording_latencies)(n_ports);

    for(size_t i=0; i<n_loops; i++) {
        r.loops_hard_sync_mapping(i) = -1;
        r.loops_soft_sync_mapping(i) = -1;
        r.states_in(i) = Stopped;
        r.next_states_in(0, i) = r.next_states_in(1, i) = Stopped;
        r.next_states_countdown_in(0, i) = r.next_states_countdown_in(1, i) = -1;
        r.positions_in(i) = 0;
        r.lengths_in(i) = 0;
        r.loop_volumes(i) = 1.0f;
        r.loops_to_ports(i) = 0;

        for (size_t s = 0; s < r.max_n_events; s++) {
            r.event_recording_timestamps_out(s, i) = -1;
        }
        for (size_t s = 0; s < r.storage_in.dim(0).extent(); s++) {
            r.storage_in(s, i) = r.storage_out(s, i) = -((float)s);
        }
        for (size_t s = 0; s < process_samples; s++) {
            r.samples_out_per_loop(s, i) = std::numeric_limits<float>::min();
        }
    }

    for(size_t i=0; i<n_ports; i++) {
        r.passthroughs(i) = 1.0f;
        r.port_volumes(i) = 1.0f;
        r.port_input_override_map(i) = i;
        r.ports_muted(i) = 0;
        r.port_inputs_muted(i) = 0;
        r.port_recording_latencies(i) = 0;
        r.n_port_events(i) = 0;
        r.ports_to_mixed_outputs_map(i) = -1;

        for (size_t s = 0; s < process_samples; s++) {
            r.samples_in(s, i) = (float)(s+i);
            r.samples_out(s, i) = std::numeric_limits<float>::max();
        }
        for (size_t s = 0; s < r.latency_buf_size; s++) {
            r.latency_buf(s, i) = ((float)s+i) * 2.0f;
        }
        for (size_t s = 0; s < r.max_n_events; s++) {
            r.port_event_timestamps_in(s, i) = -1;
        }
    }

    for(size_t i=0; i<n_mixed_output_ports; i++) {
        for (size_t s = 0; s < process_samples; s++) {
            r.mixed_samples_out(s, i) = 10*s;
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
        bufs.latency_buf,
        bufs.latency_buf_write_pos(),
        bufs.port_recording_latencies,
        bufs.states_in,
        bufs.positions_in,
        bufs.lengths_in,
        bufs.next_states_in,
        bufs.next_states_countdown_in,
        bufs.storage_in,
        bufs.loops_to_ports,
        bufs.loops_hard_sync_mapping,
        bufs.loops_soft_sync_mapping,
        bufs.port_input_override_map,
        bufs.ports_to_mixed_outputs_map,
        bufs.n_mixed_output_ports,
        bufs.passthroughs,
        bufs.port_volumes,
        bufs.ports_muted,
        bufs.port_inputs_muted,
        bufs.loop_volumes,
        bufs.port_event_timestamps_in,
        bufs.n_port_events,
        bufs.process_samples,
        bufs.storage_in.dim(0).extent(),
        bufs.storage_lock,
        bufs.samples_out,
        bufs.mixed_samples_out,
        bufs.port_input_peaks,
        bufs.port_output_peaks,
        bufs.loop_output_peaks,
        bufs.latency_buf,
        bufs.latency_buf_write_pos,
        bufs.samples_out_per_loop,
        bufs.states_out,
        bufs.positions_out,
        bufs.lengths_out,
        bufs.next_states_out,
        bufs.next_states_countdown_out,
        bufs.event_recording_timestamps_out,
        bufs.storage_out
    );
}

suite loops_tests = []() {
    srand(1235930); // Seed random generator

    "1_stop"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Stopped;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        run_loops(bufs);
        expect(bufs.states_out(0) == Stopped);
        expect(bufs.positions_out(0) == 0_i); // Stopping always resets to 0
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
        };
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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

    "2_1_play_long"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 2048, 256);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 2048;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
        };

        for(size_t i=0; i<8; i++) {
            run_loops(bufs);
            expect(eq(((int)bufs.states_out(0)), Playing));
            expect(eq(bufs.positions_out(0), (256*(i+1)) % 2048)) << " at iteration " << i;
            expect(bufs.lengths_out(0) == 2048_i);
            
            for(size_t j=0; j<bufs.process_samples; j++) {
                float sample_out = bufs.samples_out(j,0);
                float storage = bufs.storage_in(j+256*i, 0);
                expect(eq(sample_out, storage)) << " at index " << j + i*256;
            }

            bufs.positions_in(0) = bufs.positions_out(0);
        }

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
    };

    "2_2_play_long_soft_synced_to_self"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 2048, 256);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 2048;
        bufs.loops_soft_sync_mapping(0) = 0;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
        };

        for(size_t i=0; i<8; i++) {
            run_loops(bufs);
            expect(eq(((int)bufs.states_out(0)), Playing));
            expect(eq(bufs.positions_out(0), (256*(i+1)) % 2048)) << " at iteration " << i;
            expect(bufs.lengths_out(0) == 2048_i);
            
            for(size_t j=0; j<bufs.process_samples; j++) {
                float sample_out = bufs.samples_out(j,0);
                float storage = bufs.storage_in(j+256*i, 0);
                expect(eq(sample_out, storage)) << " at index " << j + i*256;
            }

            bufs.positions_in(0) = bufs.positions_out(0);
        }

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
    };

    "2_3_play_and_wait_for_soft_sync"_test = []() {
        loops_buffers bufs = setup_buffers(2, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.states_in(1) = bufs.next_states_in(0, 1) = Stopped; // no soft sync generated
        bufs.loops_soft_sync_mapping(0) = 1;
        bufs.positions_in(0) = 8;
        bufs.lengths_in(0) = 11;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
        };
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(std::min<size_t>(i+8, 10), 0);
            expect(eq(sample_out, storage)) << " at index " << i;
        }
    };

    "2_4_play_wrap"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.positions_in(0) = 6;
        bufs.lengths_in(0) = 11;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
        };
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 3_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in((i+6) % 11, 0);
            expect(eq(sample_out, storage)) << " at index " << i;
        }
    };

    "2_5_play_with_delayed_soft_sync"_test = []() {
        loops_buffers bufs = setup_buffers(2, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.states_in(1) = bufs.next_states_in(0, 1) = Playing;
        bufs.loops_soft_sync_mapping(0) = 1;
        bufs.positions_in(0) = 8;
        bufs.positions_in(1) = 8;
        bufs.lengths_in(0) = 11;
        bufs.lengths_in(1) = 13;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
            bufs.samples_in(i, 1) = 0.0f;
        };
        for(int i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            bufs.storage_in(i, 1) = 0.0f;
        }
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 3_i);
        expect(bufs.positions_out(1) == 3_i);
        expect(bufs.lengths_out(0) == 11_i);
        expect(bufs.lengths_out(1) == 13_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<5; i++) {
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(std::min<size_t>(i+8, 10), 0);
            expect(eq(sample_out, storage)) << " at index " << i;
        }
        for(size_t i=6; i<bufs.process_samples; i++) {
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i-5, 0);
            expect(eq(sample_out, storage)) << " at index " << i;
        }
    };

    "2_6_play_mixed"_test = []() {
        loops_buffers bufs = setup_buffers(2, 2, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.states_in(1) = bufs.next_states_in(1, 0) = Playing;
        bufs.positions_in(0) = bufs.positions_in(1) = 2;
        bufs.lengths_in(0) = bufs.lengths_in(1) = 11;
        bufs.ports_to_mixed_outputs_map(0) = 0;
        bufs.ports_to_mixed_outputs_map(1) = 0;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
            bufs.samples_in(i, 1) = 0.0f;
        };
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_out = bufs.mixed_samples_out(i,0);
            float storage1 = bufs.storage_in(i+2, 0);
            float storage2 = bufs.storage_in(i+2, 1);
            expect(eq(sample_out, storage1 + storage2)) << " at index " << i;
        }
    };

    "3_play_muted"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = PlayingMuted;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), PlayingMuted));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 2;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 10_i);


        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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

    "4_1_record_from_0"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 0;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        // Note: after recording we have position == length.
        // That sounds invalid, but makes the most sense if recording is set to continue:
        // the new sample would be saved there and the length increased.
        expect(bufs.positions_out(0) == 8_i);
        expect(bufs.lengths_out(0) == 8_i);


        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            if (i < 8) {
                // Recorded
                float sample_in = bufs.samples_in(i, 0);
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

    "4_2_record_with_latency"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 2;
        bufs.latency_buf_write_pos() = 2;
        bufs.port_recording_latencies(0) = 4;
        Buffer<float, 2> latencybuf_copy = bufs.latency_buf;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 10_i);
        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            if (i >= 2 && i < 6) {
                // Recorded from latency buffer
                float sample_in = latencybuf_copy((2-4+i-2) % bufs.latency_buf_size, 0);
                expect(eq(storage_out, sample_in)) << " at index " << i;
            } else if (i >= 6 && i < 10) {
                // Recorded from samples in
                float sample_in = bufs.samples_in(i-2-4, 0);
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
        for(size_t i=0; i<bufs.latency_buf_size; i++) {
            float lbuf_sample = bufs.latency_buf(i, 0);
            if (i >= 2 && i < 10) {
                expect(eq(lbuf_sample, bufs.samples_in(i-2, 0)));
            } else {
                expect(eq(lbuf_sample, latencybuf_copy(i, 0))); // original value
            }
        }
    };

    "4_3_record_from_0_remapped"_test = []() {
        loops_buffers bufs = setup_buffers(1, 4, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 0;
        bufs.port_input_override_map(0) = 3;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 8_i);
        expect(bufs.lengths_out(0) == 8_i);


        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            if (i < 8) {
                // Recorded
                float sample_in = bufs.samples_in(i, 3);
                expect(eq(storage_out, sample_in)) << " at index " << i;
            } else {
                expect(eq(storage_out, storage_in)) << " at index " << i;
            }
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,3);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out, sample_in)) << " at index " << i;
        }
    };

    "5_passthrough"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Stopped;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        bufs.passthroughs(0) = 0.5f;
        run_loops(bufs);
        expect(bufs.states_out(0) == Stopped);
        expect(bufs.positions_out(0) == 0_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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

    "5_1_passthrough_remapped"_test = []() {
        loops_buffers bufs = setup_buffers(1, 2, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Stopped;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        bufs.port_input_override_map(1) = 0;
        bufs.passthroughs(0) = 0.5f;
        bufs.passthroughs(1) = 2.0f;
        for(size_t i=0; i<8; i++) {
            bufs.samples_in(i, 0) = 100.0f + (float)i;
        }
        run_loops(bufs);
        expect(bufs.states_out(0) == Stopped);
        expect(bufs.positions_out(0) == 0_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out_0 = bufs.samples_out(i,0);
            float sample_out_1 = bufs.samples_out(i,1);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out_0, sample_in*0.5f)) << " at index " << i;
            expect(eq(sample_out_1, sample_in*2.0f)) << " at index " << i;
        }
    };

    "6_loop_volume"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        bufs.loop_volumes(0) = 0.5f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = Playing;
        bufs.next_states_in(0, 0) = Stopped;
        bufs.next_states_countdown_in(0, 0) = 0;
        bufs.positions_in(0) = 10;
        bufs.lengths_in(0) = 16;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Stopped));
        expect(bufs.positions_out(0) == 0_i);
        expect(bufs.lengths_out(0) == 16);
        expect(bufs.next_states_countdown_out(0, 0) == -1);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = Playing;
        bufs.next_states_in(0, 0) = Recording;
        bufs.next_states_countdown_in(0, 0) = 0;
        bufs.positions_in(0) = 10;
        bufs.lengths_in(0) = 16;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 2_i);
        expect(bufs.lengths_out(0) == 2_i);
        expect(bufs.next_states_countdown_out(0, 0) == -1);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = Stopped;
        bufs.next_states_in(0, 0) = Recording;
        bufs.next_states_countdown_in(0, 0) = 0;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 0;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 8_i);
        expect(bufs.lengths_out(0) == 8_i);
        expect(bufs.next_states_countdown_out(0, 0) == -1);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = Recording;
        bufs.next_states_in(0, 0) = Playing;
        bufs.next_states_countdown_in(0, 0) = 0;
        bufs.positions_in(0) = 9;
        bufs.lengths_in(0) = 9;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 8_i);
        expect(bufs.lengths_out(0) == 9_i);
        expect(bufs.next_states_countdown_out(0, 0) == -1);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            // Recording should have stopped immediately
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            // Playback should have started immediately
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i%9, 0);
            // Recording, just passthrough
            expect(eq(sample_out, storage)) << " at index " << i;
        }
    };

    "11_soft_sync_play_to_record_self"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = Playing;
        bufs.next_states_in(0, 0) = Recording;
        bufs.next_states_countdown_in(0, 0) = 0;
        bufs.positions_in(0) = 5;
        bufs.lengths_in(0) = 10;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 3_i);
        expect(bufs.lengths_out(0) == 3_i);
        expect(bufs.next_states_countdown_out(0, 0) == -1);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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

    "12_no_soft_sync_play_to_record_self"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = Playing;
        bufs.next_states_in(0, 0) = Recording;
        bufs.next_states_countdown_in(0, 0) = 0;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 12;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 12_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in(i+2, 0);
            expect(eq(sample_out, storage)) << " at index " << i;          
        }
    };

    "13_hard_sync_stop"_test = []() {
        loops_buffers bufs = setup_buffers(2, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Stopped;
        bufs.states_in(1) = bufs.next_states_in(0, 1) = Playing;
        bufs.next_states_countdown_in(0, 0) = 0;
        bufs.positions_in(0) = 2;
        bufs.positions_in(1) = 5;
        bufs.lengths_in(0) = 11;
        bufs.lengths_in(1) = 16;
        bufs.loops_hard_sync_mapping(1) = 0;
        bufs.passthroughs(0) = 1.0f;
        run_loops(bufs);
        expect(bufs.states_out(1) == Stopped);
        expect(bufs.positions_out(1) == 0_i);
        expect(bufs.lengths_out(1) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,1);
            float storage_out = bufs.storage_out(i,1);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            expect(eq(sample_out, sample_in)) << " at index " << i;
        }
    };

    "13_1_hard_sync_soft_sync_play_to_record_self"_test = []() {
        loops_buffers bufs = setup_buffers(2, 2, 1, 16, 8);
        bufs.states_in(0) = Playing;
        bufs.next_states_in(0, 0) = Recording;
        bufs.next_states_countdown_in(0, 0) = 0;
        bufs.states_in(1) = Stopped; bufs.next_states_in(0, 1) = Stopped;
        bufs.positions_in(0) = 5;
        bufs.lengths_in(0) = 10;
        bufs.positions_in(1) = 2;
        bufs.lengths_in(1) = 2;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.loops_hard_sync_mapping(1) = 0;
        bufs.passthroughs(0) = 0.0f;
        bufs.passthroughs(1) = 0.0f;
        bufs.loop_volumes(1) = 0.5f;
        bufs.loops_to_ports(1) = 1;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(1)), Recording));
        expect(bufs.positions_out(1) == 3_i);
        expect(bufs.lengths_out(1) == 3_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,1);
            float storage_out = bufs.storage_out(i,1);
            if(i<3) {
                float sample_in = bufs.samples_in(i+5,1);
                expect(eq(storage_out, sample_in)) << " at index " << i;
            } else {
                expect(eq(storage_out, storage_in)) << " at index " << i;
            }
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_out = bufs.samples_out(i,1);
            if(i<5) {
                float storage = bufs.storage_in(i+5, 1);
                expect(eq(sample_out, storage * 0.5f)) << " at index " << i;
            } else {
                expect(eq(sample_out, 0.0_f)) << " at index " << i;
            }            
        }
    };

    "14_mute"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
        };
        bufs.ports_muted(0) = 1;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_out = bufs.samples_out(i,0);
            expect(eq(sample_out, 0.0f)) << " at index " << i;
        }
    };

    "15_mute_input"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Stopped;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        bufs.passthroughs(0) = 0.5f;
        bufs.port_inputs_muted(0) = 1;
        run_loops(bufs);
        expect(bufs.states_out(0) == Stopped);
        expect(bufs.positions_out(0) == 0_i);
        expect(bufs.lengths_out(0) == 11_i);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_out = bufs.samples_out(i,0);
            expect(eq(sample_out, 0.0f)) << " at index " << i;
        }
    };

    "16_soft_sync_play_to_stop_self_delayed"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = Playing;
        bufs.next_states_in(0, 0) = Stopped;
        bufs.next_states_countdown_in(0, 0) = 1;
        bufs.positions_in(0) = 10;
        bufs.lengths_in(0) = 16;
        bufs.loops_soft_sync_mapping(0) = 0;
        bufs.passthroughs(0) = 0.0f;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 2_i);
        expect(bufs.lengths_out(0) == 16);
        expect(bufs.next_states_countdown_out(0, 0) == 0);
        expect(bufs.next_states_out(0, 0) == Stopped);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            expect(eq(storage_out, storage_in)) << " at index " << i;
        }
        for(size_t i=0; i<bufs.process_samples; i++) {
            float sample_in = bufs.samples_in(i,0);
            float sample_out = bufs.samples_out(i,0);
            float storage = bufs.storage_in((i+10)%16, 0);
            // Still playing back
            expect(eq(sample_out, storage)) << " at index " << i;
        }
    };

    "17_discard_events_while_not_recording"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Playing;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 11;
        for(size_t i=0; i<bufs.process_samples; i++) {
            bufs.samples_in(i, 0) = 0.0f;
        };
        bufs.port_event_timestamps_in(0, 0) = 2;
        bufs.port_event_timestamps_in(1, 0) = 4;
        bufs.n_port_events(0) = 2;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Playing));
        expect(bufs.positions_out(0) == 10_i);
        expect(bufs.lengths_out(0) == 11_i);
        expect(bufs.event_recording_timestamps_out(0, 0) == -1);
        expect(bufs.event_recording_timestamps_out(1, 0) == -1);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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

    "18_record_events_from_0"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 0;
        bufs.port_event_timestamps_in(0, 0) = 3;
        bufs.port_event_timestamps_in(1, 0) = 5;
        bufs.port_event_timestamps_in(2, 0) = 6;
        bufs.n_port_events(0) = 2; // to test that the 3rd one is ignored
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 8_i);
        expect(bufs.lengths_out(0) == 8_i);
        expect(bufs.event_recording_timestamps_out(0, 0) == 3);
        expect(bufs.event_recording_timestamps_out(1, 0) == 5);
        expect(bufs.event_recording_timestamps_out(2, 0) == -1);

        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
            float storage_in = bufs.storage_in(i,0);
            float storage_out = bufs.storage_out(i,0);
            if (i < 8) {
                // Recorded
                float sample_in = bufs.samples_in(i, 0);
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

    "18_1_record_events_from_2"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 2;
        bufs.port_event_timestamps_in(0, 0) = 3;
        bufs.port_event_timestamps_in(1, 0) = 5;
        bufs.port_event_timestamps_in(2, 0) = 6;
        bufs.n_port_events(0) = 2; // to test that the 3rd one is ignored
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.lengths_out(0) == 10_i);
        expect(bufs.event_recording_timestamps_out(0, 0) == 5);
        expect(bufs.event_recording_timestamps_out(1, 0) == 7);
        expect(bufs.event_recording_timestamps_out(2, 0) == -1);
    };

    "18_2_record_events_from_0_remapped"_test = []() {
        loops_buffers bufs = setup_buffers(1, 4, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 0;
        bufs.port_input_override_map(0) = 3;
        bufs.port_event_timestamps_in(0, 3) = 3;
        bufs.port_event_timestamps_in(1, 3) = 5;
        bufs.port_event_timestamps_in(2, 3) = 6;
        bufs.n_port_events(3) = 2; // to test that the 3rd one is ignored
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 8_i);
        expect(bufs.lengths_out(0) == 8_i);
        expect(bufs.event_recording_timestamps_out(0, 0) == 3);
        expect(bufs.event_recording_timestamps_out(1, 0) == 5);
        expect(bufs.event_recording_timestamps_out(2, 0) == -1);
    };

    "19_record_with_storage_lock"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 2;
        bufs.lengths_in(0) = 3;
        bufs.storage_lock = 1;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 3_i);
        expect(bufs.lengths_out(0) == 3_i);


        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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

    "19_1_record_from_0_with_storage_lock"_test = []() {
        loops_buffers bufs = setup_buffers(1, 1, 1, 16, 8);
        bufs.states_in(0) = bufs.next_states_in(0, 0) = Recording;
        bufs.positions_in(0) = 0;
        bufs.lengths_in(0) = 0;
        bufs.storage_lock = 1;
        run_loops(bufs);
        expect(eq(((int)bufs.states_out(0)), Recording));
        expect(bufs.positions_out(0) == 0_i);
        expect(bufs.lengths_out(0) == 0_i);


        for(size_t i=0; i<bufs.storage_in.dim(0).extent(); i++) {
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