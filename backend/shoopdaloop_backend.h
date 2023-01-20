#include <jack/types.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    Stopped,
    Playing,
    PlayingMuted, // Useful for generating sync while not outputting any sound
    Recording,
    LOOP_STATE_MAX
} loop_state_t;

typedef enum  {
    DoRecord,           // Arg 1 is # of cycles to delay before starting.
    DoRecordNCycles,    // Arg 1 is # of cycles to delay before starting. Arg 2 # of cycles to record. Arg 3 is the next state after finishing.
    DoPlay,             // Arg 1 is # of cycles to delay before starting.
    DoPlayMuted,        // Arg 1 is # of cycles to delay before starting.
    DoStop,             // Arg 1 is # of cycles to delay before starting.
    DoClear,
    SetLoopVolume,      // Arg 1 is the new volume
    LOOP_ACTION_MAX
} loop_action_t;

typedef enum {
    DoMute,
    DoUnmute,
    DoMuteInput,
    DoUnmuteInput,
    SetPortVolume,
    SetPortPassthrough,
    PORT_ACTION_MAX
} port_action_t;

typedef enum {
    Default,
    Profiling,
    Tracing
} backend_features_t;

// The update callback provides the current looper state.
// The callee should not free any pointers.
typedef int(*UpdateCallback) (
    unsigned n_loops,
    unsigned n_ports,
    unsigned sample_rate,
    loop_state_t *loop_states,
    loop_state_t *loop_next_states,
    int32_t *loop_next_state_countdowns,
    int *loop_lengths,
    int *loop_positions,
    float *loop_volumes,
    float *port_volumes,
    float *port_passthrough_levels,
    int *port_latencies,
    int8_t *ports_muted,
    int8_t *port_inputs_muted,
    float *loop_output_peaks,
    float *port_output_peaks,
    float *port_input_peaks,
    // MIDI:
    unsigned *loop_n_output_events,
    unsigned *port_n_input_events,
    unsigned *port_n_output_events
);

typedef void (*AbortCallback) ();

// Initialize the back-end.
// n_loops: Amount of loops to instantiate. Note that each loop is a mono channel only.
// n_ports: Amount of ports to instantiate. A "port" here actually means a pair of
//          associated input and output ports.
// n_mixed_output_ports: Amount of additional ports to instantiate which will output mixed samples of the other
//                       ports.
// loop_len_seconds: Maximum amount of seconds a loop can store.
// loops_to_ports_mapping: Should contain n_loops indices, which indicate to which port the loop should be connected.
//                         Multiple loops can be connected to the same port.
//                         In that case, they receive the same input and their output is mixed.
// loops_hard_sync_mapping: Should contain n_loops indices, which indicate to which other loop the loop should be
//                          "hard-synced". A hard-synced loop will always mirror its master's state, length and position.
//                          Typical use-case is for making a multi-channel loop, using a looper for each channel.
//                          Using a negative number or the loop's own index disables hard sync for that loop.
// loops_soft_sync_mapping: Should contain n_loops indices, which indicate to which other loop the loop should be
//                          "soft-synced". A soft-synced loop will only change state when its master (re-)starts playing.
//                          Using a negative number or the loop's own index disables soft sync for that loop.
// ports_to_mixed_outputs_mapping: These indices indicate to which mixed output port each regular port's samples will
//                                 be mixed. <0 is no target.
// ports_midi_enabled_list: List of port indexes for the ports which should receive a pair of MIDI ports in addition
//                          to audio.
// input_port_names: n_ports strings which give the names of the inputs for the ports.
// output_port_names: n_ports strings which give the names of the outputs for the ports.
// mixed_output_port_names: n_mixed_ports strings which give the names of the mixed output ports.
// client_name: Name of the JACK client to register
// update_cb: this callback will be called when a state update is requested.
// abort_cb: this callback will be called if the back-end aborts operation for any reason.
jack_client_t* initialize(
    unsigned n_loops,
    unsigned n_ports,
    unsigned n_mixed_output_ports,
    float loop_len_seconds,
    unsigned *loops_to_ports_mapping,
    int *loops_hard_sync_mapping,
    int *loops_soft_sync_mapping,
    int *ports_to_mixed_outputs_mapping,
    int *ports_midi_enabled_list,
    const char **input_port_names,
    const char **output_port_names,
    const char **mixed_output_port_names,
    const char *client_name,
    unsigned latency_buf_size,
    UpdateCallback update_cb,
    AbortCallback abort_cb,
    backend_features_t features
);


// Perform an action on the given loop(s).
// Passing multiple loops guarantees that state change on them
// happens simultaneously.
// Some actions have an argument (see the loop_action_t definition)
int do_loop_action(
    unsigned *loop_idxs,
    unsigned n_loop_idxs,
    loop_action_t action,
    float *maybe_args,
    unsigned n_args,
    unsigned with_soft_sync
);

