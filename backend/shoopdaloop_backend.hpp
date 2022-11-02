extern "C" {

enum loop_state_t { Paused, Playing, Recording };
enum loop_action_t { DoRecord,
                     DoPlay,
                     DoPause };

typedef int(*UpdateCallback) (
    unsigned n_loops,
    unsigned n_ports,
    loop_state_t *loop_states,
    int *loop_lengths,
    int *loop_positions,
    float *loop_volumes,
    float *port_volumes,
    float *port_passthrough_levels
);
typedef void (*AbortCallback) ();

int initialize(
    unsigned n_loops,
    unsigned n_ports,
    unsigned loop_len_samples,
    unsigned loop_channels,
    unsigned *loops_to_ports_mapping,
    const char **input_port_names,
    const char **output_port_names,
    const char *client_name,
    unsigned update_period_ms,
    UpdateCallback update_cb,
    AbortCallback abort_cb
);

int do_loop_action(
    unsigned loop_idx,
    loop_action_t action
);

}
