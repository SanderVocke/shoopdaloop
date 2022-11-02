#include <HalideBuffer.h>
#include <cmath>
#include <jack/types.h>
extern "C" {
#include <jack/jack.h>
}

#include "shoopdaloop_backend.hpp"
#include "shoopdaloop_loops.h"
#include <chrono>
#include <cstdlib>

#include <iostream>
#include <string>
#include <vector>
#include <unistd.h>
#include <functional>
#include <atomic>
#include <thread>

using namespace Halide::Runtime;
using namespace std;

// State for each loop
Buffer<int8_t, 1> g_states;

// Position for each loop
Buffer<int32_t, 1> g_positions;

// Length for each loop
Buffer<int32_t, 1> g_lengths;

// Sample input and output buffers
std::vector<float> g_samples_in_raw, g_samples_out_raw; //Use vectors here so we control the layout
Buffer<float, 2> g_samples_in, g_samples_out;

// Intermediate sample buffer which stores per loop (not port)
Buffer<float, 2> g_samples_out_per_loop;

// Loop storage
Buffer<float, 2> g_storage;

// Map of loop idx to port idx
Buffer<int32_t, 1> g_loops_to_ports;

// Levels
Buffer<float, 1> g_passthroughs; // Per port
Buffer<float, 1> g_loop_volumes; // Per loop
Buffer<float, 1> g_port_volumes; // Per port

std::vector<jack_port_t*> g_input_ports;
std::vector<jack_port_t*> g_output_ports;

size_t g_n_loops;
size_t g_n_ports;
size_t g_loop_len;
size_t g_buf_size;

jack_client_t* g_jack_client;

UpdateCallback g_update_callback;
AbortCallback g_abort_callback;

std::thread g_reporting_thread;

// A structure of atomic scalars is used to communicate the latest
// state back from the Jack processing thread to the main thread.
struct atomic_state {
    std::vector<std::atomic<loop_state_t>> states;
    std::vector<std::atomic<int32_t>> positions, lengths;
    std::vector<std::atomic<float>> passthroughs, loop_volumes, port_volumes;
};
atomic_state g_atomic_state;

bool finished = false;

