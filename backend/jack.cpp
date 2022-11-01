#include <jack/jack.h>
#include "HalideBuffer.h"

#include "shoopdaloop_loops.h"
#include "types.hpp"

#include <iostream>
#include <string>
#include <vector>
#include <unistd.h>

using namespace Halide::Runtime;
using namespace std;

const char* client_name = "shoopdaloop_test";

Buffer<uint8_t, 1> states;
Buffer<uint16_t, 1> positions;
Buffer<uint16_t, 1> lengths;
Buffer<float, 2> samples_in, samples_out;
Buffer<float, 2> storage;

std::vector<jack_port_t*> input_ports;
std::vector<jack_port_t*> output_ports;

size_t n_loops;
size_t loop_len;

void usage(char **argv) {
    std::cout << "Usage: " << argv[0] << " n_loops loop_len" << std::endl;
}

int process (jack_nframes_t nframes, void *arg) {
    jack_default_audio_sample_t *buf;

    for(int i=0; i<n_loops; i++) {
        buf = (jack_default_audio_sample_t*) jack_port_get_buffer(input_ports[i], nframes);
        if (buf) {
            for(size_t s=0; s<nframes; s++) {
                samples_in(i, s) = buf[s];
            }
        }
    }

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

    for(int i=0; i<n_loops; i++) {
        buf = (jack_default_audio_sample_t*) jack_port_get_buffer(output_ports[i], nframes);
        if (buf) {
            for(size_t s=0; s<nframes; s++) {
                buf[s] = samples_out(i, s);
            }
        }
    }
}

int main(int argc, char **argv) {
    if (argc != 3) {
        usage(argv);
        exit(1);
    }

    n_loops = stoi(argv[1]);
    loop_len = stoi(argv[2]);
    jack_status_t status;
    jack_client_t* client = jack_client_open(client_name, JackNullOption, &status, nullptr);

    if(client == nullptr) {
        std::cerr << "Failed to open Jack client." << std::endl;
        exit(1);
    }

    size_t buf_size = jack_port_type_get_buffer_size(client, JACK_DEFAULT_AUDIO_TYPE);

    states = Buffer<uint8_t, 1>(n_loops);
    positions = Buffer<uint16_t, 1>(n_loops);
    lengths = Buffer<uint16_t, 1>(n_loops);
    samples_in = Buffer<float, 2>(n_loops, buf_size);
    samples_out = Buffer<float, 2>(n_loops, buf_size);
    storage= Buffer<float, 2>(n_loops, loop_len);

    jack_set_process_callback(client, process, 0);

    for(size_t i=0; i<n_loops; i++) {
        states(i) = Paused;
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
    }

    sleep(-1);
}
