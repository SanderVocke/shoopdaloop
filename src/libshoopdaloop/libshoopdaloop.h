#include "types.h"

#ifdef _WIN32
    #define SHOOP_EXPORT __declspec(dllexport)
#else
    #define SHOOP_EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

// Audio driver
SHOOP_EXPORT shoop_audio_driver_t *create_audio_driver (shoop_audio_driver_type_t type);
SHOOP_EXPORT shoop_audio_driver_state_t *get_audio_driver_state (shoop_audio_driver_t *driver);
SHOOP_EXPORT unsigned driver_type_supported(shoop_audio_driver_type_t driver_type);
SHOOP_EXPORT void destroy_audio_driver (shoop_audio_driver_t *driver);
SHOOP_EXPORT void *maybe_driver_handle (shoop_audio_driver_t *driver);
SHOOP_EXPORT const char* maybe_driver_instance_name (shoop_audio_driver_t *driver);
SHOOP_EXPORT unsigned get_sample_rate (shoop_audio_driver_t *driver);
SHOOP_EXPORT unsigned get_buffer_size (shoop_audio_driver_t *driver);
SHOOP_EXPORT unsigned get_driver_active (shoop_audio_driver_t *driver);
SHOOP_EXPORT void start_dummy_driver(shoop_audio_driver_t *driver, shoop_dummy_audio_driver_settings_t settings);
SHOOP_EXPORT void start_jack_driver(shoop_audio_driver_t *driver, shoop_jack_audio_driver_settings_t settings);
SHOOP_EXPORT void wait_process(shoop_audio_driver_t *driver);
SHOOP_EXPORT shoop_external_port_descriptors_t *find_external_ports(shoop_audio_driver_t *driver, const char* maybe_name_regex, shoop_port_direction_t maybe_port_direction_filter, shoop_port_data_type_t maybe_data_type_filter);

// Test functions
SHOOP_EXPORT void do_segfault_on_process_thread(shoop_backend_session_t *backend);
SHOOP_EXPORT void do_abort_on_process_thread(shoop_backend_session_t *backend);

// Session
SHOOP_EXPORT shoop_backend_session_t *create_backend_session ();
SHOOP_EXPORT void destroy_backend_session (shoop_backend_session_t *session);
SHOOP_EXPORT shoop_backend_session_state_info_t *get_backend_session_state(shoop_backend_session_t *session);
SHOOP_EXPORT shoop_profiling_report_t* get_profiling_report(shoop_backend_session_t *session);
SHOOP_EXPORT shoop_result_t set_audio_driver(shoop_backend_session_t *session, shoop_audio_driver_t *driver);

// Loops
SHOOP_EXPORT shoopdaloop_loop_t *create_loop(shoop_backend_session_t *backend);
SHOOP_EXPORT shoopdaloop_loop_audio_channel_t *add_audio_channel (shoopdaloop_loop_t *loop, shoop_channel_mode_t mode);
SHOOP_EXPORT shoopdaloop_loop_midi_channel_t  *add_midi_channel  (shoopdaloop_loop_t *loop, shoop_channel_mode_t mode);
SHOOP_EXPORT unsigned          get_n_audio_channels     (shoopdaloop_loop_t *loop);
SHOOP_EXPORT unsigned          get_n_midi_channels      (shoopdaloop_loop_t *loop);
SHOOP_EXPORT void              clear_loop               (shoopdaloop_loop_t *loop, unsigned length);
SHOOP_EXPORT shoop_loop_state_info_t *get_loop_state          (shoopdaloop_loop_t *loop);
SHOOP_EXPORT void              set_loop_length          (shoopdaloop_loop_t *loop, unsigned length);
SHOOP_EXPORT void              set_loop_position        (shoopdaloop_loop_t *loop, unsigned position);
SHOOP_EXPORT void              set_loop_sync_source     (shoopdaloop_loop_t *loop, shoopdaloop_loop_t *sync_source);
SHOOP_EXPORT void              adopt_ringbuffer_contents(shoopdaloop_loop_t *loop, int reverse_cycles_start, int cycles_length, int go_to_cycle, shoop_loop_mode_t go_to_mode);
// Transition loop(s).
// maybe_delay is given in # of triggers to wait before transitioning.
//   when -1 is given, the delay is instant and does not take into account the sync source.
// maybe_to_sync_at_cycle is ignored if it is negative. If it is >= 0, the transition will
//   be instantaneous, and the position will be set to the Nth loop cycle plus the current
//   sync source position. In other words: the loop will be in sync with the sync source.
SHOOP_EXPORT void loop_transition(shoopdaloop_loop_t *loop,
                      shoop_loop_mode_t mode,
                      int maybe_delay,
                      int maybe_to_sync_at_cycle);
