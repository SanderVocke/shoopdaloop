#include <HalideBuffer.h>
#include <algorithm>
#include <cmath>
#include <jack/types.h>
#include <jack/jack.h>
#include <jack/midiport.h>

#include "shoopdaloop_backend.h"
#include "shoopdaloop_loops.h"
#include "shoopdaloop_loops_profile.h"
#include <chrono>
#include <cstdlib>

#include <iostream>
#include <queue>
#include <string>
#include <vector>
#include <unistd.h>
#include <functional>
#include <atomic>
#include <thread>
#include <boost/lockfree/spsc_queue.hpp>
#include <memory>
#include <condition_variable>

#include <cmath>

using namespace Halide::Runtime;
using namespace std;

constexpr size_t slow_midi_port_queue_starting_size = 4096;

backend_features_t g_features;
typedef decltype(&shoopdaloop_loops) loops_fn;
loops_fn g_loops_fn = shoopdaloop_loops;

// Last written output index in in/out pairs (goes tick tock)
uint8_t g_last_written_output_buffer_tick_tock;

// State for each loop
Buffer<int8_t, 1> g_states[2];

// Planned states for each loop
Buffer<int8_t, 2> g_next_states[2];
Buffer<int8_t, 2> g_next_states_countdown[2];

// Position for each loop
Buffer<int32_t, 1> g_positions[2];

// Length for each loop
Buffer<int32_t, 1> g_lengths[2];

// Event handling
Buffer<int32_t, 1> g_n_port_events;
Buffer<int32_t, 2> g_port_event_timestamps_in;
Buffer<int32_t, 2> g_event_recording_timestamps_out;

// Sample input and output buffers
std::vector<float> g_samples_in_raw, g_samples_out_raw; //Use vectors here so we control the layout
Buffer<float, 2> g_samples_in, g_samples_out;

// Peak buffers
Buffer<float, 1> g_port_output_peaks;
Buffer<float, 1> g_port_input_peaks;
Buffer<float, 1> g_loop_output_peaks;

// Intermediate sample buffer which stores per loop (not port)
Buffer<float, 2> g_samples_out_per_loop;

// Loop storage
Buffer<float, 2> g_storage;

// Latency buffer
Buffer<float, 2> g_latency_buf;
Buffer<int32_t, 1> g_port_recording_latencies;
Buffer<int32_t, 0> g_latency_buf_write_pos;

// Map of loop idx to port idx
Buffer<int32_t, 1> g_loops_to_ports;

// Sync mappings
Buffer<int32_t, 1> g_loops_hard_sync_mapping;
Buffer<int32_t, 1> g_loops_soft_sync_mapping;

// Port input mappings
Buffer<int32_t, 1> g_port_input_mappings;

// Levels
Buffer<float, 1> g_passthroughs; // Per port
Buffer<float, 1> g_loop_volumes; // Per loop
Buffer<float, 1> g_port_volumes; // Per port
Buffer<int8_t, 1> g_ports_muted; // per port
Buffer<int8_t, 1> g_port_inputs_muted; // per port

std::vector<jack_port_t*> g_input_ports;
std::vector<jack_port_t*> g_output_ports;
std::vector<jack_nframes_t> g_input_port_latencies;
std::vector<jack_nframes_t> g_output_port_latencies;

size_t g_n_loops;
size_t g_n_ports;
size_t g_loop_len;
size_t g_buf_nframes;
size_t g_sample_rate;

std::vector<jack_default_audio_sample_t*> g_input_bufs, g_output_bufs;

jack_client_t* g_jack_client;

AbortCallback g_abort_callback;
UpdateCallback g_update_cb;

std::thread g_reporting_thread;

constexpr size_t g_max_midi_events_per_cycle = 256;

// A structure of atomic scalars is used to communicate the latest
// state back from the Jack processing thread to the main thread.
struct atomic_state {
    std::vector<std::atomic<loop_state_t>> states;
    std::vector<std::vector<std::atomic<loop_state_t>>> next_states;
    std::vector<std::vector<std::atomic<int8_t>>> next_states_countdown;
    std::vector<std::atomic<int32_t>> positions, lengths, latencies;
    std::vector<std::atomic<float>> passthroughs, loop_volumes, port_volumes, port_input_peaks, port_output_peaks, loop_output_peaks;
    std::vector<std::atomic<int8_t>> ports_muted, port_inputs_muted;
    std::vector<std::atomic<unsigned>> loop_n_output_events_since_last_update,
        port_n_input_events_since_last_update,
        port_n_output_events_since_last_update;
};
atomic_state g_atomic_state;

std::condition_variable g_terminate_cv;
std::mutex g_terminate_cv_m;

