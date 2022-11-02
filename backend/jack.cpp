#include <HalideRuntime.h>
extern "C" {
#include <jack/jack.h>
}

#include "HalideBuffer.h"

#include "shoopdaloop_loops.h"
#include "types.hpp"
#include <chrono>
#include <cstdlib>

#include <iostream>
#include <string>
#include <vector>
#include <unistd.h>
#include <functional>

using namespace Halide::Runtime;
using namespace std;

const char* client_name = "shoopdaloop_test";

Buffer<int8_t, 1> states;
Buffer<int32_t, 1> positions;
Buffer<int32_t, 1> lengths;
std::vector<float> samples_in_raw, samples_out_raw; //Use vectors here so we know the layout
Buffer<float, 2> samples_in, samples_out;
Buffer<float, 2> storage;

std::vector<jack_port_t*> input_ports;
std::vector<jack_port_t*> output_ports;

size_t n_loops;
size_t loop_len;
size_t buf_size;

void usage(char **argv) {
    std::cout << "Usage: " << argv[0] << " n_loops loop_len" << std::endl;
}

int process (jack_nframes_t nframes, void *arg) {
    static auto last_measurement = std::chrono::high_resolution_clock::now();
    static size_t n_measurements = 0;
    static float in_time = 0.0, proc_time = 0.0, out_time = 0.0;

    jack_default_audio_sample_t *buf;

    auto start = std::chrono::high_resolution_clock::now();

    for(int i=0; i<input_ports.size(); i++) {
        if(input_ports[i]) {
            buf = (jack_default_audio_sample_t*) jack_port_get_buffer(input_ports[i], nframes);
            if (buf) {
                memcpy((void*)(&samples_in_raw[buf_size*i]), (void*)buf, sizeof(float) * nframes);
            }
        }
    }

    auto did_input = std::chrono::high_resolution_clock::now();

    shoopdaloop_loops(
        samples_in,
        states,
        positions,
        lengths,
        storage,
        nframes,
        loop_len,
        samples_out,
        states,
        positions,
        lengths,
        storage
    );

    auto did_process = std::chrono::high_resolution_clock::now();

    for(int i=0; i<output_ports.size(); i++) {
        if(output_ports[i]) {
            buf = (jack_default_audio_sample_t*) jack_port_get_buffer(output_ports[i], nframes);
            if (buf) {
                memcpy((void*)buf, (void*)(&samples_out_raw[buf_size*i]), sizeof(float) * nframes);
            }
        }
    }

    auto did_output = std::chrono::high_resolution_clock::now();

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

int main(int argc, char **argv) {
    if (argc != 3) {
        usage(argv);
        exit(1);
    }

    n_loops = stoi(std::string(argv[1]));
    loop_len = stoi(std::string(argv[2]));

    jack_status_t status;
    jack_client_t* client = jack_client_open(client_name, JackNullOption, &status, nullptr);

    if(client == nullptr) {
        std::cerr << "Failed to open Jack client." << std::endl;
        exit(1);
    }

    buf_size = jack_port_type_get_buffer_size(client, JACK_DEFAULT_AUDIO_TYPE);
    std::cout << "JACK buffer size: " << buf_size << std::endl;

    states = Buffer<int8_t, 1>(n_loops);
    positions = Buffer<int32_t, 1>(n_loops);
    lengths = Buffer<int32_t, 1>(n_loops);
    samples_in_raw.resize(n_loops*buf_size); //Buffer<float, 2>(n_loops, buf_size);
    samples_out_raw.resize(n_loops*buf_size); //Buffer<float, 2>(n_loops, buf_size);
    storage = Buffer<float, 2>(loop_len, n_loops);

    samples_in = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)samples_in_raw.data(),
        buf_size, n_loops
    );
    samples_out = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)samples_out_raw.data(),
        buf_size, n_loops
    );

    jack_set_process_callback(client, process, 0);

    for(size_t i=0; i<n_loops; i++) {
        states(i) = Recording;
        positions(i) = 0;
        lengths(i) = 0;

        input_ports.push_back(jack_port_register(client, "in", JACK_DEFAULT_AUDIO_TYPE, JackPortIsInput, 0));
        output_ports.push_back(jack_port_register(client, "out", JACK_DEFAULT_AUDIO_TYPE, JackPortIsOutput, 0));

        if(input_ports[i] == nullptr) {
            std::cerr << "Got null port" << std::endl;
            exit(1);
        }
        if(output_ports[i] == nullptr) {
            std::cerr << "Got null port" << std::endl;
            exit(1);
        }
    }

    if(jack_activate(client)) {
        std::cerr << "Failed to activate client" << std::endl;
        exit(1);
    }

    std::cout << "sleep" << std::endl;
    sleep(-1);
    std::cout << "done" << std::endl;
}
