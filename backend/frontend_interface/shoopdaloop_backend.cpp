#include <stdio.h>
#include "AudioPortInterface.h"
#include "AudioBufferPool.h"
#include "process_loops.h"
#include "types.h"
#include <memory>
#include <vector>
#include <optional>
#include <boost/lockfree/spsc_queue.hpp>
#include <chrono>
#include "shoopdaloop_backend.h"

#include "JackAudioSystem.h"
#include "AudioLoop.h"

using namespace std::chrono_literals;

// TYPES

struct PortInfo {
    std::shared_ptr<PortInterface> port;
    float   *maybe_audio_buffer;
    uint8_t *maybe_midi_buffer;
};

struct LoopInfo : public LoopInterface {
    std::shared_ptr<LoopInterface> loop;
    std::shared_ptr<PortInfo> maybe_input_port;
    std::shared_ptr<PortInfo> maybe_output_port;

    // Implement the loop interface by forwarding all calls to the actual loop.
    // This way we can use this structure as a loop everywhere.
    void check() {
        if (!loop) { throw std::runtime_error("Attempting to access non-existent loop."); }
    }
    std::optional<size_t> get_next_poi() const override                           { check(); return loop->get_next_poi(); }
    bool is_triggering_now() override                                             { check(); return loop->is_triggering_now(); }
    std::shared_ptr<LoopInterface> const& get_sync_source() const override        { check(); return loop->get_sync_source(); }
    void set_sync_source(std::shared_ptr<LoopInterface> const& src) override      { check(); loop->set_sync_source(src); }
    void trigger() override                                                       { check(); loop->trigger(); }
    void handle_poi() override                                                    { check(); loop->handle_poi(); }
    void handle_sync() override                                                   { check(); loop->handle_sync(); }
    void process(size_t n_samples) override                                       { check(); loop->process(n_samples); }
    size_t get_n_planned_transitions() const override                             { check(); return loop->get_n_planned_transitions(); }
    size_t get_planned_transition_delay(size_t idx) const override                { check(); return loop->get_planned_transition_delay(idx); }
    loop_state_t get_planned_transition_state(size_t idx) const override          { check(); return loop->get_planned_transition_state(idx); }
    void clear_planned_transitions() override                                     { check(); loop->clear_planned_transitions(); }
    void plan_transition(loop_state_t state, size_t n_cycles_delay = 0) override  { check(); loop->plan_transition(state, n_cycles_delay); }
    size_t get_position() const override                                          { check(); return loop->get_position(); }
    size_t get_length() const override                                            { check(); return loop->get_length(); }
    void set_position(size_t pos) override                                        { check(); loop->set_position(pos); }
    loop_state_t get_state() const override                                       { check(); return loop->get_state(); }
};

// CONSTANTS

constexpr size_t g_slow_midi_port_queue_starting_size = 4096;
constexpr size_t g_midi_buffer_bytes = 81920;
constexpr size_t g_audio_recording_buffer_size = 24000; // 0.5 sec / buffer @ 48kHz
constexpr size_t g_midi_max_merge_size = 4096;
constexpr size_t g_command_queue_len = 1024;
constexpr size_t g_n_buffers_in_pool = 100;
constexpr auto g_buffer_pool_replenish_delay = 100ms;
constexpr size_t g_initial_loop_max_n_buffers = 256; // Allows for about 2 minutes of buffers before expanding



// GLOBALS

backend_features_t g_features;
std::unique_ptr<JackAudioSystem> g_audio;
std::vector<std::shared_ptr<LoopInfo>> g_loops;
std::vector<std::shared_ptr<PortInfo>> g_loop_ports;
std::shared_ptr<AudioBufferPool<float>> g_audio_buffer_pool;

// A lock-free queue is used to pass commands to the audio
// processing thread in the form of functors.
boost::lockfree::spsc_queue<std::function<void()>> g_cmd_queue(g_command_queue_len);



// PRIVATE FUNCTIONS