// A lock-free queue is used to pass commands to the audio
// processing thread in the form of functors.
constexpr unsigned command_queue_len = 1024;
boost::lockfree::spsc_queue<std::function<void()>> g_cmd_queue(command_queue_len);
void push_command(std::function<void()> cmd) {
    while (!g_cmd_queue.push(cmd)) { 
        std::this_thread::sleep_for(std::chrono::microseconds(100));
    }
}

// Another lock-free queue is used to pass benchmarking information
// from the processing thread to an optional reporter.
constexpr unsigned benchmark_queue_len = 10;
struct benchmark_info {
    float process_slow_midi_us;
    float request_buffers_us;
    float command_processing_us;
    float input_copy_us;
    float processing_us;
    float output_copy_us;
    float total_us;
};
boost::lockfree::spsc_queue<benchmark_info> g_benchmark_queue(benchmark_queue_len);

// Keep track of the slow midi ports that have been opened.
struct _slow_midi_port {
    jack_port_t* jack_port;
    std::string name;
    slow_midi_port_kind_t kind;
    std::vector<std::vector<uint8_t>> queue;
    SlowMIDIReceivedCallback maybe_rcv_callback;
};
std::vector<_slow_midi_port> g_slow_midi_ports;

extern "C" {

// Process the slow midi ports
void process_slow_midi_ports(jack_nframes_t nframes) {
    for(auto &port : g_slow_midi_ports) {

        if(port.jack_port) {
            auto buf = jack_port_get_buffer(port.jack_port, nframes);

            if(port.kind == Input) {
                auto n_events = jack_midi_get_event_count(buf);
                if(n_events > 0) {
                    std::cout << "Receiving MIDI!\n";
                }
                for(size_t i=0; i<n_events; i++) {
                    jack_midi_event_t e;
                    jack_midi_event_get(&e, buf, i);
                    std::vector<uint8_t> msg(e.size); // TODO: don't allocate here
                    memcpy((void*)msg.data(), (void*)e.buffer, e.size);
                    port.queue.push_back(msg);
                }
            } else {
                jack_midi_clear_buffer(buf);
                for(auto &elem : port.queue) {
                    jack_midi_event_write(buf, 0, (jack_midi_data_t*)elem.data(), elem.size());
                }
                port.queue.clear();
            }
        }
    }
}

// The latency update callback to be called by Jack.
void jack_latency_callback(jack_latency_callback_mode_t mode, void * arg) {
    if (mode == JackCaptureLatency) {
        for(size_t i=0; i<g_input_ports.size(); i++) {
            jack_latency_range_t range;
            jack_port_get_latency_range(g_input_ports[i], JackCaptureLatency, &range);
            g_input_port_latencies[i] = range.min;
            // TODO: possible race condition? Although this should be a read-only from
            // the other side.
            // TODO: rethink the whole latency approach
            //g_port_recording_latencies(i) = g_input_port_latencies[i] + g_output_port_latencies[i];
        }
    } else if (mode == JackPlaybackLatency) {
        for(size_t i=0; i<g_input_ports.size(); i++) {
            jack_latency_range_t range;
            jack_port_get_latency_range(g_output_ports[i], JackPlaybackLatency, &range);
            g_output_port_latencies[i] = range.min;
            // TODO: possible race condition? Although this should be a read-only from
            // the other side.
            // TODO: rethink the whole latency approach
            //g_port_recording_latencies(i) = g_input_port_latencies[i] + g_output_port_latencies[i];
        }
    }
}

// The processing callback to be called by Jack.
int jack_process (jack_nframes_t nframes, void *arg) {
    static auto last_measurement = std::chrono::high_resolution_clock::now();
    static size_t n_measurements = 0;
    static float slow_midi_time = 0.0,
                 get_buffers_time = 0.0,
                 cmd_time = 0.0,
                 in_time = 0.0,
                 proc_time = 0.0,
                 out_time = 0.0,
                 total_time = 0.0;
    static uint8_t tick = 0;
    static uint8_t tock = 1;

    jack_default_audio_sample_t *buf;

    auto start = std::chrono::high_resolution_clock::now();

    process_slow_midi_ports(nframes);

    auto did_slow_midi = std::chrono::high_resolution_clock::now();

    // Get buffers
    for(int i=0; i<g_input_ports.size(); i++) {
            g_input_bufs[i] =
                g_input_ports[i] ?
                (jack_default_audio_sample_t*) jack_port_get_buffer(g_input_ports[i], nframes) :
                nullptr;
            g_output_bufs[i] =
                g_output_ports[i] && jack_port_connected(g_output_ports[i]) ?
                (jack_default_audio_sample_t*) jack_port_get_buffer(g_output_ports[i], nframes) :
                nullptr;
    }

    auto got_buffers = std::chrono::high_resolution_clock::now();

    // Process commands
    std::function<void()> cmd;
    while(g_cmd_queue.pop(cmd)) {
        cmd();
    }

    auto did_cmds = std::chrono::high_resolution_clock::now();

    // Copy samples into the combined input buffer.
    for(int i=0; i<g_input_ports.size(); i++) {
        if(g_input_bufs[i]) {
            memcpy((void*)(&g_samples_in_raw[g_buf_nframes*i]), (void*)g_input_bufs[i], sizeof(float) * nframes);
        }
    }

    auto did_input = std::chrono::high_resolution_clock::now();

    // Execute the loop engine.
    if(g_loops_fn) {
        g_loops_fn(
            g_samples_in,
            g_latency_buf,
            g_latency_buf_write_pos(),
            g_port_recording_latencies,
            g_states[tick],
            g_positions[tick],
            g_lengths[tick],
            g_next_states[tick],
            g_next_states_countdown[tick],
            g_storage,
            g_loops_to_ports,
            g_loops_hard_sync_mapping,
            g_loops_soft_sync_mapping,
            g_port_input_mappings,
            g_passthroughs,
            g_port_volumes,
            g_ports_muted,
            g_port_inputs_muted,
            g_loop_volumes,
            g_port_event_timestamps_in,
            g_n_port_events,
            nframes,
            g_loop_len,
            g_samples_out,
            g_port_input_peaks,
            g_port_output_peaks,
            g_loop_output_peaks,
            g_latency_buf,
            g_latency_buf_write_pos,
            g_samples_out_per_loop,
            g_states[tock],
            g_positions[tock],
            g_lengths[tock],
            g_next_states[tock],
            g_next_states_countdown[tock],
            g_event_recording_timestamps_out,
            g_storage
        );
    }
    g_last_written_output_buffer_tick_tock = tock;

    // Debugging
    if (false) {
        for(int i=0; i<g_states[0].dim(0).extent(); i++) {
            if(g_states[tock](i) != g_states[tick](i)) {
                std::cout << "Loop " << i << ": " << (int)g_states[tick](i) << " -> "
                        << (int)g_states[tock](i) 
                        << " (-> " << (int)g_next_states[tock](0, i) << ")"
                        << std::endl;
            }
        }
    }

    tock = (tock + 1) % 2;
    tick = (tick + 1) % 2;

    auto did_process = std::chrono::high_resolution_clock::now();

    // Get output port buffers and copy samples into them.
    for(int i=0; i<g_output_ports.size(); i++) {
        if(g_output_bufs[i]) {
            memcpy((void*)g_output_bufs[i], (void*)(&g_samples_out_raw[g_buf_nframes*i]), sizeof(float) * nframes);
        }
    }

    // Copy states to atomic for read-out
    for(int i=0; i<g_n_loops; i++) {
        g_atomic_state.states[i] = (loop_state_t) g_states[g_last_written_output_buffer_tick_tock](i);
        g_atomic_state.lengths[i] = g_lengths[g_last_written_output_buffer_tick_tock](i);
        g_atomic_state.loop_volumes[i] = g_loop_volumes(i);
        g_atomic_state.positions[i] = g_positions[g_last_written_output_buffer_tick_tock](i);
        g_atomic_state.loop_output_peaks[i] = g_loop_output_peaks(i);
        g_atomic_state.next_states[i][0] = (loop_state_t) g_next_states[0](0, i);
        g_atomic_state.next_states[i][1] = (loop_state_t) g_next_states[1](1, i);
        g_atomic_state.next_states_countdown[i][0] = (loop_state_t) g_next_states_countdown[0](0, i);
        g_atomic_state.next_states_countdown[i][1] = (loop_state_t) g_next_states_countdown[1](1, i);
    }
    for(int i=0; i<g_n_ports; i++) {
        g_atomic_state.passthroughs[i] = g_passthroughs(i);
        g_atomic_state.port_volumes[i] = g_port_volumes(i);
        g_atomic_state.ports_muted[i] = g_ports_muted(i);
        g_atomic_state.port_inputs_muted[i] = g_port_inputs_muted(i);
        g_atomic_state.latencies[i] = g_port_recording_latencies(i);
        g_atomic_state.port_input_peaks[i] = g_port_input_peaks(i);
        g_atomic_state.port_output_peaks[i] = g_port_output_peaks(i);
    }

    auto did_output = std::chrono::high_resolution_clock::now();

    // Benchmarking
    slow_midi_time += std::chrono::duration_cast<std::chrono::microseconds>(did_slow_midi - start).count();
    get_buffers_time += std::chrono::duration_cast<std::chrono::microseconds>(got_buffers - did_slow_midi).count();
    cmd_time += std::chrono::duration_cast<std::chrono::microseconds>(did_cmds - got_buffers).count();
    in_time += std::chrono::duration_cast<std::chrono::microseconds>(did_input - did_cmds).count();
    proc_time += std::chrono::duration_cast<std::chrono::microseconds>(did_process - did_input).count();
    out_time += std::chrono::duration_cast<std::chrono::microseconds>(did_output - did_process).count();
    total_time += std::chrono::duration_cast<std::chrono::microseconds>(did_output - start).count();
    n_measurements++;

    if (std::chrono::duration_cast<std::chrono::milliseconds>(did_output - last_measurement).count() > 1000.0) {
        g_benchmark_queue.push(benchmark_info {
            .process_slow_midi_us = slow_midi_time / (float)n_measurements,
            .request_buffers_us = get_buffers_time / (float)n_measurements,
            .command_processing_us = cmd_time / (float)n_measurements,
            .input_copy_us = in_time / (float)n_measurements,
            .processing_us = proc_time / (float)n_measurements,
            .output_copy_us = out_time / (float)n_measurements,
            .total_us = total_time / (float)n_measurements
        });
        in_time = out_time = proc_time = total_time = cmd_time = get_buffers_time = slow_midi_time = 0.0f;
        n_measurements = 0;
        last_measurement = did_output;
    }

    return 0;
}

jack_client_t* initialize(
    unsigned n_loops,
    unsigned n_ports,
    float loop_len_seconds,
    unsigned *loops_to_ports_mapping,
    int *loops_hard_sync_mapping,
    int *loops_soft_sync_mapping,
    const char **input_port_names,
    const char **output_port_names,
    const char *client_name,
    unsigned latency_buf_size,
    UpdateCallback update_cb,
    AbortCallback abort_cb,
    backend_features_t features
) {
    g_n_loops = n_loops;
    g_n_ports = n_ports;
    g_features = features;

    switch(g_features) {
        case Profiling:
            g_loops_fn = shoopdaloop_loops_profile;
            break;
        default:
        break;
    }

    // Create a JACK client
    jack_status_t status;
    g_jack_client = jack_client_open(client_name, JackNullOption, &status, nullptr);
    if(g_jack_client == nullptr) {
        std::cerr << "Backend: given a null client." << std::endl;
        return nullptr;
    }

    // Get JACK buffer size and sample rate
    g_buf_nframes = 
        jack_port_type_get_buffer_size(g_jack_client, JACK_DEFAULT_AUDIO_TYPE) / sizeof(jack_default_audio_sample_t);
    g_sample_rate =
        (size_t) jack_get_sample_rate(g_jack_client);
    std::cout << "Backend: JACK: buf size " << g_buf_nframes << " @ " << g_sample_rate << "samples/s" << std::endl;

    g_loop_len = (size_t)((float) g_sample_rate * loop_len_seconds);
    g_abort_callback = nullptr;
    g_last_written_output_buffer_tick_tock = 0;
    g_cmd_queue.reset();

    // Allocate buffers
    g_states[0] = Buffer<int8_t, 1>(n_loops);
    g_states[1] = Buffer<int8_t, 1>(n_loops);
    g_next_states[0] = Buffer<int8_t, 2>(2, n_loops);
    g_next_states[1] = Buffer<int8_t, 2>(2, n_loops);
    g_next_states_countdown[0] = Buffer<int8_t, 2>(2, n_loops);
    g_next_states_countdown[1] = Buffer<int8_t, 2>(2, n_loops);
    g_positions[0] = Buffer<int32_t, 1>(n_loops);
    g_positions[1] = Buffer<int32_t, 1>(n_loops);
    g_lengths[0] = Buffer<int32_t, 1>(n_loops);
    g_lengths[1] = Buffer<int32_t, 1>(n_loops);
    g_samples_in_raw.resize(n_ports*g_buf_nframes);
    g_samples_out_raw.resize(n_ports*g_buf_nframes);
    g_samples_out_per_loop = Buffer<float, 2>(g_buf_nframes, n_loops);
    g_storage = Buffer<float, 2>(g_loop_len, n_loops);
    g_loops_to_ports = Buffer<int32_t, 1>(n_loops);
    g_loops_hard_sync_mapping = Buffer<int32_t, 1>(n_loops);
    g_loops_soft_sync_mapping = Buffer<int32_t, 1>(n_loops);
    g_passthroughs = Buffer<float, 1>(n_ports);
    g_port_volumes = Buffer<float, 1>(n_ports);
    g_ports_muted = Buffer<int8_t, 1>(n_ports);
    g_port_inputs_muted = Buffer<int8_t, 1>(n_ports);
    g_loop_volumes = Buffer<float, 1>(n_loops);
    g_port_input_mappings = Buffer<int32_t, 1>(n_ports);
    g_latency_buf_write_pos = Buffer<int32_t>::make_scalar();
    g_latency_buf_write_pos() = 0;
    g_port_recording_latencies = Buffer<int32_t>(n_ports);
    g_latency_buf = Buffer<float, 2>(latency_buf_size, n_ports);
    g_port_input_peaks = Buffer<float, 1>(n_ports);
    g_port_output_peaks = Buffer<float, 1>(n_ports);
    g_loop_output_peaks = Buffer<float, 1>(n_loops);
    g_port_event_timestamps_in = Buffer<int32_t, 2>(g_max_midi_events_per_cycle, n_ports);
    g_n_port_events = Buffer<int32_t, 1>(n_ports);
    g_event_recording_timestamps_out = Buffer<int32_t, 2>(g_max_midi_events_per_cycle, n_loops);

    g_samples_in = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)g_samples_in_raw.data(),
        g_buf_nframes, n_ports
    );
    g_samples_out = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)g_samples_out_raw.data(),
        g_buf_nframes, n_ports
    );

    // Allocate atomic state
    g_atomic_state.states = std::vector<std::atomic<loop_state_t>>(n_loops);
    g_atomic_state.lengths = std::vector<std::atomic<int32_t>>(n_loops);
    g_atomic_state.loop_volumes = std::vector<std::atomic<float>>(n_loops);
    g_atomic_state.port_volumes = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.passthroughs = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.positions = std::vector<std::atomic<int32_t>>(n_loops);
    g_atomic_state.ports_muted = std::vector<std::atomic<int8_t>>(n_ports);
    g_atomic_state.port_inputs_muted = std::vector<std::atomic<int8_t>>(n_ports);
    g_atomic_state.latencies = std::vector<std::atomic<int32_t>>(n_ports);
    g_atomic_state.port_input_peaks = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.port_output_peaks = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.loop_output_peaks = std::vector<std::atomic<float>>(n_loops);
    g_atomic_state.next_states = std::vector<std::vector<std::atomic<loop_state_t>>>(n_loops);
    g_atomic_state.next_states_countdown = std::vector<std::vector<std::atomic<int8_t>>>(n_loops);
    for(size_t i=0; i<n_loops; i++) {
        g_atomic_state.next_states[i] = std::vector<std::atomic<loop_state_t>>(2);
        g_atomic_state.next_states_countdown[i] = std::vector<std::atomic<int8_t>>(2);
    }
    g_atomic_state.loop_n_output_events_since_last_update = std::vector<std::atomic<unsigned>>(n_loops);
    g_atomic_state.port_n_output_events_since_last_update = std::vector<std::atomic<unsigned>>(n_ports);
    g_atomic_state.port_n_input_events_since_last_update = std::vector<std::atomic<unsigned>>(n_ports);

    // Initialize loops
    for(size_t i=0; i<g_n_loops; i++) {
        g_states[0](i) = g_states[1](i) = Stopped;
        g_next_states[0](0, i) = g_next_states[0](1, i) = Stopped;
        g_next_states[1](0, i) = g_next_states[1](1, i) = Stopped;
        g_next_states_countdown[0](0, i) = g_next_states_countdown[0](1, i) = -1;
        g_next_states_countdown[1](0, i) = g_next_states_countdown[1](1, i) = -1;
        g_positions[0](i) = g_positions[1](i) = 0;
        g_lengths[0](i) = g_lengths[1](i) = 0;
        g_loops_to_ports(i) = loops_to_ports_mapping[i];
        g_loops_hard_sync_mapping(i) = loops_hard_sync_mapping[i] < 0 ?
            i : loops_hard_sync_mapping[i];
        g_loops_soft_sync_mapping(i) = loops_soft_sync_mapping[i];
        g_loop_volumes(i) = 1.0f;
        g_atomic_state.loop_n_output_events_since_last_update[i] = 0;
    }
    
    // Initialize ports
    g_input_bufs = std::vector<jack_default_audio_sample_t*>(n_ports);
    g_output_bufs = std::vector<jack_default_audio_sample_t*>(n_ports);
    g_input_ports.clear();
    g_output_ports.clear();
    g_input_port_latencies.clear();
    g_output_port_latencies.clear();
    for(size_t i=0; i<n_ports; i++) {
        g_port_input_mappings(i) = i;
        g_input_ports.push_back(jack_port_register(g_jack_client, input_port_names[i], JACK_DEFAULT_AUDIO_TYPE, JackPortIsInput, 0));
        g_output_ports.push_back(jack_port_register(g_jack_client, output_port_names[i], JACK_DEFAULT_AUDIO_TYPE, JackPortIsOutput, 0));
        g_input_port_latencies.push_back(0);
        g_output_port_latencies.push_back(0);
        g_port_recording_latencies(i) = 0;

        if(g_input_ports[i] == nullptr) {
            std::cerr << "Backend: Got null port" << std::endl;
            return nullptr;
        }
        if(g_output_ports[i] == nullptr) {
            std::cerr << "Backend: Got null port" << std::endl;
            return nullptr;
        }

        g_passthroughs(i) = 1.0f;
        g_port_volumes(i) = 1.0f;
        g_ports_muted(i) = 0;
        g_port_inputs_muted(i) = 0;
        g_n_port_events(i) = 0;

        g_atomic_state.port_n_input_events_since_last_update[i] = 0;
        g_atomic_state.port_n_output_events_since_last_update[i] = 0;
    }

    // Set the JACK process and latency callback
    jack_set_process_callback(g_jack_client, jack_process, nullptr);
    jack_set_latency_callback(g_jack_client, jack_latency_callback, nullptr);

    if(jack_activate(g_jack_client)) {
        std::cerr << "Backend: Failed to activate client" << std::endl;
        return nullptr;
    }

    g_update_cb = update_cb;

    if(features == Profiling) {
        g_reporting_thread = std::thread([]() {
            while(true) {
                std::unique_lock<std::mutex> lock(g_terminate_cv_m);
                if(g_terminate_cv.wait_for(lock, std::chrono::milliseconds(500)) == std::cv_status::no_timeout) {
                    // Terminated.
                    std::cout << "Reporting thread terminating." << std::endl;
                    return nullptr;
                }

                size_t popped = 0;
                benchmark_info info;
                while(g_benchmark_queue.pop(info)) { popped++; };
                if (popped > 0) {
                    int period = (int)(1.0f / (float)g_sample_rate * (float)g_buf_nframes * 1000000.0f);
                    float percent_of_period = (float) info.total_us / (float) period * 100.0f;
                    std::cout
                        << "Backend: total, slowmid, getbuf, cmd, in, proc, out (us)" << std::endl
                        << "         "
                        << info.total_us << " "
                        << info.process_slow_midi_us << " "
                        << info.request_buffers_us << " "
                        << info.command_processing_us << " "
                        << info.input_copy_us << " "
                        << info.processing_us << " "
                        << info.output_copy_us << std::endl
                        << "         period: " << g_buf_nframes << " @ " << g_sample_rate << " -> " << period << " us"
                        << " (using " << percent_of_period << "%)\n";
                }
            }
        });
    }

    return g_jack_client;
}

