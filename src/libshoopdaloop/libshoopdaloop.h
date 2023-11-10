#include "types.h"
#include "export.h"

#ifdef __cplusplus
extern "C" {
#endif

// General
SHOOP_EXPORT shoopdaloop_backend_instance_t *initialize (audio_system_type_t audio_system_type, const char* client_name_hint, const char* argstring);
SHOOP_EXPORT void terminate_backend (shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT void* maybe_jack_client_handle(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT const char *get_client_name(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT unsigned get_sample_rate(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT unsigned get_buffer_size(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT backend_state_info_t *get_backend_state(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT profiling_report_t* get_profiling_report(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT unsigned has_audio_system_support(audio_system_type_t audio_system_type);

// Loops
SHOOP_EXPORT shoopdaloop_loop_t *create_loop(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT shoopdaloop_loop_audio_channel_t *add_audio_channel (shoopdaloop_loop_t *loop, channel_mode_t mode);
SHOOP_EXPORT shoopdaloop_loop_midi_channel_t  *add_midi_channel  (shoopdaloop_loop_t *loop, channel_mode_t mode);
SHOOP_EXPORT shoopdaloop_loop_audio_channel_t *get_audio_channel (shoopdaloop_loop_t *loop, unsigned idx);
SHOOP_EXPORT shoopdaloop_loop_midi_channel_t  *get_midi_channel  (shoopdaloop_loop_t *loop, unsigned idx);
SHOOP_EXPORT unsigned          get_n_audio_channels     (shoopdaloop_loop_t *loop);
SHOOP_EXPORT unsigned          get_n_midi_channels      (shoopdaloop_loop_t *loop);
SHOOP_EXPORT void              clear_loop               (shoopdaloop_loop_t *loop, unsigned length);
SHOOP_EXPORT loop_state_info_t *get_loop_state          (shoopdaloop_loop_t *loop);
SHOOP_EXPORT void              set_loop_length          (shoopdaloop_loop_t *loop, unsigned length);
SHOOP_EXPORT void              set_loop_position        (shoopdaloop_loop_t *loop, unsigned position);
SHOOP_EXPORT void              set_loop_sync_source     (shoopdaloop_loop_t *loop, shoopdaloop_loop_t *sync_source);

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
SHOOP_EXPORT audio_channel_data_t  *get_audio_channel_data   (shoopdaloop_loop_audio_channel_t *channel);
SHOOP_EXPORT midi_sequence_t   *get_midi_channel_data    (shoopdaloop_loop_midi_channel_t  *channel);
SHOOP_EXPORT void                   load_audio_channel_data  (shoopdaloop_loop_audio_channel_t *channel, audio_channel_data_t *data);
SHOOP_EXPORT void                   load_midi_channel_data   (shoopdaloop_loop_midi_channel_t  *channel, midi_sequence_t  *data);
SHOOP_EXPORT audio_channel_state_info_t *get_audio_channel_state  (shoopdaloop_loop_audio_channel_t *channel);
SHOOP_EXPORT void                   set_audio_channel_volume (shoopdaloop_loop_audio_channel_t *channel, float volume);
SHOOP_EXPORT midi_channel_state_info_t  *get_midi_channel_state   (shoopdaloop_loop_midi_channel_t  *channel);
SHOOP_EXPORT void                   set_audio_channel_mode   (shoopdaloop_loop_audio_channel_t * channel, channel_mode_t mode);
SHOOP_EXPORT void                   set_midi_channel_mode    (shoopdaloop_loop_midi_channel_t * channel, channel_mode_t mode);
SHOOP_EXPORT void                   set_audio_channel_start_offset (shoopdaloop_loop_audio_channel_t *channel, int offset);
SHOOP_EXPORT void                   set_midi_channel_start_offset  (shoopdaloop_loop_midi_channel_t *channel, int offset);
SHOOP_EXPORT void                   set_audio_channel_n_preplay_samples (shoopdaloop_loop_audio_channel_t *channel, unsigned n);
SHOOP_EXPORT void                   set_midi_channel_n_preplay_samples  (shoopdaloop_loop_midi_channel_t *channel, unsigned n);
SHOOP_EXPORT void                   clear_audio_channel_data_dirty (shoopdaloop_loop_audio_channel_t * channel);
SHOOP_EXPORT void                   clear_midi_channel_data_dirty (shoopdaloop_loop_midi_channel_t * channel);
SHOOP_EXPORT 
SHOOP_EXPORT void loop_transition(shoopdaloop_loop_t *loop,
                      loop_mode_t mode,
                      unsigned delay, // In # of triggers
                      unsigned wait_for_sync);
SHOOP_EXPORT void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop_t **loops,
                      loop_mode_t mode,
                      unsigned delay, // In # of triggers
                      unsigned wait_for_sync);

// FX chains
SHOOP_EXPORT shoopdaloop_fx_chain_t *create_fx_chain(shoopdaloop_backend_instance_t *backend, fx_chain_type_t type, const char* title);
SHOOP_EXPORT void fx_chain_set_ui_visible(shoopdaloop_fx_chain_t *chain, unsigned visible);
SHOOP_EXPORT fx_chain_state_info_t *get_fx_chain_state(shoopdaloop_fx_chain_t *chain);
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
SHOOP_EXPORT void set_audio_port_volume(shoopdaloop_audio_port_t *port, float volume);
SHOOP_EXPORT void set_audio_port_muted(shoopdaloop_audio_port_t *port, unsigned muted);
SHOOP_EXPORT void set_audio_port_passthroughMuted(shoopdaloop_audio_port_t *port, unsigned muted);
SHOOP_EXPORT void add_audio_port_passthrough(shoopdaloop_audio_port_t *from, shoopdaloop_audio_port_t *to);
SHOOP_EXPORT audio_port_state_info_t *get_audio_port_state(shoopdaloop_audio_port_t *port);
SHOOP_EXPORT port_connections_state_t *get_audio_port_connections_state(shoopdaloop_audio_port_t *port);
SHOOP_EXPORT void connect_external_audio_port(shoopdaloop_audio_port_t *ours, const char* external_port_name);
SHOOP_EXPORT void disconnect_external_audio_port(shoopdaloop_audio_port_t *ours, const char* external_port_name);
SHOOP_EXPORT shoopdaloop_audio_port_t *open_audio_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction);

// For JACK audio ports only
SHOOP_EXPORT void* get_audio_port_jack_handle(shoopdaloop_audio_port_t *port);

// Midi ports
SHOOP_EXPORT midi_port_state_info_t *get_midi_port_state(shoopdaloop_midi_port_t *port);
SHOOP_EXPORT void set_midi_port_muted(shoopdaloop_midi_port_t *port, unsigned muted);
SHOOP_EXPORT void set_midi_port_passthroughMuted(shoopdaloop_midi_port_t *port, unsigned muted);
SHOOP_EXPORT void add_midi_port_passthrough(shoopdaloop_midi_port_t *from, shoopdaloop_midi_port_t *to);
SHOOP_EXPORT port_connections_state_t *get_midi_port_connections_state(shoopdaloop_midi_port_t *port);
SHOOP_EXPORT void connect_external_midi_port(shoopdaloop_midi_port_t *ours, const char* external_port_name);
SHOOP_EXPORT void disconnect_external_midi_port(shoopdaloop_midi_port_t *ours, const char* external_port_name);
SHOOP_EXPORT shoopdaloop_midi_port_t *open_midi_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction);

// For JACK midi ports only
SHOOP_EXPORT void* get_midi_port_jack_handle(shoopdaloop_midi_port_t *port);

// Decoupled midi ports
SHOOP_EXPORT shoopdaloop_decoupled_midi_port_t *open_decoupled_midi_port(shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction);
SHOOP_EXPORT midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port_t *port);
SHOOP_EXPORT void send_decoupled_midi(shoopdaloop_decoupled_midi_port_t *port, unsigned length, unsigned char *data);
SHOOP_EXPORT const char* get_decoupled_midi_port_name(shoopdaloop_decoupled_midi_port_t *port);
SHOOP_EXPORT void close_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port);

// Helpers for freeing any objects/handles obtained from this API.
// This will always safely destroy, including breaking any made connections to other objects, etc.
SHOOP_EXPORT void destroy_midi_event(midi_event_t *d);
SHOOP_EXPORT void destroy_midi_sequence(midi_sequence_t *d);
SHOOP_EXPORT void destroy_audio_channel_data(audio_channel_data_t *d);
SHOOP_EXPORT void destroy_audio_channel_state_info(audio_channel_state_info_t *d);
SHOOP_EXPORT void destroy_midi_channel_state_info(midi_channel_state_info_t *d);
SHOOP_EXPORT void destroy_backend_state_info(backend_state_info_t *d);
SHOOP_EXPORT void destroy_loop(shoopdaloop_loop_t *d);
SHOOP_EXPORT void destroy_audio_port(shoopdaloop_audio_port_t *d);
SHOOP_EXPORT void destroy_midi_port(shoopdaloop_midi_port_t *d);
SHOOP_EXPORT void destroy_midi_port_state_info(midi_port_state_info_t *d);
SHOOP_EXPORT void destroy_audio_port_state_info(audio_port_state_info_t *d);
SHOOP_EXPORT void destroy_audio_channel(shoopdaloop_loop_audio_channel_t *d);
SHOOP_EXPORT void destroy_midi_channel(shoopdaloop_loop_midi_channel_t *d);
SHOOP_EXPORT void destroy_shoopdaloop_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *d);
SHOOP_EXPORT void destroy_loop_state_info(loop_state_info_t *d);
SHOOP_EXPORT void destroy_fx_chain(shoopdaloop_fx_chain_t *d);
SHOOP_EXPORT void destroy_fx_chain_state(fx_chain_state_info_t *d);
SHOOP_EXPORT void destroy_profiling_report(profiling_report_t *d);
SHOOP_EXPORT void destroy_string(const char* s);
SHOOP_EXPORT void destroy_port_connections_state(port_connections_state_t *d);
SHOOP_EXPORT void destroy_logger(shoopdaloop_logger_t *logger);

// Helpers for allocating data objects
SHOOP_EXPORT midi_event_t *alloc_midi_event(unsigned data_bytes);
SHOOP_EXPORT midi_sequence_t *alloc_midi_sequence(unsigned n_events);
SHOOP_EXPORT audio_channel_data_t *alloc_audio_channel_data(unsigned n_samples);

// Logging
SHOOP_EXPORT shoopdaloop_logger_t *get_logger(const char* name);
SHOOP_EXPORT void set_global_logging_level(log_level_t level);
SHOOP_EXPORT void set_logger_level_override(shoopdaloop_logger_t *logger, log_level_t level);
SHOOP_EXPORT void reset_logger_level_override(shoopdaloop_logger_t *logger);
SHOOP_EXPORT void shoopdaloop_log(shoopdaloop_logger_t *logger, log_level_t level, const char *msg);
SHOOP_EXPORT unsigned shoopdaloop_should_log(shoopdaloop_logger_t *logger, log_level_t level);

// For testing purposes
SHOOP_EXPORT void dummy_audio_port_queue_data(shoopdaloop_audio_port_t *port, unsigned n_frames, audio_sample_t const* data);
SHOOP_EXPORT void dummy_audio_port_dequeue_data(shoopdaloop_audio_port_t *port, unsigned n_frames, audio_sample_t * store_in);
SHOOP_EXPORT void dummy_audio_port_request_data(shoopdaloop_audio_port_t* port, unsigned n_frames);
SHOOP_EXPORT void dummy_audio_enter_controlled_mode(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT void dummy_audio_enter_automatic_mode(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT unsigned dummy_audio_is_in_controlled_mode(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT void dummy_audio_request_controlled_frames(shoopdaloop_backend_instance_t *backend, unsigned n_frames);
SHOOP_EXPORT unsigned dummy_audio_n_requested_frames(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT void dummy_audio_wait_process(shoopdaloop_backend_instance_t *backend);
SHOOP_EXPORT void dummy_midi_port_queue_data(shoopdaloop_midi_port_t *port, midi_sequence_t* events);
SHOOP_EXPORT midi_sequence_t   *dummy_midi_port_dequeue_data(shoopdaloop_midi_port_t *port);
SHOOP_EXPORT void dummy_midi_port_request_data(shoopdaloop_midi_port_t* port, unsigned n_frames);
SHOOP_EXPORT void dummy_midi_port_clear_queues(shoopdaloop_midi_port_t* port);

#ifdef __cplusplus
}
#endif