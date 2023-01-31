#include <cmath>
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

    PortInfo(std::shared_ptr<PortInterface> port) : port(port) {}
};

struct LoopInfo : public LoopInterface {
    std::shared_ptr<LoopInterface> loop;
    std::shared_ptr<PortInfo> maybe_input_port;
    std::shared_ptr<PortInfo> maybe_output_port;

    // Implement the loop interface by forwarding all calls to the actual loop.
    // This way we can use this structure as a loop everywhere.
    void check() const {
        if (!loop) { throw std::runtime_error("Attempting to access non-existent loop."); }
    }
    std::optional<size_t> get_next_poi() const override                           { check(); return loop->get_next_poi(); }
    bool is_triggering_now() override                                             { check(); return loop->is_triggering_now(); }
    std::shared_ptr<LoopInterface> const& get_soft_sync_source() const override   { check(); return loop->get_soft_sync_source(); }
    void set_soft_sync_source(std::shared_ptr<LoopInterface> const& src) override { check(); loop->set_soft_sync_source(src); }
    std::shared_ptr<LoopInterface> const& get_hard_sync_source() const override   { check(); return loop->get_hard_sync_source(); }
    void set_hard_sync_source(std::shared_ptr<LoopInterface> const& src) override { check(); loop->set_hard_sync_source(src); }
    void trigger(bool propagate) override                                         { check(); loop->trigger(propagate); }
    void handle_poi() override                                                    { check(); loop->handle_poi(); }
    void handle_soft_sync() override                                              { check(); loop->handle_soft_sync(); }
    void handle_hard_sync() override                                              { check(); loop->handle_hard_sync(); }   
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
    void set_state(loop_state_t state) override                                   { check(); return loop->set_state(state); }
    void set_length(size_t length) override                                       { check(); return loop->set_length(length); }
};

// A structure of atomic scalars is used to communicate the latest
// state back from the Jack processing thread to the main thread.
// The templating allows us to inject atomic types or not depending on the need.
template<typename LoopState, typename Int8, typename Int32, typename Float, typename Unsigned>
struct StateReportTemplate {
    std::vector<LoopState> states;
    std::vector<LoopState> next_states;
    std::vector<Int32> next_states_countdown;
    std::vector<Int32> positions, lengths, latencies;
    std::vector<Float> passthroughs, loop_volumes, port_volumes, port_input_peaks, port_output_peaks, loop_output_peaks;
    std::vector<Int8> ports_muted, port_inputs_muted;
    std::vector<Unsigned> loop_n_output_events_since_last_update,
        port_n_input_events_since_last_update,
        port_n_output_events_since_last_update;
    
    // Resize the state object. Makes contents invalid.
    void resize(size_t n_loops, size_t n_ports) {
        states = decltype(states)(n_loops);
        next_states = decltype(next_states)(n_loops);
        next_states_countdown = decltype(next_states_countdown)(n_loops);
        positions = decltype(positions)(n_loops);
        lengths = decltype(lengths)(n_loops);
        latencies = decltype(latencies)(n_loops);
        loop_volumes = decltype(loop_volumes)(n_loops);
        loop_output_peaks = decltype(loop_output_peaks)(n_loops);
        loop_n_output_events_since_last_update = decltype(loop_n_output_events_since_last_update)(n_loops);    
        port_volumes = decltype(port_volumes)(n_ports);
        passthroughs = decltype(passthroughs)(n_ports);
        port_input_peaks = decltype(port_input_peaks)(n_ports); 
        port_output_peaks = decltype(port_output_peaks)(n_ports); 
        ports_muted = decltype(ports_muted)(n_ports); 
        port_inputs_muted = decltype(port_inputs_muted)(n_ports); 
        port_n_input_events_since_last_update = decltype(port_n_input_events_since_last_update)(n_ports); 
        port_n_output_events_since_last_update = decltype(port_n_output_events_since_last_update)(n_ports); 
    }

    // Copy to another instance. Storate types must support individual assignment.
    template<typename OtherLoopState, typename OtherInt8, typename OtherInt32, typename OtherFloat, typename OtherUnsigned>
    void copy_to(StateReportTemplate<OtherLoopState, OtherInt8, OtherInt32, OtherFloat, OtherUnsigned> &other) const {
        other.resize(states.size(), port_volumes.size());
        for (size_t idx = 0; idx<states.size(); idx++) {
            other.states[idx] = states[idx];
            other.positions[idx] = positions[idx];
            other.lengths[idx] = lengths[idx];
            other.latencies[idx] = latencies[idx];
            other.loop_volumes[idx] = loop_volumes[idx];
            other.loop_output_peaks[idx] = loop_output_peaks[idx];
            other.loop_n_output_events_since_last_update[idx] = loop_n_output_events_since_last_update[idx];
            other.next_states[idx] = next_states[idx];
            other.next_states_countdown[idx] = next_states_countdown[idx];
        }
        for (size_t idx=0; idx<port_volumes.size(); idx++) {
            other.passthroughs[idx] = passthroughs[idx];
            other.port_volumes[idx] = port_volumes[idx];
            other.port_input_peaks[idx] = port_input_peaks[idx];
            other.port_output_peaks[idx] = port_output_peaks[idx];
            other.ports_muted[idx] = ports_muted[idx];
            other.port_inputs_muted[idx] = port_inputs_muted[idx];
            other.port_n_input_events_since_last_update[idx] = port_n_input_events_since_last_update[idx];
            other.port_n_output_events_since_last_update[idx] = port_n_output_events_since_last_update[idx];
        }
    }