int do_loop_action(
    unsigned *loop_idxs,
    unsigned n_loop_idxs,
    loop_action_t action,
    float* maybe_args,
    unsigned n_args,
    unsigned with_soft_sync
) {
    std::function<void()> cmd = nullptr;
    std::vector<unsigned> idxs(n_loop_idxs);
    memcpy((void*)idxs.data(), (void*)loop_idxs, n_loop_idxs*sizeof(unsigned));

    auto check_args = [n_args](int n_needed) {
        if (n_args != n_needed) {
            throw std::runtime_error("do_loop_action: incorrect # of args");
        }
    };

    auto apply_state = [with_soft_sync](std::vector<unsigned> loops, loop_state_t state, int delay=0, bool is_next_next_state=false) {
        for(auto const& loop: loops) {
            int x_idx = is_next_next_state ? 1 : 0;
            g_next_states[0](x_idx, loop) = g_next_states[1](x_idx, loop) = state;
            g_next_states_countdown[0](x_idx, loop) = g_next_states_countdown[1](x_idx, loop) = delay;
            if(!with_soft_sync) {
                g_states[0](loop) = g_states[1](loop) = state;
            }
        }
    };

    int arg1_i = n_args >= 1 ? std::round(maybe_args[0]) : 0;
    int arg2_i = n_args >= 2 ? std::round(maybe_args[1]) : 0;
    int arg1_f = n_args >= 1 ? maybe_args[0] : 0;
    int arg2_f = n_args >= 2 ? maybe_args[1] : 0;

    switch(action) {
        case DoRecord:
            check_args(1);
            cmd = [idxs, apply_state, arg1_i]() {
                apply_state(idxs, Recording, arg1_i);
                for(auto const& idx: idxs) {
                    g_positions[0](idx) = g_positions[1](idx) = 0;
                    g_lengths[0](idx) = g_lengths[1](idx) = 0;
                }
            };
            break;
        case DoRecordNCycles:
            check_args(2);
            cmd = [idxs, apply_state, arg1_i, arg2_i]() {
                apply_state(idxs, Recording, arg1_i);
                apply_state(idxs, Playing, arg2_i-1, true);
                for(auto const& idx: idxs) {
                    g_positions[0](idx) = g_positions[1](idx) = 0;
                    g_lengths[0](idx) = g_lengths[1](idx) = 0;
                }
            };
            break;
        case DoPlay:
            check_args(1);
            cmd = [idxs, apply_state, arg1_i]() {
                apply_state(idxs, Playing, arg1_i);
            };
            break;
        case DoPlayMuted:
            check_args(1);
            cmd = [idxs, apply_state, arg1_i]() {
                apply_state(idxs, PlayingMuted, arg1_i);
            };
            break;
        case DoStop:
            check_args(1);
            cmd = [idxs, apply_state, arg1_i]() {
                apply_state(idxs, Stopped, arg1_i);
            };
            break;
        case DoClear:
            check_args(0);
            cmd = [idxs, apply_state]() {
                apply_state(idxs, Stopped);
                for(auto const& idx: idxs) {
                    g_positions[0](idx) = g_positions[1](idx) = 0;
                    g_lengths[0](idx) = g_lengths[1](idx) = 0;
                }
            };
            break;
        case SetLoopVolume:
            check_args(1);
            cmd = [idxs, arg1_f]() {
                for (auto const& idx: idxs) {
                    g_loop_volumes(idx) = arg1_f;
                }
            };
            break;
        default:
        break;
    }

    if(cmd) { push_command(cmd); }
    return 0;
}

