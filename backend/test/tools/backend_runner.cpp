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
#include <thread>

#include "shoopdaloop_backend.h"
#include "types.h"

namespace po = boost::program_options;
using namespace std::chrono_literals;

std::vector<shoopdaloop_loop*> g_loops;
std::vector<shoopdaloop_audio_port*> g_audio_input_ports, g_audio_output_ports;
std::vector<shoopdaloop_midi_port*> g_midi_input_ports, g_midi_output_ports;
std::vector<std::vector<shoopdaloop_loop_audio_channel*>> g_audio_channels;
std::vector<std::vector<shoopdaloop_loop_midi_channel*>> g_midi_channels;

po::options_description get_options() {
    po::options_description options ("Options");
    options.add_options()
        ("help,h", "show help information")
        ("loops,l", po::value<size_t>()->default_value(1), "number of loops to instantiate")
        ("audio-input-ports", po::value<size_t>()->default_value(1), "number of audio input ports")
        ("audio-output-ports", po::value<size_t>()->default_value(1), "number of audio output ports")
        ("midi-input-ports", po::value<size_t>()->default_value(1), "number of midi input ports")
        ("midi-output-ports", po::value<size_t>()->default_value(1), "number of midi output ports")
        ("audio-channels,a", po::value<size_t>()->default_value(1), "number of audio channels per loop")
        ("midi-channels,m", po::value<size_t>()->default_value(1), "number of midi channels per loop")
        ("record", po::value<size_t>()->default_value(0), "# of loops to set to recording state")
        ("play", po::value<size_t>()->default_value(0), "# of loops to playback state")
        ("playmuted", po::value<size_t>()->default_value(0), "# of loops to set to playback muted state")
        ("time,t", po::value<float>(), "amount of seconds to run before exiting")
        ("length", po::value<size_t>()->default_value(0), "length of the loops at init")
        ;
    return options;
}

int main(int argc, const char* argv[]) {
    auto options = get_options();
    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, options), vm);
    po::notify(vm);

    if (vm.count("help")) {
        std::cout << options << std::endl;
        return 1;
    }

    // Initialize the back-end.
    initialize("shoopdaloop_backend");

    size_t n_loops = vm["loops"].as<size_t>();
    size_t n_audio_channels = vm["audio-channels"].as<size_t>();
    size_t n_midi_channels = vm["midi-channels"].as<size_t>();
    std::cout << "Creating " << n_loops << " loops." << std::endl;
    for(size_t idx=0; idx < n_loops; idx++) {
        auto loop = create_loop();
        std::vector<shoopdaloop_loop_audio_channel*> audios;
        std::vector<shoopdaloop_loop_midi_channel*> midis;

        for (size_t j=0; j<n_audio_channels; j++) {
            audios.push_back(add_audio_channel(loop));
        }

        for (size_t j=0; j<n_midi_channels; j++) {
            midis.push_back(add_midi_channel(loop));
        }

        g_loops.push_back(loop);
        g_audio_channels.push_back(audios);
        g_midi_channels.push_back(midis);
    }

    size_t n_audio_inputs = vm["audio-input-ports"].as<size_t>();
    size_t n_audio_outputs = vm["audio-output-ports"].as<size_t>();
    size_t n_midi_inputs = vm["midi-input-ports"].as<size_t>();
    size_t n_midi_outputs = vm["midi-output-ports"].as<size_t>();
    std::cout << "Creating " << n_audio_inputs << " audio inputs." << std::endl;
    for(size_t idx=0; idx < n_audio_inputs; idx++) {
        std::string name = "audio_in_" + std::to_string(idx+1);
        g_audio_input_ports.push_back(open_audio_port(name.c_str(), Input));
        std::cout << "Saved " << g_audio_input_ports.back() << std::endl;
    }
    // std::cout << "Creating " << n_audio_outputs << " audio outputs." << std::endl;
    // for(size_t idx=0; idx < n_audio_outputs; idx++) {
    //     std::string name = "audio_out_" + std::to_string(idx+1);
    //     g_audio_output_ports.push_back(open_audio_port(name.c_str(), Output));
    // }
    // std::cout << "Creating " << n_midi_inputs << " MIDI inputs." << std::endl;
    // for(size_t idx=0; idx < n_midi_inputs; idx++) {
    //     std::string name = "midi_in_" + std::to_string(idx+1);
    //     g_midi_input_ports.push_back(open_midi_port(name.c_str(), Input));
    // }
    // std::cout << "Creating " << n_midi_outputs << " MIDI outputs." << std::endl;
    // for(size_t idx=0; idx < n_midi_outputs; idx++) {
    //     std::string name = "midi_out_" + std::to_string(idx+1);
    //     g_midi_output_ports.push_back(open_midi_port(name.c_str(), Output));
    // }

    std::cout << "Connecting ports in round-robin fashion." << std::endl;
    size_t port_idx;
    port_idx = 0;
    for (size_t loop_idx=0; loop_idx < n_loops; loop_idx++) {
        for (size_t chan_idx=0; chan_idx < n_audio_channels; chan_idx++) {
            std::cout << "Using " << g_audio_input_ports[port_idx] << std::endl;
            connect_audio_input(g_audio_channels[loop_idx][chan_idx], g_audio_input_ports[port_idx]);
            port_idx = (port_idx+1) % n_audio_inputs;
        }
    }
    // port_idx = 0;
    // for (size_t loop_idx=0; loop_idx < n_loops; loop_idx++) {
    //     for (size_t chan_idx=0; chan_idx < n_audio_channels; chan_idx++) {
    //         //connect_audio_output(g_audio_channels[loop_idx][chan_idx], g_audio_output_ports[port_idx]);
    //         port_idx = (port_idx+1) % n_audio_outputs;
    //     }
    // }
    // port_idx = 0;
    // for (size_t loop_idx=0; loop_idx < n_loops; loop_idx++) {
    //     for (size_t chan_idx=0; chan_idx < n_midi_channels; chan_idx++) {
    //         //connect_midi_input(g_midi_channels[loop_idx][chan_idx], g_midi_input_ports[port_idx]);
    //         port_idx = (port_idx+1) % n_midi_inputs;
    //     }
    // }
    // port_idx = 0;
    // for (size_t loop_idx=0; loop_idx < n_loops; loop_idx++) {
    //     for (size_t chan_idx=0; chan_idx < n_midi_channels; chan_idx++) {
    //         //connect_midi_output(g_midi_channels[loop_idx][chan_idx], g_midi_output_ports[port_idx]);
    //         port_idx = (port_idx+1) % n_midi_outputs;
    //     }
    // }

    while(true) { std::this_thread::sleep_for(10ms); }
}