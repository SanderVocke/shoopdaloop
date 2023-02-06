#include "MidiLoop.h"
#include "MidiPortInterface.h"
#include "ObjectPool.h"
#include "JackAudioSystem.h"
#include "JackAudioPort.h"
#include "AudioLoop.h"
#include "process_loops.h"
#include "types.h"
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

std::vector<std::shared_ptr<PortInterface>> g_input_audio_ports, g_output_audio_ports;
std::vector<std::shared_ptr<MidiPortInterface>> g_input_midi_ports, g_output_midi_ports;
std::vector<float*> g_input_audio_bufs, g_output_audio_bufs;
std::vector<std::unique_ptr<MidiReadableBufferInterface>> g_input_midi_bufs;
std::vector<std::unique_ptr<MidiWriteableBufferInterface>> g_output_midi_bufs;
jack_client_t *g_client;
size_t g_n_audio_loops = 1;
size_t g_n_midi_loops = 1;
size_t g_n_audio_loops_per_port = 1;
size_t g_n_midi_loops_per_port = 1;
size_t g_n_audio_ports = 1;
size_t g_n_midi_ports = 1;
size_t g_buffer_size = 48000;
loop_state_t g_state = Stopped;
std::vector<std::shared_ptr<AudioLoop<float>>> g_audio_loops;
std::vector<std::shared_ptr<MidiLoop<uint32_t, uint16_t>>> g_midi_loops;
std::unique_ptr<JackAudioSystem> g_audio;
bool g_midi = false;
constexpr size_t g_midi_buffer_bytes = 81920;

po::options_description get_options() {
    po::options_description options ("Options");
    options.add_options()
        ("help,h", "show help information")
        ("loops,l", po::value<size_t>(), "number of loops to instantiate")
        ("loops-per-port,p", po::value<size_t>(), "number of loops per JACK port")
        ("buffer-size,b", po::value<size_t>(), "# of samples per audio buffer")
        ("record", po::value<size_t>(), "# of loops to set to recording state")
        ("play", po::value<size_t>(), "# of loops to playback state")
        ("playmuted", po::value<size_t>(), "# of loops to set to playback muted state")
        ("time,t", po::value<float>(), "amount of seconds to run before exiting")
        ("length", po::value<size_t>(), "length of the loops at init")
        ("midi,m", "use MIDI loops (audio loops are default)")
        ;
    return options;
}

void process (jack_nframes_t nframes)
{
    for (size_t idx=0; idx<g_input_audio_ports.size(); idx++) {
        auto port = std::dynamic_pointer_cast<JackAudioPort>(g_input_audio_ports[idx]);
        g_input_audio_bufs[idx] = port ? port->get_buffer(nframes) : nullptr;
    }
    for (size_t idx=0; idx<g_output_audio_ports.size(); idx++) {
        auto port = std::dynamic_pointer_cast<JackAudioPort>(g_output_audio_ports[idx]);
        g_output_audio_bufs[idx] = port ? port->get_buffer(nframes) : nullptr;
    }
    for (size_t idx=0; idx<g_input_midi_ports.size(); idx++) {
        auto port = std::dynamic_pointer_cast<JackMidiPort>(g_input_midi_ports[idx]);
        g_input_midi_bufs[idx] = port ? port->get_read_buffer(nframes) : nullptr;
    }
    for (size_t idx=0; idx<g_output_midi_ports.size(); idx++) {
        auto port = std::dynamic_pointer_cast<JackMidiPort>(g_output_midi_ports[idx]);
        g_output_midi_bufs[idx] = port ? port->get_write_buffer(nframes) : nullptr;
    }

    for (size_t idx=0; idx<g_n_audio_loops; idx++) {
        g_audio_loops[idx]->set_recording_buffer(
            g_input_audio_bufs[idx / g_n_audio_loops_per_port],
            nframes
        );
        g_audio_loops[idx]->set_playback_buffer(
            g_output_audio_bufs[idx / g_n_audio_loops_per_port],
            nframes
        );
    }

    for (size_t idx=0; idx<g_n_midi_loops; idx++) {
        g_midi_loops[idx]->set_recording_buffer(
            g_input_midi_bufs[idx / g_n_midi_loops_per_port].get(),
            nframes
        );
        g_midi_loops[idx]->set_playback_buffer(
            g_output_midi_bufs[idx / g_n_midi_loops_per_port].get(),
            nframes
        );
    }

    process_loops(g_audio_loops, nframes);
}