int do_port_action(
    unsigned port_idx,
    port_action_t action,
    float maybe_arg
) {
    std::function<void()> cmd = nullptr;

    switch(action) {
        case DoMute:
            cmd = [port_idx]() {
                g_ports_muted(port_idx) = 1;
            };
            break;
        case DoUnmute:
            cmd = [port_idx]() {
                g_ports_muted(port_idx) = 0;
            };
            break;
        case DoMuteInput:
            cmd = [port_idx]() {
                g_port_inputs_muted(port_idx) = 1;
            };
            break;
        case DoUnmuteInput:
            cmd = [port_idx]() {
                g_port_inputs_muted(port_idx) = 0;
            };
            break;
        case SetPortPassthrough:
            cmd = [port_idx, maybe_arg]() {
                g_passthroughs(port_idx) = maybe_arg;
            };
            break;
        case SetPortVolume:
            cmd = [port_idx, maybe_arg]() {
                g_port_volumes(port_idx) = maybe_arg;
            };
            break;
        default:
        break;
    }

    if(cmd) { push_command(cmd); }
    return 0;
}

void request_update() {
    // Allocate memory for a copy of the relevant state
    std::vector<loop_state_t> states(g_n_loops), next_states(g_n_loops);
    std::vector<int32_t> positions(g_n_loops), lengths(g_n_loops), latencies(g_n_ports), next_state_countdowns(g_n_loops);
    std::vector<float> passthroughs(g_n_ports), loop_volumes(g_n_loops), port_volumes(g_n_ports);
    std::vector<int8_t> ports_muted(g_n_ports), port_inputs_muted(g_n_ports);
    std::vector<unsigned> loop_n_output_events(g_n_loops), port_n_input_events(g_n_ports), port_n_output_events(g_n_ports);

    // Copy the state
    for(int i=0; i<g_n_loops; i++) {
        states[i] = g_atomic_state.states[i];
        // TODO
        next_states[i] = g_atomic_state.next_states[i][0];
        next_state_countdowns[i] = g_atomic_state.next_states_countdown[i][0];
        positions[i] = g_atomic_state.positions[i];
        lengths[i] = g_atomic_state.lengths[i];
        loop_volumes[i] = g_atomic_state.loop_volumes[i];
        loop_n_output_events[i] = g_atomic_state.loop_n_output_events_since_last_update[i];
        // Reset counters
        g_atomic_state.loop_n_output_events_since_last_update[i] = 0;
    }
    for(int i=0; i<g_n_ports; i++) {
        passthroughs[i] = g_atomic_state.passthroughs[i];
        port_volumes[i] = g_atomic_state.port_volumes[i];
        ports_muted[i] = g_atomic_state.ports_muted[i];
        port_inputs_muted[i] = g_atomic_state.port_inputs_muted[i];
        latencies[i] = g_atomic_state.latencies[i];
        port_n_input_events[i] = g_atomic_state.port_n_input_events_since_last_update[i];
        port_n_output_events[i] = g_atomic_state.port_n_output_events_since_last_update[i];
        // Reset counters
        g_atomic_state.port_n_input_events_since_last_update[i] = 0;
        g_atomic_state.port_n_output_events_since_last_update[i] = 0;
    }
    
    if (g_update_cb) {
        int r = g_update_cb(
            g_n_loops,
            g_n_ports,
            g_sample_rate,
            states.data(),
            next_states.data(),
            next_state_countdowns.data(),
            lengths.data(),
            positions.data(),
            loop_volumes.data(),
            port_volumes.data(),
            passthroughs.data(),
            latencies.data(),
            ports_muted.data(),
            port_inputs_muted.data(),
            g_loop_output_peaks.data(),
            g_port_output_peaks.data(),
            g_port_input_peaks.data(),
            loop_n_output_events.data(),
            port_n_input_events.data(),
            port_n_output_events.data()
        );
    }
}

