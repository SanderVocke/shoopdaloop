#include <HalideBuffer.h>
#include <cmath>
#include <jack/types.h>
#include <jack/jack.h>

#include "shoopdaloop_backend.h"
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
#include <boost/lockfree/spsc_queue.hpp>
#include <memory>

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
size_t g_sample_rate;

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

UpdateCallback g_update_cb;

bool finished = false;

// A lock-free queue is used to pass commands to the audio
// processing thread in the form of functors.
constexpr unsigned command_queue_len = 1024;
boost::lockfree::spsc_queue<std::function<void()>> g_cmd_queue(command_queue_len);
void push_command(std::function<void()> cmd) {
    while (!g_cmd_queue.push(cmd)) { 
        std::this_thread::sleep_for(std::chrono::microseconds(100));
    }
}

extern "C" {

// The processing callback to be called by Jack.
int jack_process (jack_nframes_t nframes, void *arg) {
    static auto last_measurement = std::chrono::high_resolution_clock::now();
    static size_t n_measurements = 0;
    static float cmd_time = 0.0, in_time = 0.0, proc_time = 0.0, out_time = 0.0;

    jack_default_audio_sample_t *buf;

    auto start = std::chrono::high_resolution_clock::now();

    // Process commands
    std::function<void()> cmd;
    while(g_cmd_queue.pop(cmd)) {
        cmd();
    }

    auto did_cmds = std::chrono::high_resolution_clock::now();

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
    cmd_time += std::chrono::duration_cast<std::chrono::microseconds>(did_cmds - start).count();
    in_time += std::chrono::duration_cast<std::chrono::microseconds>(did_input - did_cmds).count();
    proc_time += std::chrono::duration_cast<std::chrono::microseconds>(did_process - did_input).count();
    out_time += std::chrono::duration_cast<std::chrono::microseconds>(did_output - did_process).count();
    n_measurements++;

    if (std::chrono::duration_cast<std::chrono::milliseconds>(did_output - last_measurement).count() > 1000.0) {
        std::cout << cmd_time/(float)n_measurements
                  << " " << in_time/(float)n_measurements 
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
    float loop_len_seconds,
    unsigned *loops_to_ports_mapping,
    const char **input_port_names,
    const char **output_port_names,
    const char *client_name,
    UpdateCallback update_cb,
    AbortCallback abort_cb
) {
    g_n_loops = n_loops;
    g_n_ports = n_ports;

    // Create a JACK client
    jack_status_t status;
    g_jack_client = jack_client_open(client_name, JackNullOption, &status, nullptr);
    if(g_jack_client == nullptr) {
        std::cerr << "Backend: given a null client." << std::endl;
        return 1;
    }

    // Get JACK buffer size and sample rate
    g_buf_size = 
        jack_port_type_get_buffer_size(g_jack_client, JACK_DEFAULT_AUDIO_TYPE) / sizeof(jack_default_audio_sample_t);
    g_sample_rate =
        (size_t) jack_get_sample_rate(g_jack_client);
    std::cout << "Backend: JACK: buf size " << g_buf_size << " @ " << g_sample_rate << "samples/s" << std::endl;

    g_loop_len = (size_t)((float) g_sample_rate * loop_len_seconds);

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
        g_states(i) = Stopped;
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

    g_update_cb = update_cb;

    return 0;
}

int do_loop_action(
    unsigned loop_idx,
    loop_action_t action
) {
    std::function<void()> cmd = nullptr;
    switch(action) {
        case DoRecord:
            cmd = [loop_idx]() {
                g_states(loop_idx) = Recording;
                g_positions(loop_idx) = 0;
                g_lengths(loop_idx) = 0;
            };
            break;
        case DoPlay:
            cmd = [loop_idx]() {
                g_states(loop_idx) = Playing;
            };
            break;
        case DoStop:
            cmd = [loop_idx]() {
                g_states(loop_idx) = Stopped;
                g_positions(loop_idx) = 0;
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
    std::vector<loop_state_t> states(g_n_loops);
    std::vector<int32_t> positions(g_n_loops), lengths(g_n_loops);
    std::vector<float> passthroughs(g_n_ports), loop_volumes(g_n_loops), port_volumes(g_n_ports);

    // Copy the state
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
    
    if (g_update_cb) {
        int r = g_update_cb(
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
}

} //extern "C"