SHOOP_EXPORT void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop_t **loops,
                      shoop_loop_mode_t mode,
                      int maybe_delay,
                      int maybe_to_sync_at_cycle);

// Loop channels
SHOOP_EXPORT void                   clear_audio_channel      (shoopdaloop_loop_audio_channel_t *channel, unsigned length);
SHOOP_EXPORT void                   clear_midi_channel       (shoopdaloop_loop_midi_channel_t  *channel);
SHOOP_EXPORT void                   connect_audio_output     (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port);
SHOOP_EXPORT void                   connect_midi_output      (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t *port);
SHOOP_EXPORT void                   connect_audio_input      (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port);
SHOOP_EXPORT void                   connect_midi_input       (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t *port);
SHOOP_EXPORT void                   disconnect_audio_outputs (shoopdaloop_loop_audio_channel_t *channel);
SHOOP_EXPORT void                   disconnect_midi_outputs  (shoopdaloop_loop_midi_channel_t  *channel);
SHOOP_EXPORT void                   disconnect_audio_output  (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port);
SHOOP_EXPORT void                   disconnect_midi_output   (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port);
SHOOP_EXPORT void                   disconnect_audio_inputs (shoopdaloop_loop_audio_channel_t *channel);
SHOOP_EXPORT void                   disconnect_midi_inputs  (shoopdaloop_loop_midi_channel_t  *channel);
SHOOP_EXPORT void                   disconnect_audio_input  (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port);
SHOOP_EXPORT void                   disconnect_midi_input   (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port);
SHOOP_EXPORT shoop_audio_channel_data_t  *get_audio_channel_data   (shoopdaloop_loop_audio_channel_t *channel);
SHOOP_EXPORT shoop_midi_sequence_t   *get_midi_channel_data    (shoopdaloop_loop_midi_channel_t  *channel);
SHOOP_EXPORT void                   load_audio_channel_data  (shoopdaloop_loop_audio_channel_t *channel, shoop_audio_channel_data_t *data);
SHOOP_EXPORT void                   load_midi_channel_data   (shoopdaloop_loop_midi_channel_t  *channel, shoop_midi_sequence_t  *data);
SHOOP_EXPORT shoop_audio_channel_state_info_t *get_audio_channel_state  (shoopdaloop_loop_audio_channel_t *channel);
SHOOP_EXPORT void                   set_audio_channel_gain (shoopdaloop_loop_audio_channel_t *channel, float gain);
SHOOP_EXPORT shoop_midi_channel_state_info_t  *get_midi_channel_state   (shoopdaloop_loop_midi_channel_t  *channel);
SHOOP_EXPORT void                   set_audio_channel_mode   (shoopdaloop_loop_audio_channel_t * channel, shoop_channel_mode_t mode);
SHOOP_EXPORT void                   set_midi_channel_mode    (shoopdaloop_loop_midi_channel_t * channel, shoop_channel_mode_t mode);
SHOOP_EXPORT void                   set_audio_channel_start_offset (shoopdaloop_loop_audio_channel_t *channel, int offset);
SHOOP_EXPORT void                   set_midi_channel_start_offset  (shoopdaloop_loop_midi_channel_t *channel, int offset);
SHOOP_EXPORT void                   set_audio_channel_n_preplay_samples (shoopdaloop_loop_audio_channel_t *channel, unsigned n);
SHOOP_EXPORT void                   set_midi_channel_n_preplay_samples  (shoopdaloop_loop_midi_channel_t *channel, unsigned n);
SHOOP_EXPORT void                   clear_audio_channel_data_dirty (shoopdaloop_loop_audio_channel_t * channel);
SHOOP_EXPORT void                   clear_midi_channel_data_dirty (shoopdaloop_loop_midi_channel_t * channel);