int load_loop_data(
    unsigned loop_idx,
    unsigned len,
    float *data
) {
    std::atomic<bool> finished = false;
    push_command([loop_idx, &finished]() {
        g_next_states_countdown[0](0, loop_idx) = g_next_states_countdown[0](1, loop_idx) =
            g_next_states_countdown[1](0, loop_idx) = g_next_states_countdown[1](1, loop_idx) = -1;
        g_states[0](loop_idx) = g_states[1](loop_idx) = Stopped;
        g_positions[0](loop_idx) = g_positions[1](loop_idx) = 0;
        finished = true;
    });

    while(!finished && g_loops_fn) {}
    for(size_t idx = 0; idx < len; idx++) {
        g_storage(idx, loop_idx) = data[idx];
    }

    finished = false;
    push_command([len, loop_idx, &finished]() {
        g_lengths[0](loop_idx) = g_lengths[1](loop_idx) = len;
        finished = true;
    });
    while(!finished && g_loops_fn) {}

    return 0;
}

unsigned get_loop_data(
    unsigned loop_idx,
    float ** data_out,
    unsigned do_stop
) {
    if (do_stop) {
        std::atomic<bool> finished = false;
        push_command([loop_idx, &finished]() {
            g_next_states_countdown[0](0, loop_idx) = g_next_states_countdown[0](1, loop_idx) =
                g_next_states_countdown[1](0, loop_idx) = g_next_states_countdown[1](1, loop_idx) = -1;
            g_states[0](loop_idx) = g_states[1](loop_idx) = Stopped;
            g_positions[0](loop_idx) = g_positions[1](loop_idx) = 0;
            finished = true;
        });
        while(!finished && g_loops_fn) {}
    }

    auto len = g_lengths[g_last_written_output_buffer_tick_tock](loop_idx);
    auto data = (float*) malloc(sizeof(float) * len);
    for(size_t idx = 0; idx < len; idx++) {
        data[idx] = g_storage(idx, loop_idx);
    }
    *data_out = data;

    return len;
}