int main(int argc, const char* argv[]) {
    auto options = get_options();
    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, options), vm);
    po::notify(vm);
    std::optional<float> time;
    size_t length = 24000;
    size_t n_record = 0;
    size_t n_play = 0;
    size_t n_play_muted = 0;
    size_t n_loops = 1;
    size_t n_loops_per_port = 1;

    if (vm.count("help") || argc < 2) {
        std::cout << options << std::endl;
        return 1;
    }

    if (vm.count("record"))         { n_record = vm["record"].as<size_t>(); }
    if (vm.count("play"))           { n_play = vm["play"].as<size_t>(); }
    if (vm.count("playmuted"))      { n_play_muted = vm["playmuted"].as<size_t>(); }
    if (vm.count("loops"))          { n_loops = vm["loops"].as<size_t>(); }
    if (vm.count("loops-per-port")) { n_loops_per_port = vm["loops-per-port"].as<size_t>(); }
    if (vm.count("buffer-size"))    { g_buffer_size = vm["buffer-size"].as<size_t>(); }
    if (vm.count("time"))           { time = vm["time"].as<float>(); }
    if (vm.count("length"))         { length = vm["length"].as<size_t>(); }
    if (vm.count("midi"))           { g_midi = true; }

    g_n_audio_loops = g_midi ? 0 : n_loops;
    g_n_midi_loops = g_midi ? n_loops : 0;
    g_n_audio_loops_per_port = g_midi ? 0 : n_loops_per_port;
    g_n_midi_loops_per_port = g_midi ? n_loops_per_port : 0;
    g_n_midi_ports = g_midi ? std::ceil((float)g_n_midi_loops / (float)g_n_midi_loops_per_port) : 0;
    g_n_audio_ports = g_midi ? 0 : std::ceil((float)g_n_audio_loops / (float)g_n_audio_loops_per_port);
    size_t pool_size = g_n_audio_loops + 1;

    auto pool = std::make_shared<ObjectPool<AudioBuffer<float>>>(
        pool_size,
        g_buffer_size
    );
    
    for(size_t idx=0; idx < g_n_audio_loops; idx++) {
        g_audio_loops.push_back(std::make_shared<AudioLoop<float>>(
            pool,
            10, // FIXME configurable
            AudioLoopOutputType::Add // FIXME configurable
        ));

        g_audio_loops.back()->set_length(length);
        if (idx < n_record) {
            g_audio_loops.back()->plan_transition(Recording);
            g_audio_loops.back()->trigger();
        } else if (idx < (n_record + n_play)) {
            g_audio_loops.back()->plan_transition(Playing);
            g_audio_loops.back()->trigger();
        } else if (idx < (n_record + n_play + n_play_muted)) {
            g_audio_loops.back()->plan_transition(PlayingMuted);
            g_audio_loops.back()->trigger();
        }
        g_audio_loops.back()->handle_poi();
    }
    for(size_t idx=0; idx < g_n_midi_loops; idx++) {
        g_midi_loops.push_back(std::make_shared<MidiLoop<uint32_t, uint16_t>>(
            g_midi_buffer_bytes
        ));

        g_midi_loops.back()->set_length(length);
        if (idx < n_record) {
            g_midi_loops.back()->plan_transition(Recording);
            g_midi_loops.back()->trigger();
        } else if (idx < (n_record + n_play)) {
            g_midi_loops.back()->plan_transition(Playing);
            g_midi_loops.back()->trigger();
        } else if (idx < (n_record + n_play + n_play_muted)) {
            g_midi_loops.back()->plan_transition(PlayingMuted);
            g_midi_loops.back()->trigger();
        }
        g_midi_loops.back()->handle_poi();
    }

    g_audio = std::make_unique<JackAudioSystem>(
        "jack_loops",
        process
    );

    for(size_t idx=0; idx<g_n_audio_ports; idx++) {
        g_input_audio_ports.push_back(g_audio->open_audio_port(
            "audio_input_" + std::to_string(idx),
            PortDirection::Input
        ));
        g_output_audio_ports.push_back(g_audio->open_audio_port(
            "audio_input_" + std::to_string(idx),
            PortDirection::Output
        ));
    }
    g_input_audio_bufs.resize(g_n_audio_ports);
    g_output_audio_bufs.resize(g_n_audio_ports);

    for(size_t idx=0; idx<g_n_midi_ports; idx++) {
        g_input_midi_ports.push_back(g_audio->open_midi_port(
            "midi_input_" + std::to_string(idx),
            PortDirection::Input,
            false
        ));
        g_output_midi_ports.push_back(g_audio->open_midi_port(
            "midi_output_" + std::to_string(idx),
            PortDirection::Output,
            false
        ));
    }
    g_input_midi_bufs.resize(g_n_midi_ports);
    g_output_midi_bufs.resize(g_n_midi_ports);

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