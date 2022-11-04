#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    Stopped,
    Playing,
    Recording,
    LOOP_STATE_MAX
} loop_state_t;

typedef enum  {
    DoRecord,
    DoRecordNCycles,
    DoPlay,
    DoStop,
    DoClear,
    LOOP_ACTION_MAX
} loop_action_t;

// The update callback provides the current looper state.
// The callee should not free any pointers.
typedef int(*UpdateCallback) (
    unsigned n_loops,
    unsigned n_ports,
    loop_state_t *loop_states,
    loop_state_t *loop_next_states,
    int *loop_lengths,
    int *loop_positions,
    float *loop_volumes,
    float *port_volumes,
    float *port_passthrough_levels
);

typedef void (*AbortCallback) ();

// Initialize the back-end.
// n_loops: Amount of loops to instantiate. Note that each loop is a mono channel only.
// n_ports: Amount of JACK ports to instantiate.
// loop_len_seconds: Maximum amount of seconds a loop can store.
// loops_to_ports_mapping: Should contain n_loops indices, which indicate to which port the loop should be connected.
//                         Multiple loops can be connected to the same port.
//                         In that case, they receive the same input and their output is mixed.
// loops_hard_sync_mapping: Should contain n_loops indices, which indicate to which other loop the loop should be
//                          "hard-synced". A hard-synced loop will always mirror its master's state, length and position.
//                          Typical use-case is for making a multi-channel loop, using a looper for each channel.
//                          Using a negative number or the loop's own index disables hard sync for that loop.
// loops_soft_sync_mapping: Should contain n_loops indices, which indicate to which other loop the loop sholud be
//                          "soft-synced". A soft-synced loop will only change state when its master (re-)starts playing.
//                          Using a negative number or the loop's own index disables soft sync for that loop.
// input_port_names: n_ports strings which give the names of the inputs for the ports.
// output_port_names: n_ports strings which give the names of the outputs for the ports.
// client_name: Name of the JACK client to register
// update_cb: this callback will be called when a state update is requested.
// abort_cb: this callback will be called if the back-end aborts operation for any reason.
int initialize(
    unsigned n_loops,
    unsigned n_ports,
    float loop_len_seconds,
    unsigned *loops_to_ports_mapping,
    int *loops_hard_sync_mapping,
    int *loops_soft_sync_mapping,
    const char **input_port_names,
    const char **output_port_names,
    const char *client_name,
    unsigned print_benchmark_info,
    UpdateCallback update_cb,
    AbortCallback abort_cb
);


// Perform an action on the given loop.
int do_loop_action(
    unsigned loop_idx,
    loop_action_t action
);

// Request an update. The update callback will be immediately called.
void request_update();

#ifdef __cplusplus
}
#endif