void terminate() {
    jack_client_close(g_jack_client);
    g_terminate_cv.notify_all();
    if (g_reporting_thread.joinable()) {
        g_reporting_thread.join();
    }
    g_cmd_queue.reset();

    // TODO: explicit profiler print, can be removed if we solve segfault
    static void *const user_context = nullptr;
    halide_profiler_report(user_context);
}

jack_port_t* get_port_output_handle(unsigned port_idx) {
    return g_output_ports[port_idx];
}

jack_port_t* get_port_input_handle(unsigned port_idx) {
    return g_input_ports[port_idx];
}

void remap_port_input(unsigned port, unsigned input_source) {
    g_port_input_mappings(port) = input_source;
}

void reset_port_input_remapping(unsigned port) {
    g_port_input_mappings(port) = port;
}

jack_port_t *create_slow_midi_port(
    const char* name,
    slow_midi_port_kind_t kind
) {
    auto jport = jack_port_register(g_jack_client, name, JACK_DEFAULT_MIDI_TYPE,
                                    kind == Input ? JackPortIsInput : JackPortIsOutput, 0);
    if (jport) {
        _slow_midi_port port {
            .jack_port = jport,
            .name = std::string(name),
            .kind = kind,
            .queue = std::vector<std::vector<uint8_t>>()
        };
        port.queue.reserve(slow_midi_port_queue_starting_size);
        g_slow_midi_ports.push_back(port);
        return jport;
    }

    std::cout << "Backend: failed to create slow MIDI port." << std::endl;
    return nullptr;
}

