#include "AudioBufferPool.h"
#include "JackAudioSystem.h"
#include "JackAudioPort.h"
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
#include <memory>

namespace po = boost::program_options;
using namespace std::chrono_literals;

std::vector<std::shared_ptr<PortInterface>> g_input_ports, g_output_ports;
std::vector<float*> g_input_bufs, g_output_bufs;
jack_client_t *g_client;
size_t g_n_loops = 1;
size_t g_n_loops_per_port = 1;
size_t g_n_ports = 1;
size_t g_buffer_size = 48000;
loop_state_t g_state = Stopped;
std::vector<std::shared_ptr<AudioLoop<float>>> g_loops;
std::unique_ptr<JackAudioSystem> g_audio;

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
        ("time,t", po::value<float>(), "amount of seconds to run before exiting")
        ;
    return options;
}

void process (jack_nframes_t nframes)
{
    for (size_t idx=0; idx<g_input_ports.size(); idx++) {
        auto port = std::dynamic_pointer_cast<JackAudioPort>(g_input_ports[idx]);
        g_input_bufs[idx] = port ? port->get_buffer(nframes) : nullptr;
    }
    for (size_t idx=0; idx<g_output_ports.size(); idx++) {
        auto port = std::dynamic_pointer_cast<JackAudioPort>(g_output_ports[idx]);
        g_output_bufs[idx] = port ? port->get_buffer(nframes) : nullptr;
    }

    if (g_state == Recording) {
        for (size_t idx=0; idx<g_loops.size(); idx++) {
            g_loops[idx]->set_recording_buffer(
                g_input_bufs[idx / g_n_loops_per_port],
                nframes
            );
        }
        process_loops(g_loops, nframes);
    }
}

int main(int argc, const char* argv[]) {
    auto options = get_options();
    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, options), vm);
    po::notify(vm);
    std::optional<float> time;

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
    if (vm.count("time"))           { time = vm["time"].as<float>(); }

    g_n_ports = g_n_loops / g_n_loops_per_port;
    size_t pool_size = g_n_loops + 1;

    auto pool = std::make_shared<AudioBufferPool<float>>(
        pool_size,
        g_buffer_size
    );
    
    for(size_t idx=0; idx < g_n_loops; idx++) {
        g_loops.push_back(std::make_shared<AudioLoop<float>>(
            pool,
            10, // FIXME configurable
            AudioLoopOutputType::Add // FIXME configurable
        ));
        g_loops.back()->plan_transition(g_state);
        g_loops.back()->trigger();
        g_loops.back()->handle_poi();
    }

    g_audio = std::make_unique<JackAudioSystem>(
        "jack_loops",
        process
    );

    if (g_state == Recording) {
        for(size_t idx=0; idx<g_n_ports; idx++) {
            g_input_ports.push_back(g_audio->open_audio_port(
                "input_" + std::to_string(idx),
                PortDirection::Input
            ));
        }
        g_input_bufs.resize(g_n_ports);
    }

    g_audio->start();

    auto t_start = std::chrono::high_resolution_clock::now();
    while(true) {
        std::this_thread::sleep_for(100ms);
        auto t = std::chrono::high_resolution_clock::now();
        auto diff = t - t_start;
        if(time.has_value() &&
           std::chrono::duration_cast<std::chrono::milliseconds>(diff).count() > (time.value() * 1000.0f)) {
                break;
           }
    }
}