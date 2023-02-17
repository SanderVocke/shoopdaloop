#include <HalideBuffer.h>
#include <algorithm>
#include <cmath>
#include <jack/types.h>
#include <jack/jack.h>
#include <jack/midiport.h>

#include "shoopdaloop_backend.h"
#include "shoopdaloop_loops.h"
#include "shoopdaloop_loops_profile.h"
#include "midi_buffer.h"
#include "midi_state_tracker.h"
#include <chrono>
#include <cstdlib>

#include <iostream>
#include <queue>
#include <ratio>
#include <string>
#include <vector>
#include <unistd.h>
#include <functional>
#include <atomic>
#include <thread>
#include <boost/lockfree/spsc_queue.hpp>
#include <memory>
#include <condition_variable>

#include <cmath>

using namespace Halide::Runtime;
using namespace std;

constexpr size_t slow_midi_port_queue_starting_size = 4096;
constexpr size_t g_max_midi_events_per_cycle = 256;
constexpr size_t g_midi_buffer_bytes = 81920;
constexpr size_t g_midi_max_merge_size = 4096;

backend_features_t g_features;
typedef decltype(&shoopdaloop_loops) loops_fn;
loops_fn g_loops_fn = shoopdaloop_loops;

// Last written output index in in/out pairs (goes tick tock)
uint8_t g_last_written_output_buffer_tick_tock;

// mode for each loop
Buffer<int8_t, 1> g_states[2];

// Planned states for each loop
Buffer<int8_t, 2> g_next_modes[2];
Buffer<int8_t, 2> g_next_modes_countdown[2];

// Position for each loop
Buffer<int32_t, 1> g_positions[2];

// Length for each loop
Buffer<int32_t, 1> g_lengths[2];

// Event handling
Buffer<int32_t, 1> g_n_port_events;
Buffer<int32_t, 2> g_port_event_timestamps_in;
Buffer<int32_t, 2> g_event_recording_timestamps_out;

// Sample input and output buffers
std::vector<float> g_samples_in_raw, g_samples_out_raw, g_mixed_samples_out_raw; //Use vectors here so we control the layout
Buffer<float, 2> g_samples_in, g_samples_out;

// Peak buffers
Buffer<float, 1> g_port_output_peaks;
Buffer<float, 1> g_port_input_peaks;
Buffer<float, 1> g_loop_output_peaks;

// Intermediate sample buffer which stores per loop (not port)
Buffer<float, 2> g_samples_out_per_loop;

// Mixed output samples
Buffer<float, 2> g_mixed_samples_out;

// Loop storage
Buffer<float, 2> g_storage;

// Latency buffer
Buffer<float, 2> g_latency_buf;
Buffer<int32_t, 1> g_port_recording_latencies;
Buffer<int32_t, 0> g_latency_buf_write_pos;

// Map of loop idx to port idx
Buffer<int32_t, 1> g_loops_to_ports;

// Sync mappings
Buffer<int32_t, 1> g_loops_hard_sync_mapping;
Buffer<int32_t, 1> g_loops_soft_sync_mapping;

// Port input mappings
Buffer<int32_t, 1> g_port_input_mappings;

// Mixed output mappings
Buffer<int32_t, 1> g_ports_to_mixed_outputs;

// Levels
Buffer<float, 1> g_passthroughs; // Per port
Buffer<float, 1> g_loop_volumes; // Per loop
Buffer<float, 1> g_port_volumes; // Per port
Buffer<int8_t, 1> g_ports_muted; // per port
Buffer<int8_t, 1> g_port_inputs_muted; // per port

int8_t g_storage_lock = 0;

struct MIDINotesStateAroundLoop : public MIDINotesState {
    MIDINotesStateAroundLoop() : MIDINotesState() {
        notes_active_at_end.reserve(16*128);
        reset();
    }

    // Inherited fields will track active/recent notes before record
    std::vector<ActiveNote> notes_active_at_end;
    std::chrono::time_point<std::chrono::high_resolution_clock> populated_at;

    // This struct also stores the settings on how to use this info per loop.
    bool auto_noteon_at_start = true;
    bool auto_noteoff_at_end = true;
    float pre_record_grace_period = 0.05;

    void reset() {
        MIDINotesState::reset();
        notes_active_at_end.clear();
    }

    void populate_from(MIDIStateTracker &other) {
        populated_at = std::chrono::high_resolution_clock::now();
        active_notes = other.active_notes;
        last_notes = other.last_notes;
        notes_active_at_end.clear();
    }

    // Takes the MIDI buffer for which the edge events are stored here,
    // and makes a "frozen" copy that has all the edge events inserted
    // as MIDI messages in the buffer.
    MIDIRingBuffer freeze_buffer(MIDIRingBuffer &other) const {
        MIDIRingBuffer retval(other.data.size());
        generate_start_msgs([&retval](uint8_t* bytes, unsigned len) {
            retval.put({ .time=0, .size=3 }, bytes);
        });
        other.reset_cursor();
        for(;other.peek_cursor_metadata();) {
            auto metadata = other.peek_cursor_metadata();
            auto data = other.peek_cursor_event_data();
            retval.put(*metadata, data);
            if(!other.increment_cursor()) { break; }
        }
        generate_end_msgs([&retval](uint8_t* bytes, unsigned len) {
            retval.put({ .time=0, .size=3 }, bytes);
        });
        return retval;
    }

    // Call the callback once in order for all the messages which should be sent
    // before to properly start the MIDI loop.
    void generate_start_msgs(std::function<void(uint8_t*, unsigned)> cb) const {
        // Now, play any complete notes that fell within the pre-record grace period at loop 0.
        if(pre_record_grace_period > 0.0f) {
            auto now = std::chrono::high_resolution_clock::now();
            for(auto const& msg: last_notes) {
                std::chrono::duration<float, std::milli> t_diff_to_record_start = populated_at - msg.on_t;
                float t_diff_to_record_start_s = t_diff_to_record_start.count() / 1000.0f;
                if(t_diff_to_record_start_s <= pre_record_grace_period) {
                    std::cout << "Playing grace-period note " << msg.note << std::endl;
                    uint8_t msg_bytes[3] = { (uint8_t)(0x90 + msg.channel),
                                            (uint8_t)msg.note,
                                            (uint8_t)msg.velocity };
                    cb(msg_bytes, 3); // noteOn
                    msg_bytes[0] = (uint8_t)(0x80 + msg.channel);
                    cb(msg_bytes, 3); // noteOff
                }
            }
        }
        // Now, play noteOns for any messages for which we found an unstarted noteOff during recording.
        if(auto_noteon_at_start) {
            for(auto const& msg: active_notes) {
                std::cout << "Playing auto note-on for note " << msg.note << std::endl;
                uint8_t msg_bytes[3] = { (uint8_t)(0x90 + msg.channel),
                                            (uint8_t)msg.note,
                                            (uint8_t)msg.velocity };
                cb(msg_bytes, 3);
            }
        }
    }