// Perform an action on the given port.
int do_port_action(
    unsigned port_idx,
    port_action_t action,
    float maybe_arg
);

// Request an update. The update callback will be immediately called.
void request_update();

// Load data into loop storage.
// Will set the loop state to Stopped and ignore any data longer
// than the maximum loop storage.
int load_loop_data(
    unsigned loop_idx,
    unsigned len,
    float *data
);

// Get raw loop data from storage.
// Returns the amount of values in the data.
// Caller is responsible for freeing the returned pointer.
// A storage lock is enabled to make sure that the data is
// valid. However, strange behavior may occur when recording
// while the storage lock is set.
// If do_stop is nonzero, the loop is first stopped. That
// way we can be sure that data and behavior are both valid.
unsigned get_loop_data(
    unsigned loop_idx,
    float **data_out,
    unsigned do_stop
);

// Get RMS power data of a loop's contents divided into bins.
// A custom sample range can be specified, as well as how
// many samples should be reduced per bin.
// Together, these determine the amount of bins returned.
// Caller is responsible for freeing the returned pointer.
unsigned get_loop_data_rms(
    unsigned loop_idx,
    unsigned from_sample,
    unsigned to_sample,
    unsigned samples_per_bin,
    float **data_out
);

// Get a copy of a loop's MIDI data.
// The binary format of the MIDI buffer is a contiguous sequence of events
// each constructed as follows:
// - uint32_t time
// - size_t   size
// - followed by [size] data bytes.
// Caller is responsible for freeing the returned pointer.
// returns length.
unsigned get_loop_midi_data(
    unsigned loop_idx,
    unsigned char **data_out
);

// Overwrite a loop's MIDI data and update its length.
// For the binary format, see get_loop_midi_data.
// The length of the loop should be set separately.
unsigned set_loop_midi_data(
    unsigned loop_idx,
    unsigned char *data,
    unsigned data_len
);

// TODO: extend for arbitrary sample range
void set_loops_length(unsigned *loop_idxs,
                      unsigned n_loop_idxs,
                      unsigned length);

// If nonzero, will emit a noteOn at loop start for every noteOff within
// the loop which did not have a noteOn. The velocity is remembered
// from when the note was actually played before recording.
void set_loop_midi_auto_noteon(unsigned loop_idx, unsigned enabled);

// Similar to auto noteon, this will automatically send a noteoff for any
// note not terminated at loop end.
void set_loop_midi_auto_noteoff(unsigned loop_idx, unsigned enabled);

// Any MIDI notes completely played within the pre-record grace period
// just before recording started, will be played back at loop start on
// every cycle. This way we can catch e.g. drum hits played slightly too
// early.
void set_loop_midi_prerecord_grace_period(unsigned loop_idx, float seconds);

// Set the global storage lock.
// Any nonzero value is "locked".
// Use with care: when set the storage lock will prevent
// loop content and lengths from changing, even if recording.
void set_storage_lock(unsigned value);

// Get access to the JACK port structures of a port pair.
jack_port_t* get_port_output_handle(unsigned port_idx);
jack_port_t* get_port_input_handle(unsigned port_idx);

// Override a port input connection. By default, each "port"
// refers to a pair of input and output that are associated with
// each other.
// If a port input is overridden, its input samples are taken from
// another port pair's JACK input instead, which affects e.g.
// loop recording and port passthrough.
void remap_port_input(unsigned port, unsigned input_source);

// Reset a port's input override. It will take samples from its own
// JACK input again.
void reset_port_input_remapping(unsigned port);

// Terminate the back-end.
void terminate();

// Functions to create and manipulate
// "slow" MIDI ports (e.g. for control purposes). Message handling
// on such ports will be decoupled from the JACK processing thread via
// queues.
typedef enum { Input, Output } slow_midi_port_kind_t;
typedef void(*SlowMIDIReceivedCallback) (
    jack_port_t *port,
    uint8_t len,
    uint8_t *data
);
jack_port_t *create_slow_midi_port(
    const char* name,
    slow_midi_port_kind_t kind
);
void set_slow_midi_port_received_callback(
    jack_port_t *port,
    SlowMIDIReceivedCallback callback
);
void destroy_slow_midi_port(jack_port_t *port);
void send_slow_midi(
    jack_port_t *port,
    uint8_t len,
    uint8_t *data
);
// Will call the received callback for any received messages.
void process_slow_midi();

void shoopdaloop_free(void* ptr);

unsigned get_sample_rate();

#ifdef __cplusplus
}
#endif