// Push a command into the command queue. It will be executed in the audio
// processing thread on the next process iteration.
void push_command(std::function<void()> cmd) {
    while (!g_cmd_queue.push(cmd)) { 
        std::this_thread::sleep_for(std::chrono::microseconds(100));
    }
}

// Audio processing function.
void process(size_t n_frames) {
    {
        // Process queued commands
        std::function<void()> cmd;
        while(g_cmd_queue.pop(cmd)) {
            cmd();
        }
    }

    {
        // Aqcuire buffers.
        for (auto &port_info : g_loop_ports) {
            port_info->maybe_audio_buffer = nullptr;
            port_info->maybe_midi_buffer  = nullptr;
            if (auto audio_port = std::dynamic_pointer_cast<AudioPortInterface<float>>(port->port)) {
                port_info->maybe_audio_buffer = audio_port->get_buffer(n_frames);
            }
        }
    }

    {
        // Connect buffers to loops
        for (auto &loop_info : g_loops) {
            if (auto audio_loop = std::dynamic_pointer_cast<AudioLoop<float>>(loop_info->loop) != nullptr) {
                if (loop_info->maybe_input_port && loop_info->maybe_input_port->maybe_audio_buffer) {
                    audio_loop->set_recording_buffer(loop_info->maybe_input_port->maybe_audio_buffer, n_frames);
                }
                if (loop_info->maybe_output_port && loop_info->maybe_output_port->maybe_audio_buffer) {
                    audio_loop->set_playback_buffer(loop_info->maybe_output_port->maybe_audio_buffer, n_frames);
                }
            }
        }
    }

    // Process.
    process_loops<LoopInfo>(g_loops, n_frames);


}



// INTERFACE IMPLEMENTATIONS

jack_client_t* initialize(
    unsigned n_loops,
    unsigned n_ports,
    unsigned n_mixed_output_ports,
    float loop_len_seconds,
    unsigned *loops_to_ports_mapping,
    int *loops_hard_sync_mapping,
    int *loops_soft_sync_mapping,
    int *ports_to_mixed_outputs_mapping,
    unsigned *ports_midi_enabled,
    const char **input_port_names,
    const char **output_port_names,
    const char **mixed_output_port_names,
    const char **midi_input_port_names,
    const char **midi_output_port_names,
    const char *client_name,
    unsigned latency_buf_size,
    UpdateCallback update_cb,
    AbortCallback abort_cb,
    backend_features_t features
) {
    if (g_audio == nullptr) {
        g_audio = std::make_unique<JackAudioSystem>(std::string(client_name), process);
    }
    if (g_audio_buffer_pool == nullptr) {
        g_audio_buffer_pool = std::make_shared<AudioBufferPool<float>>(
            g_n_buffers_in_pool,
            g_audio_recording_buffer_size,
            g_buffer_pool_replenish_delay
        );
    }

    if (features & Profiling) {
        throw std::runtime_error ("Profiling not implemented.");
    }

    g_cmd_queue.reset();
    g_loops.clear();
    g_loop_ports.clear();
    
    {
        // Create loops.
        // TODO: MIDI
        for (size_t idx=0; idx < n_loops; idx++) {
            auto loop = std::make_shared<LoopInfo>();
            loop->loop = std::make_shared<AudioLoop<float>>(
                g_audio_buffer_pool,
                g_initial_loop_max_n_buffers,
                AudioLoopOutputType::Add // TODO: mixing properly with volume
            );
            g_loops.push_back(loop);
        }
    }

    {
        // Create ports.
        for (size_t idx=0; idx < n_ports; idx++) {
            std::string audio_input_name(input_port_names[idx]);
            std::string audio_output_name(output_port_names[idx]);
            g_loop_ports.push_back(g_audio->open_audio_port(audio_input_name, PortDirection::Input));
            g_loop_ports.push_back(g_audio->open_audio_port(audio_output_name, PortDirection::Output));
            // TODO MIDI
        }
    }

    g_audio->start();

    return g_audio->get_client();
}

unsigned get_sample_rate() {
    if (g_audio == nullptr) {
        throw std::runtime_error("Requesting sample-rate before back-end initialized");
    }
    return g_audio->get_sample_rate();
}