// FX chains
SHOOP_EXPORT shoopdaloop_fx_chain_t *create_fx_chain(shoop_backend_session_t *backend, shoop_fx_chain_type_t type, const char* title);
SHOOP_EXPORT void fx_chain_set_ui_visible(shoopdaloop_fx_chain_t *chain, unsigned visible);
SHOOP_EXPORT shoop_fx_chain_state_info_t *get_fx_chain_state(shoopdaloop_fx_chain_t *chain);
SHOOP_EXPORT void set_fx_chain_active(shoopdaloop_fx_chain_t *chain, unsigned active);
SHOOP_EXPORT const char *get_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain);
SHOOP_EXPORT void restore_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain, const char* state);
SHOOP_EXPORT unsigned n_fx_chain_audio_input_ports(shoopdaloop_fx_chain_t *chain);
SHOOP_EXPORT unsigned n_fx_chain_audio_output_ports(shoopdaloop_fx_chain_t *chain);
SHOOP_EXPORT unsigned n_fx_chain_midi_input_ports(shoopdaloop_fx_chain_t *chain);
SHOOP_EXPORT shoopdaloop_audio_port_t *fx_chain_audio_input_port(shoopdaloop_fx_chain_t *chain, unsigned int idx);
SHOOP_EXPORT shoopdaloop_audio_port_t *fx_chain_audio_output_port(shoopdaloop_fx_chain_t *chain, unsigned int idx);
SHOOP_EXPORT shoopdaloop_midi_port_t *fx_chain_midi_input_port(shoopdaloop_fx_chain_t *chain, unsigned int idx);

// Audio ports
SHOOP_EXPORT void connect_audio_port_internal(shoopdaloop_audio_port_t *from, shoopdaloop_audio_port_t *to);
SHOOP_EXPORT void connect_audio_port_external(shoopdaloop_audio_port_t *ours, const char* external_port_name);
SHOOP_EXPORT void disconnect_audio_port_external(shoopdaloop_audio_port_t *ours, const char* external_port_name);
SHOOP_EXPORT void disconnect_audio_port_internal(shoopdaloop_audio_port_t *from, shoopdaloop_audio_port_t *to);
SHOOP_EXPORT void set_audio_port_gain(shoopdaloop_audio_port_t *port, float gain);
SHOOP_EXPORT void set_audio_port_muted(shoopdaloop_audio_port_t *port, unsigned muted);
SHOOP_EXPORT void set_audio_port_passthroughMuted(shoopdaloop_audio_port_t *port, unsigned muted);
SHOOP_EXPORT shoop_audio_port_state_info_t *get_audio_port_state(shoopdaloop_audio_port_t *port);
SHOOP_EXPORT shoop_port_connections_state_t *get_audio_port_connections_state(shoopdaloop_audio_port_t *port);
SHOOP_EXPORT void* get_audio_port_driver_handle(shoopdaloop_audio_port_t *port);
SHOOP_EXPORT shoopdaloop_audio_port_t *open_driver_audio_port (
    shoop_backend_session_t *backend,
    shoop_audio_driver_t *driver,
    const char* name_hint,
    shoop_port_direction_t direction,
    unsigned min_always_on_ringbuffer_samples
);
SHOOP_EXPORT shoopdaloop_audio_port_t *open_internal_audio_port (
    shoop_backend_session_t *backend,
    const char* name_hint,
    unsigned min_always_on_ringbuffer_samples
);
SHOOP_EXPORT unsigned get_audio_port_input_connectability(shoopdaloop_audio_port_t* port);
SHOOP_EXPORT unsigned get_audio_port_output_connectability(shoopdaloop_audio_port_t* port);
SHOOP_EXPORT void     set_audio_port_ringbuffer_n_samples (shoopdaloop_audio_port_t* port, unsigned n);