void set_slow_midi_port_received_callback(
    jack_port_t *port,
    SlowMIDIReceivedCallback callback
) {
    for(auto &it : g_slow_midi_ports) {
        if(it.jack_port == port) {
            it.maybe_rcv_callback = callback;
        }
    }
}

void destroy_slow_midi_port(jack_port_t *port) {
    if (port) {
        jack_port_unregister(g_jack_client, port);
    }

    for(auto it = g_slow_midi_ports.begin(); it != g_slow_midi_ports.end(); it++) {
        if(it->jack_port == port) {
            g_slow_midi_ports.erase(it);
            break;
        }
    }
}

void send_slow_midi(
    jack_port_t *port,
    uint8_t len,
    uint8_t *data
) {
    for(auto &it : g_slow_midi_ports) {
        if(it.jack_port == port) {
            std::vector<uint8_t> msg(len);
            memcpy((void*)msg.data(), (void*)data, len);
            it.queue.push_back(msg);
            break;
        }
    }
}

void process_slow_midi() {
    for (auto &it : g_slow_midi_ports) {
        if(it.kind == Input && it.queue.size() > 0) {
            if(it.maybe_rcv_callback) {
                for (auto &elem : it.queue) {
                    if(it.maybe_rcv_callback) {
                        std::cout << "rcv cb" << std::endl;
                        it.maybe_rcv_callback(it.jack_port, elem.size(), elem.data());
                    }
                }
            }
            it.queue.clear();
        }
    }
}

void shoopdaloop_free(void* ptr) {
    free (ptr);
}

unsigned get_sample_rate() {
    return g_sample_rate;
}

} //extern "C"