extern "C" {

// The processing callback to be called by Jack.
int jack_process (jack_nframes_t nframes, void *arg) {
    static auto last_measurement = std::chrono::high_resolution_clock::now();
    static size_t n_measurements = 0;
    static float in_time = 0.0, proc_time = 0.0, out_time = 0.0;

    jack_default_audio_sample_t *buf;

    auto start = std::chrono::high_resolution_clock::now();

    // Get input port buffers and copy samples into the combined input buffer.
    for(int i=0; i<g_input_ports.size(); i++) {
        if(g_input_ports[i]) {
            buf = (jack_default_audio_sample_t*) jack_port_get_buffer(g_input_ports[i], nframes);
            if (buf) {
                memcpy((void*)(&g_samples_in_raw[g_buf_size*i]), (void*)buf, sizeof(float) * nframes);
            }
        }
    }

    auto did_input = std::chrono::high_resolution_clock::now();

    // Execute the loop engine.
    shoopdaloop_loops(
        g_samples_in,
        g_states,
        g_positions,
        g_lengths,
        g_storage,
        g_loops_to_ports,
        g_passthroughs,
        g_port_volumes,
        g_loop_volumes,
        nframes,
        g_loop_len,
        g_samples_out,
        g_samples_out_per_loop,
        g_states,
        g_positions,
        g_lengths,
        g_storage
    );

    auto did_process = std::chrono::high_resolution_clock::now();

    // Get output port buffers and copy samples into them.
    for(int i=0; i<g_output_ports.size(); i++) {
        if(g_output_ports[i]) {
            buf = (jack_default_audio_sample_t*) jack_port_get_buffer(g_output_ports[i], nframes);
            if (buf) {
                memcpy((void*)buf, (void*)(&g_samples_out_raw[g_buf_size*i]), sizeof(float) * nframes);
            }
        }
    }

    // Copy states to atomic for read-out
    for(int i=0; i<g_n_loops; i++) {
        g_atomic_state.states[i] = (loop_state_t) g_states(i);
        g_atomic_state.lengths[i] = g_lengths(i);
        g_atomic_state.loop_volumes[i] = g_loop_volumes(i);
        g_atomic_state.positions[i] = g_positions(i);
    }
    for(int i=0; i<g_n_ports; i++) {
        g_atomic_state.passthroughs[i] = g_passthroughs(i);
        g_atomic_state.port_volumes[i] = g_port_volumes(i);
    }

    auto did_output = std::chrono::high_resolution_clock::now();

    // Benchmarking
    in_time += std::chrono::duration_cast<std::chrono::microseconds>(did_input - start).count();
    proc_time += std::chrono::duration_cast<std::chrono::microseconds>(did_process - did_input).count();
    out_time += std::chrono::duration_cast<std::chrono::microseconds>(did_output - did_process).count();
    n_measurements++;

    if (std::chrono::duration_cast<std::chrono::milliseconds>(did_output - last_measurement).count() > 1000.0) {
        std::cout << in_time/(float)n_measurements 
                  << " " << proc_time/(float)n_measurements
                  << " " << out_time/(float)n_measurements
                  << std::endl;
        in_time = out_time = proc_time = 0.0f;
        n_measurements = 0;
        last_measurement = did_output;
    }

    return 0;
}

int initialize(
    unsigned n_loops,
    unsigned n_ports,
    unsigned loop_len_samples,
    unsigned loop_channels,
    unsigned *loops_to_ports_mapping,
    const char **input_port_names,
    const char **output_port_names,
    const char *client_name,
    unsigned update_period_ms,
    UpdateCallback update_cb,
    AbortCallback abort_cb
) {
    g_n_loops = n_loops;
    g_n_ports = n_ports;
    g_loop_len = loop_len_samples;

    // Create a JACK client
    jack_status_t status;
    g_jack_client = jack_client_open(client_name, JackNullOption, &status, nullptr);
    if(g_jack_client == nullptr) {
        std::cerr << "Backend: given a null client." << std::endl;
        return 1;
    }

    // Get JACK buffer size
    g_buf_size = 
        jack_port_type_get_buffer_size(g_jack_client, JACK_DEFAULT_AUDIO_TYPE) / sizeof(jack_default_audio_sample_t);
    std::cout << "Backend: JACK buffer size: " << g_buf_size << " samples" << std::endl;

    // Allocate buffers
    g_states = Buffer<int8_t, 1>(n_loops);
    g_positions = Buffer<int32_t, 1>(n_loops);
    g_lengths = Buffer<int32_t, 1>(n_loops);
    g_samples_in_raw.resize(n_ports*g_buf_size);
    g_samples_out_raw.resize(n_ports*g_buf_size);
    g_samples_out_per_loop = Buffer<float, 2>(g_buf_size, n_loops);
    g_storage = Buffer<float, 2>(g_loop_len, n_loops);
    g_loops_to_ports = Buffer<int32_t, 1>(n_loops);
    g_passthroughs = Buffer<float, 1>(n_ports);
    g_port_volumes = Buffer<float, 1>(n_ports);
    g_loop_volumes = Buffer<float, 1>(n_loops);

    g_samples_in = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)g_samples_in_raw.data(),
        g_buf_size, n_ports
    );
    g_samples_out = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)g_samples_out_raw.data(),
        g_buf_size, n_ports
    );

    // Allocate atomic state
    g_atomic_state.states = std::vector<std::atomic<loop_state_t>>(n_loops);
    g_atomic_state.lengths = std::vector<std::atomic<int32_t>>(n_loops);
    g_atomic_state.loop_volumes = std::vector<std::atomic<float>>(n_loops);
    g_atomic_state.port_volumes = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.passthroughs = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.positions = std::vector<std::atomic<int32_t>>(n_loops);

    // Set the JACK process callback
    jack_set_process_callback(g_jack_client, jack_process, 0);

    // Initialize loops
    for(size_t i=0; i<g_n_loops; i++) {
        g_states(i) = Paused;
        g_positions(i) = 0;
        g_lengths(i) = 0;
        g_loops_to_ports(i) = loops_to_ports_mapping[i];
        g_loop_volumes(i) = 1.0f;
    }

    // Initialize ports
    for(size_t i=0; i<n_ports; i++) {
        g_input_ports.push_back(jack_port_register(g_jack_client, input_port_names[i], JACK_DEFAULT_AUDIO_TYPE, JackPortIsInput, 0));
        g_output_ports.push_back(jack_port_register(g_jack_client, output_port_names[i], JACK_DEFAULT_AUDIO_TYPE, JackPortIsOutput, 0));

        if(g_input_ports[i] == nullptr) {
            std::cerr << "Backend: Got null port" << std::endl;
            return 1;
        }
        if(g_output_ports[i] == nullptr) {
            std::cerr << "Backend: Got null port" << std::endl;
            return 1;
        }

        g_passthroughs(i) = 1.0f;
        g_port_volumes(i) = 1.0f;
    }

    if(jack_activate(g_jack_client)) {
        std::cerr << "Backend: Failed to activate client" << std::endl;
        return 1;
    }

    // Start the reporting thread
    g_reporting_thread = std::thread([update_period_ms, update_cb]() {
        std::vector<loop_state_t> states(g_n_loops);
        std::vector<int32_t> positions(g_n_loops), lengths(g_n_loops);
        std::vector<float> passthroughs(g_n_ports), loop_volumes(g_n_loops), port_volumes(g_n_ports);
        while(!finished) {
            std::this_thread::sleep_for(std::chrono::milliseconds(update_period_ms));
            for(int i=0; i<g_n_loops; i++) {
                states[i] = g_atomic_state.states[i];
                positions[i] = g_atomic_state.states[i];
                lengths[i] = g_atomic_state.lengths[i];
                loop_volumes[i] = g_atomic_state.loop_volumes[i];
            }
            for(int i=0; i<g_n_ports; i++) {
                passthroughs[i] = g_atomic_state.passthroughs[i];
                port_volumes[i] = g_atomic_state.port_volumes[i];
            }
            
            update_cb(
                g_n_loops,
                g_n_ports,
                states.data(),
                lengths.data(),
                positions.data(),
                loop_volumes.data(),
                port_volumes.data(),
                passthroughs.data()
            );
        }
    });

    return 0;
}

} //extern "C"