// Midi ports
SHOOP_EXPORT void connect_midi_port_internal(shoopdaloop_midi_port_t *from, shoopdaloop_midi_port_t *to);
SHOOP_EXPORT void connect_midi_port_external(shoopdaloop_midi_port_t *ours, const char* external_port_name);
SHOOP_EXPORT void disconnect_midi_port_external(shoopdaloop_midi_port_t *ours, const char* external_port_name);
SHOOP_EXPORT void disconnect_midi_port_internal(shoopdaloop_midi_port_t *ours, shoopdaloop_midi_port_t *to);
SHOOP_EXPORT shoop_midi_port_state_info_t *get_midi_port_state(shoopdaloop_midi_port_t *port);
SHOOP_EXPORT void set_midi_port_muted(shoopdaloop_midi_port_t *port, unsigned muted);
SHOOP_EXPORT void set_midi_port_passthroughMuted(shoopdaloop_midi_port_t *port, unsigned muted);
SHOOP_EXPORT shoop_port_connections_state_t *get_midi_port_connections_state(shoopdaloop_midi_port_t *port);
SHOOP_EXPORT void* get_midi_port_driver_handle(shoopdaloop_midi_port_t *port);
SHOOP_EXPORT shoopdaloop_midi_port_t *open_driver_midi_port (
    shoop_backend_session_t *backend,
    shoop_audio_driver_t *driver,
    const char* name_hint,
    shoop_port_direction_t direction,
    unsigned min_always_on_ringbuffer_samples
);
SHOOP_EXPORT shoopdaloop_midi_port_t *open_internal_midi_port (
    shoop_backend_session_t *backend,
    const char* name_hint,
    unsigned min_always_on_ringbuffer_samples
);
SHOOP_EXPORT unsigned get_midi_port_input_connectability(shoopdaloop_midi_port_t* port);
SHOOP_EXPORT unsigned get_midi_port_output_connectability(shoopdaloop_midi_port_t* port);
SHOOP_EXPORT void     set_midi_port_ringbuffer_n_samples (shoopdaloop_midi_port_t* port, unsigned n);

// Decoupled midi ports
SHOOP_EXPORT shoopdaloop_decoupled_midi_port_t *open_decoupled_midi_port(shoop_audio_driver_t *driver, const char* name_hint, shoop_port_direction_t direction);
SHOOP_EXPORT shoop_midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port_t *port);
SHOOP_EXPORT void send_decoupled_midi(shoopdaloop_decoupled_midi_port_t *port, unsigned length, unsigned char *data);
SHOOP_EXPORT const char* get_decoupled_midi_port_name(shoopdaloop_decoupled_midi_port_t *port);
SHOOP_EXPORT void close_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port);
SHOOP_EXPORT shoop_port_connections_state_t *get_decoupled_midi_port_connections_state(shoopdaloop_decoupled_midi_port_t *port);
SHOOP_EXPORT void connect_external_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *ours, const char* external_port_name);
SHOOP_EXPORT void disconnect_external_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *ours, const char* external_port_name);

// Helpers for freeing any objects/handles obtained from this API.
// This will always safely destroy, including breaking any made connections to other objects, etc.
SHOOP_EXPORT void destroy_midi_event(shoop_midi_event_t *d);
SHOOP_EXPORT void destroy_midi_sequence(shoop_midi_sequence_t *d);
SHOOP_EXPORT void destroy_audio_channel_data(shoop_audio_channel_data_t *d);
SHOOP_EXPORT void destroy_audio_channel_state_info(shoop_audio_channel_state_info_t *d);
SHOOP_EXPORT void destroy_midi_channel_state_info(shoop_midi_channel_state_info_t *d);
SHOOP_EXPORT void destroy_backend_state_info(shoop_backend_session_state_info_t *d);
SHOOP_EXPORT void destroy_loop(shoopdaloop_loop_t *d);
SHOOP_EXPORT void destroy_audio_port(shoopdaloop_audio_port_t *d);
SHOOP_EXPORT void destroy_midi_port(shoopdaloop_midi_port_t *d);
SHOOP_EXPORT void destroy_midi_port_state_info(shoop_midi_port_state_info_t *d);
SHOOP_EXPORT void destroy_audio_port_state_info(shoop_audio_port_state_info_t *d);
SHOOP_EXPORT void destroy_audio_channel(shoopdaloop_loop_audio_channel_t *d);
SHOOP_EXPORT void destroy_midi_channel(shoopdaloop_loop_midi_channel_t *d);
SHOOP_EXPORT void destroy_shoopdaloop_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *d);
SHOOP_EXPORT void destroy_loop_state_info(shoop_loop_state_info_t *d);
SHOOP_EXPORT void destroy_fx_chain(shoopdaloop_fx_chain_t *d);
SHOOP_EXPORT void destroy_fx_chain_state(shoop_fx_chain_state_info_t *d);
SHOOP_EXPORT void destroy_profiling_report(shoop_profiling_report_t *d);
SHOOP_EXPORT void destroy_string(const char* s);
SHOOP_EXPORT void destroy_port_connections_state(shoop_port_connections_state_t *d);
SHOOP_EXPORT void destroy_logger(shoopdaloop_logger_t *logger);
SHOOP_EXPORT void destroy_audio_driver_state(shoop_audio_driver_state_t *state);
SHOOP_EXPORT void destroy_multichannel_audio(shoop_multichannel_audio_t *audio);
SHOOP_EXPORT void destroy_external_port_descriptors(shoop_external_port_descriptors_t *desc);