    // Call the callback once in order for all the messages which should be sent
    // to properly terminate the MIDI loop.
    void generate_end_msgs(std::function<void(uint8_t*, unsigned)> cb) const {
        // First, auto-play noteoff messages for any active notes.
        if(auto_noteoff_at_end) {
            for(auto const& msg: notes_active_at_end) {
                std::cout << "Playing auto note-off for note " << msg.note << std::endl;
                uint8_t msg_bytes[3] = { (uint8_t)(0x80 + msg.channel),
                                         (uint8_t)msg.note,
                                         (uint8_t)msg.velocity };
                cb(msg_bytes, 3);
            }
        }
    }
};

std::vector<jack_port_t*> g_input_ports;
std::vector<jack_port_t*> g_output_ports;
std::vector<jack_port_t*> g_mixed_output_ports;
std::vector<jack_port_t*> g_maybe_midi_input_ports;
std::vector<jack_port_t*> g_maybe_midi_output_ports;
std::vector<MIDIStateTracker> g_midi_input_state_trackers;
std::vector<MIDINotesStateAroundLoop> g_midi_input_notes_state_around_record;
std::vector<jack_nframes_t> g_input_port_latencies;
std::vector<jack_nframes_t> g_output_port_latencies;

size_t g_n_loops;
size_t g_n_ports;
size_t g_n_mixed_output_ports;
size_t g_loop_len;
size_t g_buf_nframes;
size_t g_sample_rate;

std::vector<jack_default_audio_sample_t*> g_input_bufs, g_output_bufs, g_mixed_output_bufs;
std::vector<void*> g_midi_input_bufs, g_midi_output_bufs;

jack_client_t* g_jack_client;

AbortCallback g_abort_callback;
UpdateCallback g_update_cb;

std::thread g_reporting_thread;
std::vector<MIDIRingBuffer> g_loop_midi_buffers;
std::vector<MIDIRingBuffer> g_temporary_merging_buffers;

constexpr unsigned g_midi_ring_buf_size = 81920;

// A structure of atomic scalars is used to communicate the latest
// mode back from the Jack processing thread to the main thread.
struct atomic_state {
    std::vector<std::atomic<loop_mode_t>> states;
    std::vector<std::vector<std::atomic<loop_mode_t>>> next_modes;
    std::vector<std::vector<std::atomic<int8_t>>> next_modes_countdown;
    std::vector<std::atomic<int32_t>> positions, lengths, latencies;
    std::vector<std::atomic<float>> passthroughs, loop_volumes, port_volumes, port_input_peaks, port_output_peaks, loop_output_peaks;
    std::vector<std::atomic<int8_t>> ports_muted, port_inputs_muted;
    std::vector<std::atomic<unsigned>> loop_n_output_events_since_last_update,
        port_n_input_events_since_last_update,
        port_n_output_events_since_last_update;
};
atomic_state g_atomic_state;

std::condition_variable g_terminate_cv;
std::mutex g_terminate_cv_m;

// A lock-free queue is used to pass commands to the audio
// processing thread in the form of functors.
constexpr unsigned command_queue_len = 1024;
boost::lockfree::spsc_queue<std::function<void()>> g_cmd_queue(command_queue_len);
void push_command(std::function<void()> cmd) {
    while (!g_cmd_queue.push(cmd)) { 
        std::this_thread::sleep_for(std::chrono::microseconds(100));
    }
}

// Another lock-free queue is used to pass benchmarking information
// from the processing thread to an optional reporter.
constexpr unsigned benchmark_queue_len = 10;
struct benchmark_info {
    float process_slow_midi_us;
    float request_buffers_us;
    float command_processing_us;
    float input_copy_us;
    float processing_us;
    float output_copy_us;
    float total_us;
};
boost::lockfree::spsc_queue<benchmark_info> g_benchmark_queue(benchmark_queue_len);

// Keep track of the slow midi ports that have been opened.
struct _slow_midi_port {
    jack_port_t* jack_port;
    std::string name;
    slow_midi_port_kind_t kind;
    std::vector<std::vector<uint8_t>> queue;
    SlowMIDIReceivedCallback maybe_rcv_callback;
};
std::vector<_slow_midi_port> g_slow_midi_ports;

