#include "AudioBufferPool.h"
#include "AudioLoop.h"
#include "process_loops.h"
#include <boost/program_options/options_description.hpp>
#include <boost/program_options/parsers.hpp>
#include <boost/program_options/variables_map.hpp>
#include <jack/jack.h>
#include <boost/program_options.hpp>
#include <iostream>
#include <chrono>
#include <jack/types.h>
#include <string>

namespace po = boost::program_options;
using namespace std::chrono_literals;

std::vector<jack_port_t*> g_input_ports, g_output_ports;
std::vector<jack_default_audio_sample_t*> g_input_bufs, g_output_bufs;
jack_client_t *g_client;
size_t g_n_loops = 1;
size_t g_n_loops_per_port = 1;
size_t g_n_ports = 1;
size_t g_buffer_size = 48000;
loop_state_t g_state = Stopped;
std::vector<std::shared_ptr<AudioLoop<float>>> g_loops;

po::options_description get_options() {
    po::options_description options ("Options");
    options.add_options()
        ("help,h", "show help information")
        ("loops,l", po::value<size_t>(), "number of loops to instantiate")
        ("loops-per-port,p", po::value<size_t>(), "number of loops per JACK port")
        ("buffer-size,b", po::value<size_t>(), "# of samples per audio buffer")
        ("record", "set loops to recording state")
        ("play", "set loops to playback state")
        ("playmuted", "set loops to playback muted state")
        ;
    return options;
}

int
process (jack_nframes_t nframes, void *arg)
{
    if (g_state == Recording) {
        for (size_t idx=0; idx<g_n_ports; idx++) {
            g_input_bufs[idx] = (float*) jack_port_get_buffer(g_input_ports[idx], nframes);
        }
        for (size_t idx=0; idx<g_n_loops; idx++) {
            g_loops[idx]->set_recording_buffer(g_input_bufs[idx/g_n_loops_per_port], nframes);
        }
        process_loops(g_loops, nframes);
    }
    return 0;
}

int main(int argc, const char* argv[]) {
    auto options = get_options();
    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, options), vm);
    po::notify(vm);

    if (vm.count("help") || argc < 2) {
        std::cout << options << std::endl;
        return 1;
    }

    if (vm.count("record"))         { g_state = Recording; }
    if (vm.count("play"))           { g_state = Playing; }
    if (vm.count("playmuted"))      { g_state = PlayingMuted; }
    if (vm.count("loops"))          { g_n_loops = vm["loops"].as<size_t>(); }
    if (vm.count("loops-per-port")) { g_n_loops_per_port = vm["loops-per-port"].as<size_t>(); }
    if (vm.count("buffer-size"))    { g_buffer_size = vm["buffer-size"].as<size_t>(); }

    g_n_ports = g_n_loops / g_n_loops_per_port;
    size_t pool_size = g_n_loops;

    auto pool = std::make_shared<AudioBufferPool<float>>(
        pool_size,
        g_buffer_size,
        100ms
    );
    
    for(size_t idx=0; idx < g_n_loops; idx++) {
        g_loops.push_back(std::make_shared<AudioLoop<float>>(
            pool,
            10, // FIXME configurable
            AudioLoopOutputType::Add // FIXME configurable
        ));
        g_loops.back()->set_next_state(g_state);
        g_loops.back()->transition_now();
        g_loops.back()->handle_poi();
    }

    jack_status_t status;
    g_client = jack_client_open ("jack_loops", JackNullOption, &status, nullptr);
    if (g_client == nullptr) {
        std::cerr << "Could not open JACK client." << std::endl;
        return 1;
    }

    if (g_state == Recording) {
        for(size_t idx=0; idx<g_n_ports; idx++) {
            g_input_ports.push_back(
                jack_port_register (g_client, ("input_" + std::to_string(idx)).c_str(),
					 JACK_DEFAULT_AUDIO_TYPE,
					 JackPortIsInput, 0));
            if (g_input_ports.back() == nullptr) {
                std::cerr << "Could not create input port " << idx << std::endl;
                return 1;
            }
        }
        g_input_bufs.resize(g_n_ports);
    }

    jack_set_process_callback (g_client, process, 0);

    if (jack_activate (g_client)) {
		std::cerr << "Could not activate client" << std::endl;
		return 1;
	}

    while(true) {
        std::this_thread::sleep_for(100ms);
    }
}