// Helpers for allocating data objects
SHOOP_EXPORT shoop_midi_event_t *alloc_midi_event(unsigned data_bytes);
SHOOP_EXPORT shoop_midi_sequence_t *alloc_midi_sequence(unsigned n_events);
SHOOP_EXPORT shoop_audio_channel_data_t *alloc_audio_channel_data(unsigned n_samples);
SHOOP_EXPORT shoop_multichannel_audio_t *alloc_multichannel_audio(unsigned n_channels, unsigned n_frames);

// Logging
SHOOP_EXPORT void initialize_logging();
SHOOP_EXPORT shoopdaloop_logger_t *get_logger(const char* name);
SHOOP_EXPORT void set_global_logging_level(shoop_log_level_t level);
SHOOP_EXPORT void set_logger_level_override(shoopdaloop_logger_t *logger, shoop_log_level_t level);
SHOOP_EXPORT void reset_logger_level_override(shoopdaloop_logger_t *logger);
SHOOP_EXPORT void shoopdaloop_log(shoopdaloop_logger_t *logger, shoop_log_level_t level, const char *msg);
SHOOP_EXPORT unsigned shoopdaloop_should_log(shoopdaloop_logger_t *logger, shoop_log_level_t level);

// For testing purposes
SHOOP_EXPORT void dummy_audio_port_queue_data(shoopdaloop_audio_port_t *port, unsigned n_frames, audio_sample_t const* data);
SHOOP_EXPORT void dummy_audio_port_dequeue_data(shoopdaloop_audio_port_t *port, unsigned n_frames, audio_sample_t * store_in);
SHOOP_EXPORT void dummy_audio_port_request_data(shoopdaloop_audio_port_t* port, unsigned n_frames);
SHOOP_EXPORT void dummy_audio_enter_controlled_mode(shoop_audio_driver_t *driver);
SHOOP_EXPORT void dummy_audio_enter_automatic_mode(shoop_audio_driver_t *driver);
SHOOP_EXPORT unsigned dummy_audio_is_in_controlled_mode(shoop_audio_driver_t *driver);
SHOOP_EXPORT void dummy_audio_request_controlled_frames(shoop_audio_driver_t *driver, unsigned n_frames);
SHOOP_EXPORT void dummy_audio_run_requested_frames(shoop_audio_driver_t *driver);
SHOOP_EXPORT unsigned dummy_audio_n_requested_frames(shoop_audio_driver_t *driver);
SHOOP_EXPORT void dummy_midi_port_queue_data(shoopdaloop_midi_port_t *port, shoop_midi_sequence_t* events);
SHOOP_EXPORT shoop_midi_sequence_t *dummy_midi_port_dequeue_data(shoopdaloop_midi_port_t *port);
SHOOP_EXPORT void dummy_midi_port_request_data(shoopdaloop_midi_port_t* port, unsigned n_frames);
SHOOP_EXPORT void dummy_midi_port_clear_queues(shoopdaloop_midi_port_t* port);
SHOOP_EXPORT void dummy_driver_add_external_mock_port(shoop_audio_driver_t* driver, const char* name, shoop_port_direction_t direction, shoop_port_data_type_t data_type);
SHOOP_EXPORT void dummy_driver_remove_external_mock_port(shoop_audio_driver_t* driver, const char* name);
SHOOP_EXPORT void dummy_driver_remove_all_external_mock_ports(shoop_audio_driver_t* driver);

// Resampling
SHOOP_EXPORT shoop_multichannel_audio_t *resample_audio(shoop_multichannel_audio_t *in, unsigned new_n_frames);

#ifdef __cplusplus
}
#endif