    void reset_peaks() {
        for (size_t idx = 0; idx<states.size(); idx++) {
            loop_output_peaks[idx] = 0;
            loop_n_output_events_since_last_update[idx] = 0;
        }
        for (size_t idx = 0; idx<port_volumes.size(); idx++) {
            port_input_peaks[idx] = 0;
            port_output_peaks[idx] = 0;
            port_n_input_events_since_last_update[idx] = 0;
            port_n_output_events_since_last_update[idx] = 0;
        }
    }
};
typedef StateReportTemplate<std::atomic<loop_state_t>,
                            std::atomic<int8_t>,
                            std::atomic<int32_t>,
                            std::atomic<float>,
                            std::atomic<unsigned>> AtomicStateReport;
typedef StateReportTemplate<loop_state_t,
                            int8_t,
                            int32_t,
                            float,
                            unsigned> StateReport;

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
std::vector<std::shared_ptr<PortInfo>> g_loop_input_ports;
std::vector<std::shared_ptr<PortInfo>> g_loop_output_ports;
std::shared_ptr<AudioBufferPool<float>> g_audio_buffer_pool;
UpdateCallback g_update_callback;
AbortCallback g_abort_callback;
AtomicStateReport g_atomic_state;

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

// Update the atomic state from the loops and ports. Should be called from the processing thread only.
void update_atomic_state() {
    for (size_t idx=0; idx<g_loops.size(); idx++) {
        auto &loop = g_loops[idx];
        g_atomic_state.positions[idx] = loop->get_position();
        g_atomic_state.lengths[idx] = loop->get_length();
        g_atomic_state.states[idx] = loop->get_state();
        g_atomic_state.next_states[idx] = loop->get_n_planned_transitions() > 0 ?
            loop->get_planned_transition_state(0) : (loop_state_t)-1;
        g_atomic_state.next_states_countdown[idx] = loop->get_n_planned_transitions() > 0 ?
            loop->get_planned_transition_delay(0) : -1;
        // TODO the rest
    }

    for (size_t idx=0; idx<g_loop_input_ports.size(); idx++) {
        // TODO the rest
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
        for (auto &port_info : g_loop_input_ports) {
            port_info->maybe_audio_buffer = nullptr;
            port_info->maybe_midi_buffer  = nullptr;
            if (auto audio_port = std::dynamic_pointer_cast<AudioPortInterface<float>>(port_info->port)) {
                port_info->maybe_audio_buffer = audio_port->get_buffer(n_frames);
            }
        }
        for (auto &port_info : g_loop_output_ports) {
            port_info->maybe_audio_buffer = nullptr;
            port_info->maybe_midi_buffer  = nullptr;
            if (auto audio_port = std::dynamic_pointer_cast<AudioPortInterface<float>>(port_info->port)) {
                port_info->maybe_audio_buffer = audio_port->get_buffer(n_frames);
            }
        }
    }

    {
        // Connect buffers to loops
        for (auto &loop_info : g_loops) {
            if (auto audio_loop = std::dynamic_pointer_cast<AudioLoop<float>>(loop_info->loop)) {
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

    // Update state
    update_atomic_state();
}



// INTERFACE IMPLEMENTATIONS

extern "C" {

jack_client_t* initialize(
    unsigned n_loops,
    unsigned n_ports,
    unsigned n_mixed_output_ports, // TODO implement
    float loop_len_seconds, // TODO remove
    unsigned *loops_to_ports_mapping,
    int *loops_hard_sync_mapping,
    int *loops_soft_sync_mapping,
    int *ports_to_mixed_outputs_mapping, // TODO implement
    unsigned *ports_midi_enabled, // TODO implement
    const char **input_port_names,
    const char **output_port_names,
    const char **mixed_output_port_names, // TODO implement
    const char **midi_input_port_names, // TODO implement
    const char **midi_output_port_names, // TODO implement
    const char *client_name,
    unsigned latency_buf_size, // TODO implement
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
    g_update_callback = update_cb;
    g_abort_callback = abort_cb;

    if (features & Profiling) {
        throw std::runtime_error ("Profiling not implemented.");
    }

    g_cmd_queue.reset();
    g_loops.clear();
    g_loop_input_ports.clear();
    g_loop_output_ports.clear();
    
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
            auto input_port = std::make_shared<PortInfo>(g_audio->open_audio_port(audio_input_name, PortDirection::Input));
            auto output_port = std::make_shared<PortInfo>(g_audio->open_audio_port(audio_output_name, PortDirection::Output));
            g_loop_input_ports.push_back(input_port);
            g_loop_output_ports.push_back(output_port);
            // TODO MIDI
        }
    }
    
    {
        // Make port mappings.
        for (size_t idx=0; idx < n_loops; idx++) {
            auto port_idx = loops_to_ports_mapping[idx];
            if (port_idx >= 0) {
                g_loops[idx]->maybe_input_port = g_loop_input_ports[port_idx];
                g_loops[idx]->maybe_output_port = g_loop_output_ports[port_idx];
            }
        }
    }

    {
        // Make loop sync connections.
        for (size_t idx=0; idx < n_loops; idx++) {
            auto soft = loops_soft_sync_mapping[idx];
            auto hard = loops_hard_sync_mapping[idx];
            if (soft >= 0 && soft != idx) { g_loops[idx]->loop->set_soft_sync_source(g_loops[soft]->loop); } // TODO sync self
            if (hard >= 0 && hard != idx) { g_loops[idx]->loop->set_hard_sync_source(g_loops[hard]->loop); }
        }
    }

    // Initialize the atomic state used for status reporting.
    g_atomic_state.resize(n_loops, n_ports);

    g_audio->start();

    return g_audio->get_client();
}

unsigned get_sample_rate() {
    if (g_audio == nullptr) {
        throw std::runtime_error("Requesting sample-rate before back-end initialized");
    }
    return g_audio->get_sample_rate();
}

void request_update() {
    StateReport s;
    g_atomic_state.copy_to(s);

    if(g_update_callback) {
        int r = g_update_callback(
            g_loops.size(),
            g_loop_input_ports.size(), // Because we regard input+output as a "port" in this context
            g_audio->get_sample_rate(),
            s.states.data(),
            s.next_states.data(),
            s.next_states_countdown.data(),
            s.lengths.data(),
            s.positions.data(),
            s.loop_volumes.data(),
            s.port_volumes.data(),
            s.passthroughs.data(),
            s.latencies.data(),
            s.ports_muted.data(),
            s.port_inputs_muted.data(),
            s.loop_output_peaks.data(),
            s.port_output_peaks.data(),
            s.port_input_peaks.data(),
            s.loop_n_output_events_since_last_update.data(),
            s.port_n_input_events_since_last_update.data(),
            s.port_n_output_events_since_last_update.data()
        );
    }

    g_atomic_state.reset_peaks();
}

unsigned get_loop_midi_data(
    unsigned loop_idx,
    unsigned char **data_out
) {
    std::cerr << "get_loop_midi_data unimplemented\n";
    return 0;
}

unsigned set_loop_midi_data(
    unsigned loop_idx,
    unsigned char *data,
    unsigned data_len
) {
    std::cerr << "set_loop_midi_data unimplemented\n";
    return 0;
}

void freeze_midi_buffer (unsigned loop_idx) {
    std::cerr << "freeze_midi_buffer unimplemented\n";
}

void set_loops_length(unsigned *loop_idxs,
                      unsigned n_loop_idxs,
                      unsigned length) {
    std::atomic<bool> finished = false;
    push_command([loop_idxs, length, n_loop_idxs, &finished]() {
        // TODO: check that we are not recording
        for(size_t idx=0; idx<n_loop_idxs; idx++) {
            g_loops[loop_idxs[idx]]->set_length(length);
        }
        finished = true;
    });
    while(!finished) {}
}

int do_port_action(
    unsigned port_idx,
    port_action_t action,
    float maybe_arg
) {
    std::cerr << "do_port_action unimplemented\n";
    return 0;
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

    auto check_args = [n_args, action](int n_needed) {
        if (n_args != n_needed) {
            throw std::runtime_error("do_loop_action: incorrect # of args");
        }
    };

    auto plan_states = [with_soft_sync](std::vector<unsigned> loops,
                                        loop_state_t state_1,
                                        size_t delay_1 = 0,
                                        std::optional<loop_state_t> state_2 = std::nullopt,
                                        size_t delay_2 = 0,
                                        std::optional<size_t> set_position_to = std::nullopt,
                                        std::optional<size_t> set_length_to = std::nullopt) {
        for (auto const& idx : loops) {
            g_loops[idx]->clear_planned_transitions();
            g_loops[idx]->plan_transition(state_1, delay_1);
            if (state_2.has_value()) {
                g_loops[idx]->plan_transition(state_2.value(), delay_2);
            }
            if (!with_soft_sync) {
                g_loops[idx]->trigger(false);
            }
        }
    };

    int arg1_i = n_args >= 1 ? std::round(maybe_args[0]) : 0;
    int arg2_i = n_args >= 2 ? std::round(maybe_args[1]) : 0;
    int arg3_i = n_args >= 3 ? std::round(maybe_args[2]) : 0;
    float arg1_f = n_args >= 1 ? maybe_args[0] : 0.0f;
    float arg2_f = n_args >= 2 ? maybe_args[1] : 0.0f;
    float arg3_f = n_args >= 3 ? maybe_args[2] : 0.0f;

    switch(action) {
        case DoRecord:
            check_args(1);
            cmd = [idxs, plan_states, arg1_i]() {
                plan_states(idxs, Recording, arg1_i, std::nullopt, 0, 0, 0);
            };
            break;
        case DoRecordNCycles:
            check_args(3);
            cmd = [idxs, plan_states, arg1_i, arg2_i, arg3_i]() {
                plan_states(idxs, Recording, arg1_i, (loop_state_t)arg3_i, arg2_i-1);
            };
            break;
        case DoPlay:
            check_args(1);
            cmd = [idxs, plan_states, arg1_i]() {
                plan_states(idxs, Playing, arg1_i);
            };
            break;
        case DoPlayMuted:
            check_args(1);
            cmd = [idxs, plan_states, arg1_i]() {
                plan_states(idxs, PlayingMuted, arg1_i);
            };
            break;
        case DoStop:
            check_args(1);
            cmd = [idxs, plan_states, arg1_i]() {
                plan_states(idxs, Stopped, arg1_i);
            };
            break;
        case DoClear:
            check_args(0);
            cmd = [idxs, plan_states]() {
                plan_states(idxs, Stopped, 0, std::nullopt, 0, 0, 0);
            };
            break;
        case SetLoopVolume:
            check_args(1);
            cmd = [idxs, arg1_f]() {
                std::cerr << "SetLoopVolume action not implemented\n";
                // TODO implement
                // for (auto const& idx: idxs) {
                //     g_loop_volumes(idx) = arg1_f;
                // }
            };
            break;
        default:
        break;
    }

    if(cmd) { push_command(cmd); }
    return 0;
}

unsigned get_loop_data_rms(
    unsigned loop_idx,
    unsigned from_sample,
    unsigned to_sample,
    unsigned samples_per_bin,
    float **data_out
) {
    std::cerr << "get_loop_data_rms unimplemented\n";
    return 0;
}

void shoopdaloop_free(void* ptr) {
    free (ptr);
}

void process_slow_midi() {
    // TODO
}

void send_slow_midi(
    jack_port_t *port,
    uint8_t len,
    uint8_t *data
) {
    std::cerr << "send_slow_midi unimplemented\n";
}

void set_slow_midi_port_received_callback(
    jack_port_t *port,
    SlowMIDIReceivedCallback callback
) {
    std::cerr << "set_slow_midi_port_received_callback unimplemented\n";
}

void destroy_slow_midi_port(jack_port_t *port) {
    std::cerr << "destroy_slow_midi_port unimplemented\n";
}

jack_port_t *create_slow_midi_port(
    const char* name,
    slow_midi_port_kind_t kind
) {
    std::cerr << "create_slow_midi_port unimplemented\n";
    return nullptr;
}

jack_port_t* get_port_output_handle(unsigned port_idx) {
    return std::dynamic_pointer_cast<JackAudioPort>(g_loop_output_ports[port_idx]->port)->get_jack_port();
}

jack_port_t* get_port_input_handle(unsigned port_idx) {
    return std::dynamic_pointer_cast<JackAudioPort>(g_loop_input_ports[port_idx]->port)->get_jack_port();
}

void remap_port_input(unsigned port, unsigned input_source) {
    std::cerr << "remap_port_input unimplemented\n";
}

void reset_port_input_remapping(unsigned port) {
    std::cerr << "reset_port_input_remapping unimplemented\n";
}

void terminate() {
    g_audio = nullptr;
    g_cmd_queue.reset();
}

int load_loop_data(
    unsigned loop_idx,
    unsigned len,
    float *data
) {
    std::cerr << "load_loop_data unimplemented\n";
    return 0;
}

unsigned get_loop_data(
    unsigned loop_idx,
    float ** data_out,
    unsigned do_stop
) {
    std::cerr << "get_loop_data unimplemented\n";
    return 0;
}

void set_storage_lock(unsigned int value) {
    std::cerr << "set_storage_lock unimplemented\n";
}

} // extern "C"