extern "C" {

bool is_playing_state (loop_mode_t mode) {
    return mode == Playing;
}

// Process the slow midi ports
void process_slow_midi_ports(jack_nframes_t nframes) {
    for(auto &port : g_slow_midi_ports) {

        if(port.jack_port) {
            auto buf = jack_port_get_buffer(port.jack_port, nframes);

            if(port.kind == Input) {
                auto n_events = jack_midi_get_event_count(buf);
                for(size_t i=0; i<n_events; i++) {
                    jack_midi_event_t e;
                    jack_midi_event_get(&e, buf, i);
                    std::vector<uint8_t> msg(e.size); // TODO: don't allocate here
                    memcpy((void*)msg.data(), (void*)e.buffer, e.size);
                    port.queue.push_back(msg);
                }
            } else {
                jack_midi_clear_buffer(buf);
                for(auto &elem : port.queue) {
                    jack_midi_event_write(buf, 0, (jack_midi_data_t*)elem.data(), elem.size());
                }
                port.queue.clear();
            }
        }
    }
}

// The latency update callback to be called by Jack.
void jack_latency_callback(jack_latency_callback_mode_t mode, void * arg) {
    if (mode == JackCaptureLatency) {
        for(size_t i=0; i<g_input_ports.size(); i++) {
            jack_latency_range_t range;
            jack_port_get_latency_range(g_input_ports[i], JackCaptureLatency, &range);
            g_input_port_latencies[i] = range.min;
            // TODO: possible race condition? Although this should be a read-only from
            // the other side.
            // TODO: rethink the whole latency approach
            //g_port_recording_latencies(i) = g_input_port_latencies[i] + g_output_port_latencies[i];
        }
    } else if (mode == JackPlaybackLatency) {
        for(size_t i=0; i<g_input_ports.size(); i++) {
            jack_latency_range_t range;
            jack_port_get_latency_range(g_output_ports[i], JackPlaybackLatency, &range);
            g_output_port_latencies[i] = range.min;
            // TODO: possible race condition? Although this should be a read-only from
            // the other side.
            // TODO: rethink the whole latency approach
            //g_port_recording_latencies(i) = g_input_port_latencies[i] + g_output_port_latencies[i];
        }
    }
}

// The processing callback to be called by Jack.
int jack_process (jack_nframes_t nframes, void *arg) {
    static auto last_measurement = std::chrono::high_resolution_clock::now();
    static size_t n_measurements = 0;
    static float slow_midi_time = 0.0,
                 get_buffers_time = 0.0,
                 cmd_time = 0.0,
                 in_time = 0.0,
                 proc_time = 0.0,
                 out_time = 0.0,
                 total_time = 0.0;
    static uint8_t tick = 0;
    static uint8_t tock = 1;

    jack_default_audio_sample_t *buf;

    auto start = std::chrono::high_resolution_clock::now();

    process_slow_midi_ports(nframes);

    auto did_slow_midi = std::chrono::high_resolution_clock::now();

    // Get buffers
    for(int i=0; i<g_input_ports.size(); i++) {
            g_input_bufs[i] =
                g_input_ports[i] ?
                (jack_default_audio_sample_t*) jack_port_get_buffer(g_input_ports[i], nframes) :
                nullptr;
            g_output_bufs[i] =
                g_output_ports[i] ?
                (jack_default_audio_sample_t*) jack_port_get_buffer(g_output_ports[i], nframes) :
                nullptr;
            g_midi_input_bufs[i] =
                g_maybe_midi_input_ports[i] ?
                jack_port_get_buffer(g_maybe_midi_input_ports[i], nframes) :
                nullptr;
            g_midi_output_bufs[i] =
                g_maybe_midi_output_ports[i] ?
                jack_port_get_buffer(g_maybe_midi_output_ports[i], nframes) :
                nullptr;
    }
    for(int i=0; i<g_mixed_output_ports.size(); i++) {
        g_mixed_output_bufs[i] =
            g_mixed_output_ports[i] ?
            (jack_default_audio_sample_t*) jack_port_get_buffer(g_mixed_output_ports[i], nframes) :
                nullptr;
    }

    auto got_buffers = std::chrono::high_resolution_clock::now();

    // Process commands
    std::function<void()> cmd;
    while(g_cmd_queue.pop(cmd)) {
        cmd();
    }

    auto did_cmds = std::chrono::high_resolution_clock::now();

    // Copy samples into the combined input buffer.
    for(int i=0; i<g_input_ports.size(); i++) {
        if(g_input_bufs[i]) {
            memcpy((void*)(&g_samples_in_raw[g_buf_nframes*i]), (void*)g_input_bufs[i], sizeof(float) * nframes);
        }
    }

    // For incoming MIDI on the input ports:
    // - Store the timestamps for the loops backend to determine whether+where to record them
    // - Store the events into a temporary buffer to combine with playback events later
    // - Notify the MIDI mode trackers so they can keep track of recent & active notes played
    for(int i=0; i<g_input_ports.size(); i++) {
        auto buf = g_midi_input_bufs[i];
        g_temporary_merging_buffers[i].clear();
        if(buf && !g_port_inputs_muted(i)) {
            auto n_events = jack_midi_get_event_count(buf);
            g_n_port_events(i) = n_events;
            for(size_t event_idx=0; event_idx<n_events; event_idx++) {
                jack_midi_event_t e;
                jack_midi_event_get(&e, buf, event_idx);
                g_port_event_timestamps_in(event_idx, i) = e.time;
                
                g_temporary_merging_buffers[i].put({
                    .time = e.time,
                    .size = (unsigned) e.size
                }, (uint8_t*)e.buffer);
                g_midi_input_state_trackers[i].process_msg({
                    .time = 0,
                    .size = e.size,
                    .buffer = (uint8_t*)e.buffer
                });
            }
        }
    }

    auto did_input = std::chrono::high_resolution_clock::now();

    // Execute the loop engine.
    if(g_loops_fn) {
        g_loops_fn(
            g_samples_in,
            g_latency_buf,
            g_latency_buf_write_pos(),
            g_port_recording_latencies,
            g_states[tick],
            g_positions[tick],
            g_lengths[tick],
            g_next_modes[tick],
            g_next_modes_countdown[tick],
            g_storage,
            g_loops_to_ports,
            g_loops_hard_sync_mapping,
            g_loops_soft_sync_mapping,
            g_port_input_mappings,
            g_ports_to_mixed_outputs,
            g_n_mixed_output_ports,
            g_passthroughs,
            g_port_volumes,
            g_ports_muted,
            g_port_inputs_muted,
            g_loop_volumes,
            g_port_event_timestamps_in,
            g_n_port_events,
            nframes,
            g_loop_len,
            g_storage_lock,
            g_samples_out,
            g_mixed_samples_out,
            g_port_input_peaks,
            g_port_output_peaks,
            g_loop_output_peaks,
            g_latency_buf,
            g_latency_buf_write_pos,
            g_samples_out_per_loop,
            g_states[tock],
            g_positions[tock],
            g_lengths[tock],
            g_next_modes[tock],
            g_next_modes_countdown[tock],
            g_event_recording_timestamps_out,
            g_storage
        );
    }
    g_last_written_output_buffer_tick_tock = tock;

    // MIDI processing.
    // Because JACK needs ordered MIDI data and we don't want to do any sorting (slow),
    // we do all the output processing - passthrough, playback - in one loop, in-order.
    // Only recording is done separately.
    for(int loop_idx=0; loop_idx<g_n_loops; loop_idx++) {
        // TODO loop muting

        auto port_idx = g_loops_to_ports(loop_idx);
        auto &out_buf = g_midi_output_bufs[port_idx];
        auto &out_port = g_maybe_midi_output_ports[port_idx];
        auto const& loop_volume = g_loop_volumes(loop_idx);
        auto const& port_volume = g_port_volumes(port_idx);
        auto const& port_muted = g_ports_muted(port_idx);
        auto const& port_in_muted = g_port_inputs_muted(port_idx);
        auto &midi_storage = g_loop_midi_buffers[loop_idx];
        auto &pos_before = g_positions[tick](loop_idx);
        auto &pos_after = g_positions[tock](loop_idx);
        auto &mode_before = g_states[tick][loop_idx];
        auto &mode_after = g_states[tock][loop_idx];
        auto &n_events = g_n_port_events(port_idx);
        auto mapped_port_idx = (g_port_input_mappings(port_idx) >= 0 && g_port_input_mappings(port_idx) != port_idx) ?
                g_port_input_mappings(port_idx) : port_idx;
        auto &maybe_jack_buf = g_midi_input_bufs[mapped_port_idx];

        // Add playback events into the merged port sorting buffer
        midi_event_metadata_t next_meta;
        uint8_t *next_data = nullptr;
        auto update = [&]() {
            if (g_states[tick](loop_idx) != Playing && g_states[tock](loop_idx) != Playing) {
                next_data = nullptr;
                return false;
            }
            // TODO correct behavior on mode changes
            while (true) {
                auto from = g_positions[tick](loop_idx);
                auto next = midi_storage.peek_cursor_metadata();
                if (midi_storage.n_events > 0) {
                }
                if (next && next->time >= from && next-> time < (from + nframes)) {
                    next_meta = *next;
                    next_meta.time -= from;
                    next_data = midi_storage.peek_cursor_event_data();
                    return true;
                }
                else if (next && next->time > (from + nframes)) {
                    next_data = nullptr;
                    return false;
                }
                else if (next == nullptr) {
                    midi_storage.reset_cursor();
                    next_data = nullptr;
                    return false;
                }

                if( !midi_storage.increment_cursor() ) {
                    midi_storage.reset_cursor();
                    next_data = nullptr;
                    return false;
                }
            }
        };

        while(update()) {
            g_temporary_merging_buffers[port_idx].insert(next_meta, next_data);
            if (!midi_storage.increment_cursor()) { midi_storage.reset_cursor(); break; }
        }

        // MIDI Recording: loops backend told us at which timestamps to record
        // the incoming messages, now actually store them
        for(int event_idx=0; event_idx<n_events; event_idx++) {
            auto storage_ts = g_event_recording_timestamps_out(event_idx, loop_idx);
            if(storage_ts >= 0 && maybe_jack_buf) {
                jack_midi_event_t e;
                jack_midi_event_get(&e, maybe_jack_buf, event_idx);
                if (!g_loop_midi_buffers[loop_idx].put({
                    .time = (unsigned) storage_ts,
                    .size = (unsigned) e.size
                }, (uint8_t*)e.buffer)) {
                    std::cerr << "Failed to record MIDI event @ loop " << loop_idx << std::endl;
                }
            }
        }
    }
    
    // We have now collected and sorted the MIDI output streams per port.
    // Send them over the JACK port.
    for (size_t i=0; i<g_n_ports; i++) {
        auto &maybe_jack_buf = g_midi_output_bufs[i];
        auto &ringbuf = g_temporary_merging_buffers[i];
        if(maybe_jack_buf && !g_ports_muted(i)) {
            jack_midi_clear_buffer(maybe_jack_buf);
            ringbuf.reset_cursor();
            auto maybe_msg = ringbuf.peek_cursor_metadata();
            while(maybe_msg) {
                jack_midi_event_write(maybe_jack_buf, maybe_msg->time, ringbuf.peek_cursor_event_data(), maybe_msg->size);
                if(!ringbuf.increment_cursor()) { break; }
                maybe_msg = ringbuf.peek_cursor_metadata();
            }
        }
    }

    // If we just started recording, set up the active notes information just before recording.
    for(int loop_idx=0; loop_idx<g_n_loops; loop_idx++) {
        if(g_states[tick](loop_idx) != Recording && g_states[tock](loop_idx) == Recording) {
            g_midi_input_notes_state_around_record[loop_idx].populate_from(g_midi_input_state_trackers[loop_idx]);
        }
    }

    // If we just ended recording, set up the active notes information at recording end.
    for(int loop_idx=0; loop_idx<g_n_loops; loop_idx++) {
        if(g_states[tick](loop_idx) == Recording && g_states[tock](loop_idx) != Recording) {
            g_midi_input_notes_state_around_record[loop_idx].notes_active_at_end =
                g_midi_input_state_trackers[loop_idx].active_notes;
        }
    }

    // Debugging
    if (false) {
        for(int i=0; i<g_states[0].dim(0).extent(); i++) {
            if(g_states[tock](i) != g_states[tick](i)) {
                std::cout << "Loop " << i << ": " << (int)g_states[tick](i) << " -> "
                        << (int)g_states[tock](i) 
                        << " (-> " << (int)g_next_modes[tock](0, i) << ")"
                        << std::endl;
            }
        }
    }

    tock = (tock + 1) % 2;
    tick = (tick + 1) % 2;

    auto did_process = std::chrono::high_resolution_clock::now();

    // Get output port buffers and copy samples into them.
    for(int i=0; i<g_output_ports.size(); i++) {
        if(g_output_bufs[i]) {
            memcpy((void*)g_output_bufs[i], (void*)(&g_samples_out_raw[g_buf_nframes*i]), sizeof(float) * nframes);
        }
    }
    for(int i=0; i<g_mixed_output_ports.size(); i++) {
        if(g_mixed_output_bufs[i]) {
            memcpy((void*)g_mixed_output_bufs[i], (void*)(&g_mixed_samples_out_raw[g_buf_nframes*i]), sizeof(float) * nframes);
        }
    }

    // Copy states to atomic for read-out
    for(int i=0; i<g_n_loops; i++) {
        g_atomic_state.states[i] = (loop_mode_t) g_states[g_last_written_output_buffer_tick_tock](i);
        g_atomic_state.lengths[i] = g_lengths[g_last_written_output_buffer_tick_tock](i);
        g_atomic_state.loop_volumes[i] = g_loop_volumes(i);
        g_atomic_state.positions[i] = g_positions[g_last_written_output_buffer_tick_tock](i);
        g_atomic_state.loop_output_peaks[i] = g_loop_output_peaks(i);
        g_atomic_state.next_modes[i][0] = (loop_mode_t) g_next_modes[0](0, i);
        g_atomic_state.next_modes[i][1] = (loop_mode_t) g_next_modes[1](1, i);
        g_atomic_state.next_modes_countdown[i][0] = (loop_mode_t) g_next_modes_countdown[0](0, i);
        g_atomic_state.next_modes_countdown[i][1] = (loop_mode_t) g_next_modes_countdown[1](1, i);
    }
    for(int i=0; i<g_n_ports; i++) {
        g_atomic_state.passthroughs[i] = g_passthroughs(i);
        g_atomic_state.port_volumes[i] = g_port_volumes(i);
        g_atomic_state.ports_muted[i] = g_ports_muted(i);
        g_atomic_state.port_inputs_muted[i] = g_port_inputs_muted(i);
        g_atomic_state.latencies[i] = g_port_recording_latencies(i);
        g_atomic_state.port_input_peaks[i] = g_port_input_peaks(i);
        g_atomic_state.port_output_peaks[i] = g_port_output_peaks(i);
    }

    auto did_output = std::chrono::high_resolution_clock::now();

    // Benchmarking
    slow_midi_time += std::chrono::duration_cast<std::chrono::microseconds>(did_slow_midi - start).count();
    get_buffers_time += std::chrono::duration_cast<std::chrono::microseconds>(got_buffers - did_slow_midi).count();
    cmd_time += std::chrono::duration_cast<std::chrono::microseconds>(did_cmds - got_buffers).count();
    in_time += std::chrono::duration_cast<std::chrono::microseconds>(did_input - did_cmds).count();
    proc_time += std::chrono::duration_cast<std::chrono::microseconds>(did_process - did_input).count();
    out_time += std::chrono::duration_cast<std::chrono::microseconds>(did_output - did_process).count();
    total_time += std::chrono::duration_cast<std::chrono::microseconds>(did_output - start).count();
    n_measurements++;

    if (std::chrono::duration_cast<std::chrono::milliseconds>(did_output - last_measurement).count() > 1000.0) {
        g_benchmark_queue.push(benchmark_info {
            .process_slow_midi_us = slow_midi_time / (float)n_measurements,
            .request_buffers_us = get_buffers_time / (float)n_measurements,
            .command_processing_us = cmd_time / (float)n_measurements,
            .input_copy_us = in_time / (float)n_measurements,
            .processing_us = proc_time / (float)n_measurements,
            .output_copy_us = out_time / (float)n_measurements,
            .total_us = total_time / (float)n_measurements
        });
        in_time = out_time = proc_time = total_time = cmd_time = get_buffers_time = slow_midi_time = 0.0f;
        n_measurements = 0;
        last_measurement = did_output;
    }

    return 0;
}

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
    g_n_loops = n_loops;
    g_n_ports = n_ports;
    g_n_mixed_output_ports = n_mixed_output_ports;
    g_features = features;

    switch(g_features) {
        case Profiling:
            g_loops_fn = shoopdaloop_loops_profile;
            break;
        default:
        break;
    }

    // Create a JACK client
    jack_status_t status;
    g_jack_client = jack_client_open(client_name, JackNullOption, &status, nullptr);
    if(g_jack_client == nullptr) {
        std::cerr << "Backend: given a null client." << std::endl;
        return nullptr;
    }

    // Get JACK buffer size and sample rate
    g_buf_nframes = 
        jack_port_type_get_buffer_size(g_jack_client, JACK_DEFAULT_AUDIO_TYPE) / sizeof(jack_default_audio_sample_t);
    g_sample_rate =
        (size_t) jack_get_sample_rate(g_jack_client);
    std::cout << "Backend: JACK: buf size " << g_buf_nframes << " @ " << g_sample_rate << "samples/s" << std::endl;

    g_loop_len = (size_t)((float) g_sample_rate * loop_len_seconds);
    g_abort_callback = nullptr;
    g_last_written_output_buffer_tick_tock = 0;
    g_cmd_queue.reset();

    // Allocate buffers
    g_states[0] = Buffer<int8_t, 1>(n_loops);
    g_states[1] = Buffer<int8_t, 1>(n_loops);
    g_next_modes[0] = Buffer<int8_t, 2>(2, n_loops);
    g_next_modes[1] = Buffer<int8_t, 2>(2, n_loops);
    g_next_modes_countdown[0] = Buffer<int8_t, 2>(2, n_loops);
    g_next_modes_countdown[1] = Buffer<int8_t, 2>(2, n_loops);
    g_positions[0] = Buffer<int32_t, 1>(n_loops);
    g_positions[1] = Buffer<int32_t, 1>(n_loops);
    g_lengths[0] = Buffer<int32_t, 1>(n_loops);
    g_lengths[1] = Buffer<int32_t, 1>(n_loops);
    g_samples_in_raw.resize(n_ports*g_buf_nframes);
    g_samples_out_raw.resize(n_ports*g_buf_nframes);
    g_mixed_samples_out_raw.resize(n_mixed_output_ports*g_buf_nframes);
    g_samples_out_per_loop = Buffer<float, 2>(g_buf_nframes, n_loops);
    g_storage = Buffer<float, 2>(g_loop_len, n_loops);
    g_loops_to_ports = Buffer<int32_t, 1>(n_loops);
    g_loops_hard_sync_mapping = Buffer<int32_t, 1>(n_loops);
    g_loops_soft_sync_mapping = Buffer<int32_t, 1>(n_loops);
    g_passthroughs = Buffer<float, 1>(n_ports);
    g_port_volumes = Buffer<float, 1>(n_ports);
    g_ports_muted = Buffer<int8_t, 1>(n_ports);
    g_port_inputs_muted = Buffer<int8_t, 1>(n_ports);
    g_loop_volumes = Buffer<float, 1>(n_loops);
    g_port_input_mappings = Buffer<int32_t, 1>(n_ports);
    g_latency_buf_write_pos = Buffer<int32_t>::make_scalar();
    g_latency_buf_write_pos() = 0;
    g_port_recording_latencies = Buffer<int32_t>(n_ports);
    g_latency_buf = Buffer<float, 2>(latency_buf_size, n_ports);
    g_port_input_peaks = Buffer<float, 1>(n_ports);
    g_port_output_peaks = Buffer<float, 1>(n_ports);
    g_loop_output_peaks = Buffer<float, 1>(n_loops);
    g_port_event_timestamps_in = Buffer<int32_t, 2>(g_max_midi_events_per_cycle, n_ports);
    g_n_port_events = Buffer<int32_t, 1>(n_ports);
    g_event_recording_timestamps_out = Buffer<int32_t, 2>(g_max_midi_events_per_cycle, n_loops);
    g_loop_midi_buffers = std::vector<MIDIRingBuffer>(n_loops);
    g_temporary_merging_buffers = std::vector<MIDIRingBuffer>(n_ports);
    g_ports_to_mixed_outputs = Buffer<int, 1>(n_ports);
    g_storage_lock = 0;

    g_samples_in = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)g_samples_in_raw.data(),
        g_buf_nframes, n_ports
    );
    g_samples_out = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)g_samples_out_raw.data(),
        g_buf_nframes, n_ports
    );
    g_mixed_samples_out = Buffer<float, 2>(
        halide_type_t {halide_type_float, 32, 1},
        (void*)g_mixed_samples_out_raw.data(),
        g_buf_nframes, n_mixed_output_ports
    );

    // Allocate atomic mode
    g_atomic_state.states = std::vector<std::atomic<loop_mode_t>>(n_loops);
    g_atomic_state.lengths = std::vector<std::atomic<int32_t>>(n_loops);
    g_atomic_state.loop_volumes = std::vector<std::atomic<float>>(n_loops);
    g_atomic_state.port_volumes = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.passthroughs = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.positions = std::vector<std::atomic<int32_t>>(n_loops);
    g_atomic_state.ports_muted = std::vector<std::atomic<int8_t>>(n_ports);
    g_atomic_state.port_inputs_muted = std::vector<std::atomic<int8_t>>(n_ports);
    g_atomic_state.latencies = std::vector<std::atomic<int32_t>>(n_ports);
    g_atomic_state.port_input_peaks = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.port_output_peaks = std::vector<std::atomic<float>>(n_ports);
    g_atomic_state.loop_output_peaks = std::vector<std::atomic<float>>(n_loops);
    g_atomic_state.next_modes = std::vector<std::vector<std::atomic<loop_mode_t>>>(n_loops);
    g_atomic_state.next_modes_countdown = std::vector<std::vector<std::atomic<int8_t>>>(n_loops);
    for(size_t i=0; i<n_loops; i++) {
        g_atomic_state.next_modes[i] = std::vector<std::atomic<loop_mode_t>>(2);
        g_atomic_state.next_modes_countdown[i] = std::vector<std::atomic<int8_t>>(2);
    }
    g_atomic_state.loop_n_output_events_since_last_update = std::vector<std::atomic<unsigned>>(n_loops);
    g_atomic_state.port_n_output_events_since_last_update = std::vector<std::atomic<unsigned>>(n_ports);
    g_atomic_state.port_n_input_events_since_last_update = std::vector<std::atomic<unsigned>>(n_ports);

    // Initialize loops
    for(size_t i=0; i<g_n_loops; i++) {
        g_states[0](i) = g_states[1](i) = Stopped;
        g_next_modes[0](0, i) = g_next_modes[0](1, i) = Stopped;
        g_next_modes[1](0, i) = g_next_modes[1](1, i) = Stopped;
        g_next_modes_countdown[0](0, i) = g_next_modes_countdown[0](1, i) = -1;
        g_next_modes_countdown[1](0, i) = g_next_modes_countdown[1](1, i) = -1;
        g_positions[0](i) = g_positions[1](i) = 0;
        g_lengths[0](i) = g_lengths[1](i) = 0;
        g_loops_to_ports(i) = loops_to_ports_mapping[i];
        g_loops_hard_sync_mapping(i) = loops_hard_sync_mapping[i] < 0 ?
            i : loops_hard_sync_mapping[i];
        g_loops_soft_sync_mapping(i) = loops_soft_sync_mapping[i];
        g_loop_volumes(i) = 1.0f;
        g_atomic_state.loop_n_output_events_since_last_update[i] = 0;
        g_loop_midi_buffers[i] = MIDIRingBuffer(g_midi_ring_buf_size);
    }
    
    // Initialize ports
    g_input_bufs = std::vector<jack_default_audio_sample_t*>(n_ports);
    g_output_bufs = std::vector<jack_default_audio_sample_t*>(n_ports);
    g_mixed_output_bufs = std::vector<jack_default_audio_sample_t*>(n_mixed_output_ports);
    g_midi_input_bufs = std::vector<void*>(n_ports);
    g_midi_output_bufs = std::vector<void*>(n_ports);
    g_input_ports.clear();
    g_output_ports.clear();
    g_maybe_midi_input_ports.clear();
    g_maybe_midi_output_ports.clear();
    g_midi_input_state_trackers.clear();
    g_midi_input_notes_state_around_record.clear();
    g_input_port_latencies.clear();
    g_output_port_latencies.clear();
    g_temporary_merging_buffers.clear();
    for(size_t i=0; i<n_mixed_output_ports; i++) {
        g_mixed_output_ports.push_back(jack_port_register(g_jack_client, mixed_output_port_names[i], JACK_DEFAULT_AUDIO_TYPE, JackPortIsOutput, 0));
    }
    size_t midi_idx = 0;
    for(size_t i=0; i<n_ports; i++) {
        g_port_input_mappings(i) = i;
        g_input_ports.push_back(jack_port_register(g_jack_client, input_port_names[i], JACK_DEFAULT_AUDIO_TYPE, JackPortIsInput, 0));
        g_output_ports.push_back(jack_port_register(g_jack_client, output_port_names[i], JACK_DEFAULT_AUDIO_TYPE, JackPortIsOutput, 0));
        if(ports_midi_enabled[i]) {
            g_maybe_midi_input_ports.push_back(jack_port_register(g_jack_client, midi_input_port_names[midi_idx], JACK_DEFAULT_MIDI_TYPE, JackPortIsInput, 0));
            g_maybe_midi_output_ports.push_back(jack_port_register(g_jack_client, midi_output_port_names[midi_idx], JACK_DEFAULT_MIDI_TYPE, JackPortIsOutput, 0));
            midi_idx++;
        } else {
            g_maybe_midi_input_ports.push_back(nullptr);
            g_maybe_midi_output_ports.push_back(nullptr);
        }
        g_midi_input_state_trackers.push_back(MIDIStateTracker());
        g_midi_input_notes_state_around_record.push_back(MIDINotesStateAroundLoop());
        g_input_port_latencies.push_back(0);
        g_output_port_latencies.push_back(0);
        g_port_recording_latencies(i) = 0;
        g_ports_to_mixed_outputs(i) = ports_to_mixed_outputs_mapping[i];
        g_temporary_merging_buffers.push_back(MIDIRingBuffer(g_midi_max_merge_size));
        if(g_input_ports[i] == nullptr) {
            std::cerr << "Backend: Got null port" << std::endl;
            return nullptr;
        }
        if(g_output_ports[i] == nullptr) {
            std::cerr << "Backend: Got null port" << std::endl;
            return nullptr;
        }

        g_passthroughs(i) = 1.0f;
        g_port_volumes(i) = 1.0f;
        g_ports_muted(i) = 0;
        g_port_inputs_muted(i) = 0;
        g_n_port_events(i) = 0;

        g_atomic_state.port_n_input_events_since_last_update[i] = 0;
        g_atomic_state.port_n_output_events_since_last_update[i] = 0;
    }

    // Set the JACK process and latency callback
    jack_set_process_callback(g_jack_client, jack_process, nullptr);
    jack_set_latency_callback(g_jack_client, jack_latency_callback, nullptr);

    if(jack_activate(g_jack_client)) {
        std::cerr << "Backend: Failed to activate client" << std::endl;
        return nullptr;
    }

    g_update_cb = update_cb;

    if(features == Profiling) {
        g_reporting_thread = std::thread([]() {
            while(true) {
                std::unique_lock<std::mutex> lock(g_terminate_cv_m);
                if(g_terminate_cv.wait_for(lock, std::chrono::milliseconds(500)) == std::cv_status::no_timeout) {
                    // Terminated.
                    std::cout << "Reporting thread terminating." << std::endl;
                    return nullptr;
                }

                size_t popped = 0;
                benchmark_info info;
                while(g_benchmark_queue.pop(info)) { popped++; };
                if (popped > 0) {
                    int period = (int)(1.0f / (float)g_sample_rate * (float)g_buf_nframes * 1000000.0f);
                    float percent_of_period = (float) info.total_us / (float) period * 100.0f;
                    std::cout
                        << "Backend: total, slowmid, getbuf, cmd, in, proc, out (us)" << std::endl
                        << "         "
                        << info.total_us << " "
                        << info.process_slow_midi_us << " "
                        << info.request_buffers_us << " "
                        << info.command_processing_us << " "
                        << info.input_copy_us << " "
                        << info.processing_us << " "
                        << info.output_copy_us << std::endl
                        << "         period: " << g_buf_nframes << " @ " << g_sample_rate << " -> " << period << " us"
                        << " (using " << percent_of_period << "%)\n";
                }
            }
        });
    }

    return g_jack_client;
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

    auto check_args = [n_args](int n_needed) {
        if (n_args != n_needed) {
            throw std::runtime_error("do_loop_action: incorrect # of args");
        }
    };

    auto apply_state = [with_soft_sync](std::vector<unsigned> loops, loop_mode_t mode, int delay=0, bool is_next_next_mode=false) {
        for(auto const& loop: loops) {
            int x_idx = is_next_next_mode ? 1 : 0;
            g_next_modes[0](x_idx, loop) = g_next_modes[1](x_idx, loop) = mode;
            g_next_modes_countdown[0](x_idx, loop) = g_next_modes_countdown[1](x_idx, loop) = delay;
            if(!with_soft_sync) {
                g_states[0](loop) = g_states[1](loop) = mode;
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
            cmd = [idxs, apply_state, arg1_i]() {
                apply_state(idxs, Recording, arg1_i);
                for(auto const& idx: idxs) {
                    g_positions[0](idx) = g_positions[1](idx) = 0;
                    g_lengths[0](idx) = g_lengths[1](idx) = 0;
                }
            };
            break;
        case DoRecordNCycles:
            check_args(3);
            cmd = [idxs, apply_state, arg1_i, arg2_i, arg3_i]() {
                apply_state(idxs, Recording, arg1_i);
                apply_state(idxs, (loop_mode_t) arg3_i, arg2_i-1, true);
            };
            break;
        case DoPlay:
            check_args(1);
            cmd = [idxs, apply_state, arg1_i]() {
                apply_state(idxs, Playing, arg1_i);
            };
            break;
        case DoPlayMuted:
            check_args(1);
            cmd = [idxs, apply_state, arg1_i]() {
                apply_state(idxs, PlayingMuted, arg1_i);
            };
            break;
        case DoStop:
            check_args(1);
            cmd = [idxs, apply_state, arg1_i]() {
                apply_state(idxs, Stopped, arg1_i);
            };
            break;
        case DoClear:
            check_args(0);
            cmd = [idxs, apply_state]() {
                apply_state(idxs, Stopped);
                for(auto const& idx: idxs) {
                    g_positions[0](idx) = g_positions[1](idx) = 0;
                    g_lengths[0](idx) = g_lengths[1](idx) = 0;
                }
            };
            break;
        case SetLoopVolume:
            check_args(1);
            cmd = [idxs, arg1_f]() {
                for (auto const& idx: idxs) {
                    g_loop_volumes(idx) = arg1_f;
                }
            };
            break;
        default:
        break;
    }

    if(cmd) { push_command(cmd); }
    return 0;
}

int do_port_action(
    unsigned port_idx,
    port_action_t action,
    float maybe_arg
) {
    std::function<void()> cmd = nullptr;

    switch(action) {
        case DoMute:
            cmd = [port_idx]() {
                g_ports_muted(port_idx) = 1;
            };
            break;
        case DoUnmute:
            cmd = [port_idx]() {
                g_ports_muted(port_idx) = 0;
            };
            break;
        case DoMuteInput:
            cmd = [port_idx]() {
                g_port_inputs_muted(port_idx) = 1;
            };
            break;
        case DoUnmuteInput:
            cmd = [port_idx]() {
                g_port_inputs_muted(port_idx) = 0;
            };
            break;
        case SetPortPassthrough:
            cmd = [port_idx, maybe_arg]() {
                g_passthroughs(port_idx) = maybe_arg;
            };
            break;
        case SetPortVolume:
            cmd = [port_idx, maybe_arg]() {
                g_port_volumes(port_idx) = maybe_arg;
            };
            break;
        default:
        break;
    }

    if(cmd) { push_command(cmd); }
    return 0;
}

void request_update() {
    // Allocate memory for a copy of the relevant mode
    std::vector<loop_mode_t> states(g_n_loops), next_modes(g_n_loops);
    std::vector<int32_t> positions(g_n_loops), lengths(g_n_loops), latencies(g_n_ports), next_mode_countdowns(g_n_loops);
    std::vector<float> passthroughs(g_n_ports), loop_volumes(g_n_loops), port_volumes(g_n_ports);
    std::vector<int8_t> ports_muted(g_n_ports), port_inputs_muted(g_n_ports);
    std::vector<unsigned> loop_n_output_events(g_n_loops), port_n_input_events(g_n_ports), port_n_output_events(g_n_ports);

    // Copy the mode
    for(int i=0; i<g_n_loops; i++) {
        states[i] = g_atomic_state.states[i];
        // TODO
        next_modes[i] = g_atomic_state.next_modes[i][0];
        next_mode_countdowns[i] = g_atomic_state.next_modes_countdown[i][0];
        positions[i] = g_atomic_state.positions[i];
        lengths[i] = g_atomic_state.lengths[i];
        loop_volumes[i] = g_atomic_state.loop_volumes[i];
        loop_n_output_events[i] = g_atomic_state.loop_n_output_events_since_last_update[i];
        // Reset counters
        g_atomic_state.loop_n_output_events_since_last_update[i] = 0;
    }
    for(int i=0; i<g_n_ports; i++) {
        passthroughs[i] = g_atomic_state.passthroughs[i];
        port_volumes[i] = g_atomic_state.port_volumes[i];
        ports_muted[i] = g_atomic_state.ports_muted[i];
        port_inputs_muted[i] = g_atomic_state.port_inputs_muted[i];
        latencies[i] = g_atomic_state.latencies[i];
        port_n_input_events[i] = g_atomic_state.port_n_input_events_since_last_update[i];
        port_n_output_events[i] = g_atomic_state.port_n_output_events_since_last_update[i];
        // Reset counters
        g_atomic_state.port_n_input_events_since_last_update[i] = 0;
        g_atomic_state.port_n_output_events_since_last_update[i] = 0;
    }
    
    if (g_update_cb) {
        int r = g_update_cb(
            g_n_loops,
            g_n_ports,
            g_sample_rate,
            states.data(),
            next_modes.data(),
            next_mode_countdowns.data(),
            lengths.data(),
            positions.data(),
            loop_volumes.data(),
            port_volumes.data(),
            passthroughs.data(),
            latencies.data(),
            ports_muted.data(),
            port_inputs_muted.data(),
            g_loop_output_peaks.data(),
            g_port_output_peaks.data(),
            g_port_input_peaks.data(),
            loop_n_output_events.data(),
            port_n_input_events.data(),
            port_n_output_events.data()
        );
    }
}

int load_loop_data(
    unsigned loop_idx,
    unsigned len,
    float *data
) {
    std::atomic<bool> finished = false;
    auto my_length = len;
    push_command([loop_idx, &finished]() {
        g_next_modes_countdown[0](0, loop_idx) = g_next_modes_countdown[0](1, loop_idx) =
            g_next_modes_countdown[1](0, loop_idx) = g_next_modes_countdown[1](1, loop_idx) = -1;
        g_states[0](loop_idx) = g_states[1](loop_idx) = Stopped;
        g_positions[0](loop_idx) = g_positions[1](loop_idx) = 0;
        finished = true;
    });

    while(!finished && g_loops_fn) {}

    auto soft_synced_to = g_loops_soft_sync_mapping(loop_idx);

    for(size_t idx = 0; idx < len; idx++) {
        g_storage(idx, loop_idx) = data[idx];
    }
    if(soft_synced_to >= 0 && soft_synced_to != loop_idx) {
        auto soft_synced_length = g_lengths[g_last_written_output_buffer_tick_tock](soft_synced_to);
        for(size_t idx = len; idx < len; idx++) {
            // Hold last value
            g_storage(idx, loop_idx) = g_storage(len-1, loop_idx);
        }
    }

    finished = false;
    push_command([loop_idx, my_length, &finished]() {
        g_lengths[0](loop_idx) = g_lengths[1](loop_idx) = my_length;
        finished = true;
    });
    while(!finished && g_loops_fn) {}

    return 0;
}

unsigned get_loop_data(
    unsigned loop_idx,
    float ** data_out,
    unsigned do_stop
) {
    auto prev_storage_lock = g_storage_lock;
    std::atomic<bool> finished = false;
    push_command([loop_idx, &finished, do_stop]() {
        if (do_stop) {
            g_next_modes_countdown[0](0, loop_idx) = g_next_modes_countdown[0](1, loop_idx) =
                g_next_modes_countdown[1](0, loop_idx) = g_next_modes_countdown[1](1, loop_idx) = -1;
            g_states[0](loop_idx) = g_states[1](loop_idx) = Stopped;
            g_positions[0](loop_idx) = g_positions[1](loop_idx) = 0;
        }
        // TODO: check that we are not recording, this may be messed up by the storage lock
        g_storage_lock = 1;
        finished = true;
    });
    while(!finished && g_loops_fn) {}

    auto len = g_lengths[g_last_written_output_buffer_tick_tock](loop_idx);
    auto data = (float*) malloc(sizeof(float) * len);
    for(size_t idx = 0; idx < len; idx++) {
        data[idx] = g_storage(idx, loop_idx);
    }
    *data_out = data;

    g_storage_lock = prev_storage_lock;

    return len;
}

void set_storage_lock(unsigned int value) {
    g_storage_lock = (int8_t) value;
}

void terminate() {
    jack_client_close(g_jack_client);
    g_terminate_cv.notify_all();
    if (g_reporting_thread.joinable()) {
        g_reporting_thread.join();
    }
    g_cmd_queue.reset();

    // TODO: explicit profiler print, can be removed if we solve segfault
    static void *const user_context = nullptr;
    halide_profiler_report(user_context);
}

jack_port_t* get_port_output_handle(unsigned port_idx) {
    return g_output_ports[port_idx];
}

jack_port_t* get_port_input_handle(unsigned port_idx) {
    return g_input_ports[port_idx];
}

void remap_port_input(unsigned port, unsigned input_source) {
    g_port_input_mappings(port) = input_source;
}

void reset_port_input_remapping(unsigned port) {
    g_port_input_mappings(port) = port;
}

jack_port_t *create_slow_midi_port(
    const char* name,
    slow_midi_port_kind_t kind
) {
    auto jport = jack_port_register(g_jack_client, name, JACK_DEFAULT_MIDI_TYPE,
                                    kind == Input ? JackPortIsInput : JackPortIsOutput, 0);
    if (jport) {
        _slow_midi_port port {
            .jack_port = jport,
            .name = std::string(name),
            .kind = kind,
            .queue = std::vector<std::vector<uint8_t>>()
        };
        port.queue.reserve(slow_midi_port_queue_starting_size);
        g_slow_midi_ports.push_back(port);
        return jport;
    }

    std::cout << "Backend: failed to create slow MIDI port." << std::endl;
    return nullptr;
}

void set_slow_midi_port_received_callback(
    jack_port_t *port,
    SlowMIDIReceivedCallback callback
) {
    for(auto &it : g_slow_midi_ports) {
        if(it.jack_port == port) {
            it.maybe_rcv_callback = callback;
        }
    }
}

void destroy_slow_midi_port(jack_port_t *port) {
    if (port) {
        jack_port_unregister(g_jack_client, port);
    }

    for(auto it = g_slow_midi_ports.begin(); it != g_slow_midi_ports.end(); it++) {
        if(it->jack_port == port) {
            g_slow_midi_ports.erase(it);
            break;
        }
    }
}

void send_slow_midi(
    jack_port_t *port,
    uint8_t len,
    uint8_t *data
) {
    for(auto &it : g_slow_midi_ports) {
        if(it.jack_port == port) {
            std::vector<uint8_t> msg(len);
            memcpy((void*)msg.data(), (void*)data, len);
            it.queue.push_back(msg);
            break;
        }
    }
}

void process_slow_midi() {
    for (auto &it : g_slow_midi_ports) {
        if(it.kind == Input && it.queue.size() > 0) {
            if(it.maybe_rcv_callback) {
                for (auto &elem : it.queue) {
                    if(it.maybe_rcv_callback) {
                        it.maybe_rcv_callback(it.jack_port, elem.size(), elem.data());
                    }
                }
            }
            it.queue.clear();
        }
    }
}

void shoopdaloop_free(void* ptr) {
    free (ptr);
}

unsigned get_sample_rate() {
    return g_sample_rate;
}

unsigned get_loop_data_rms(
    unsigned loop_idx,
    unsigned from_sample,
    unsigned to_sample,
    unsigned samples_per_bin,
    float **data_out
) {
    unsigned n_bins = std::ceil((float)std::max<int>(0, to_sample - from_sample) / (float) samples_per_bin);
    float *bins = (float*)malloc(sizeof(float) * n_bins);

    auto prev_storage_lock = g_storage_lock;
    std::atomic<bool> finished = false;
    push_command([loop_idx, &finished]() {
        // TODO: check that we are not recording, this may be messed up by the storage lock
        g_storage_lock = 1;
        finished = true;
    });
    while(!finished && g_loops_fn) {}

    for(size_t idx = 0; idx < n_bins; idx++) {
        bins[idx] = 0.0f;
        unsigned bin_start = from_sample + idx*samples_per_bin;
        for(size_t sample = bin_start; sample < bin_start + samples_per_bin && sample < g_storage.dim(0).extent(); sample++) {
            auto v = g_storage(from_sample + sample, loop_idx);
            bins[idx] += v * v;
        }
    }
    g_storage_lock = prev_storage_lock;

    for(size_t idx = 0; idx < n_bins; idx++) {
        bins[idx] = std::sqrt(bins[idx] / (float) samples_per_bin);
    }

    *data_out = bins;
    return n_bins;
}

unsigned get_loop_midi_data(
    unsigned loop_idx,
    unsigned char **data_out
) {
    unsigned len;
    *data_out = g_loop_midi_buffers[loop_idx].copy_bytes(&len);
    return len;
}

unsigned set_loop_midi_data(
    unsigned loop_idx,
    unsigned char *data,
    unsigned data_len
) {
    MIDIRingBuffer temp(g_midi_ring_buf_size);
    temp.adopt_bytes(data, data_len);

    std::atomic<bool> finished = false;
    push_command([loop_idx, &finished, &temp]() {
        g_loop_midi_buffers[loop_idx] = temp;
        g_midi_input_notes_state_around_record[loop_idx].reset();
        finished = true;
    });
    while(!finished && g_loops_fn) {}
    
    return 0;
}

void freeze_midi_buffer (unsigned loop_idx) {
    MIDIRingBuffer temp = g_midi_input_notes_state_around_record[loop_idx].freeze_buffer(g_loop_midi_buffers[loop_idx]);

    std::atomic<bool> finished = false;
    push_command([loop_idx, &finished, &temp]() {
        g_loop_midi_buffers[loop_idx] = temp;
        g_midi_input_notes_state_around_record[loop_idx].reset();
        finished = true;
    });
    while(!finished && g_loops_fn) {}
}

void set_loops_length(unsigned *loop_idxs,
                      unsigned n_loop_idxs,
                      unsigned length) {
    std::atomic<bool> finished = false;
    push_command([loop_idxs, length, n_loop_idxs, &finished]() {
        // TODO: check that we are not recording, this may be messed up by the storage lock
        for(size_t idx=0; idx<n_loop_idxs; idx++) {
            g_lengths[0](loop_idxs[idx]) = g_lengths[1](loop_idxs[idx]) = length;
        }
        finished = true;
    });
    while(!finished && g_loops_fn) {}
}

} //extern "C"