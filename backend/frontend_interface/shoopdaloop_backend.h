#include <jack/types.h>
#include "types.h"

#ifdef __cplusplus
extern "C" {
#endif

// General
shoopdaloop_backend_instance_t *initialize (audio_system_type_t audio_system_type, const char* client_name_hint);
void terminate (shoopdaloop_backend_instance_t *backend);
jack_client_t *maybe_jack_client_handle(shoopdaloop_backend_instance_t *backend);
const char *get_client_name(shoopdaloop_backend_instance_t *backend);
unsigned get_sample_rate(shoopdaloop_backend_instance_t *backend);
backend_state_info_t *get_backend_state(shoopdaloop_backend_instance_t *backend);
profiling_report_t get_profiling_report(shoopdaloop_backend_instance_t *backend);

// Loops
shoopdaloop_loop_t *create_loop(shoopdaloop_backend_instance_t *backend);
shoopdaloop_loop_audio_channel_t *add_audio_channel (shoopdaloop_loop_t *loop, channel_mode_t mode);
shoopdaloop_loop_midi_channel_t  *add_midi_channel  (shoopdaloop_loop_t *loop, channel_mode_t mode);
shoopdaloop_loop_audio_channel_t *get_audio_channel (shoopdaloop_loop_t *loop, size_t idx);
shoopdaloop_loop_midi_channel_t  *get_midi_channel  (shoopdaloop_loop_t *loop, size_t idx);
unsigned          get_n_audio_channels     (shoopdaloop_loop_t *loop);
unsigned          get_n_midi_channels      (shoopdaloop_loop_t *loop);
void              clear_loop               (shoopdaloop_loop_t *loop, size_t length);
loop_state_info_t *get_loop_state          (shoopdaloop_loop_t *loop);
void              set_loop_length          (shoopdaloop_loop_t *loop, size_t length);
void              set_loop_position        (shoopdaloop_loop_t *loop, size_t position);
void              set_loop_sync_source     (shoopdaloop_loop_t *loop, shoopdaloop_loop_t *sync_source);

// Loop channels
void                   clear_audio_channel      (shoopdaloop_loop_audio_channel_t *channel);
void                   clear_midi_channel       (shoopdaloop_loop_midi_channel_t  *channel);
void                   connect_audio_output     (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port);
void                   connect_midi_output      (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t *port);
void                   connect_audio_input      (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port);
void                   connect_midi_input       (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t *port);
void                   disconnect_audio_outputs (shoopdaloop_loop_audio_channel_t *channel);
void                   disconnect_midi_outputs  (shoopdaloop_loop_midi_channel_t  *channel);
void                   disconnect_audio_output  (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port);
void                   disconnect_midi_output   (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port);
void                   disconnect_audio_inputs (shoopdaloop_loop_audio_channel_t *channel);
void                   disconnect_midi_inputs  (shoopdaloop_loop_midi_channel_t  *channel);
void                   disconnect_audio_input  (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port);
void                   disconnect_midi_input   (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port);
audio_channel_data_t  *get_audio_channel_data   (shoopdaloop_loop_audio_channel_t *channel);
midi_channel_data_t   *get_midi_channel_data    (shoopdaloop_loop_midi_channel_t  *channel);
void                   load_audio_channel_data  (shoopdaloop_loop_audio_channel_t *channel, audio_channel_data_t *data);
void                   load_midi_channel_data   (shoopdaloop_loop_midi_channel_t  *channel, midi_channel_data_t  *data);
audio_channel_state_info_t *get_audio_channel_state  (shoopdaloop_loop_audio_channel_t *channel);
void                   set_audio_channel_volume (shoopdaloop_loop_audio_channel_t *channel, float volume);
midi_channel_state_info_t  *get_midi_channel_state   (shoopdaloop_loop_midi_channel_t  *channel);
void                   set_audio_channel_mode   (shoopdaloop_loop_audio_channel_t * channel, channel_mode_t mode);
void                   set_midi_channel_mode    (shoopdaloop_loop_midi_channel_t * channel, channel_mode_t mode);
void                   set_audio_channel_start_offset (shoopdaloop_loop_audio_channel_t *channel, int offset);
void                   set_midi_channel_start_offset  (shoopdaloop_loop_midi_channel_t *channel, int offset);
void                   set_audio_channel_n_preplay_samples (shoopdaloop_loop_audio_channel_t *channel, unsigned n);
void                   set_midi_channel_n_preplay_samples  (shoopdaloop_loop_midi_channel_t *channel, unsigned n);
void                   clear_audio_channel_data_dirty (shoopdaloop_loop_audio_channel_t * channel);
void                   clear_midi_channel_data_dirty (shoopdaloop_loop_midi_channel_t * channel);

void loop_transition(shoopdaloop_loop_t *loop,
                      loop_mode_t mode,
                      size_t delay, // In # of triggers
                      unsigned wait_for_sync);
void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop_t **loops,
                      loop_mode_t mode,
                      size_t delay, // In # of triggers
                      unsigned wait_for_sync);

// FX chains
shoopdaloop_fx_chain_t *create_fx_chain(shoopdaloop_backend_instance_t *backend, fx_chain_type_t type, const char* title);
void fx_chain_set_ui_visible(shoopdaloop_fx_chain_t *chain, unsigned visible);
fx_chain_state_info_t *get_fx_chain_state(shoopdaloop_fx_chain_t *chain);
void set_fx_chain_active(shoopdaloop_fx_chain_t *chain, unsigned active);
const char *get_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain);
void restore_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain, const char* state);
shoopdaloop_audio_port_t **fx_chain_audio_input_ports(shoopdaloop_fx_chain_t *chain, unsigned int *n_out);
shoopdaloop_audio_port_t **fx_chain_audio_output_ports(shoopdaloop_fx_chain_t *chain, unsigned int *n_out);
shoopdaloop_midi_port_t **fx_chain_midi_input_ports(shoopdaloop_fx_chain_t *chain, unsigned int *n_out);

// Audio ports
void set_audio_port_volume(shoopdaloop_audio_port_t *port, float volume);
void set_audio_port_passthroughVolume(shoopdaloop_audio_port_t *port, float volume);
void set_audio_port_muted(shoopdaloop_audio_port_t *port, unsigned muted);
void set_audio_port_passthroughMuted(shoopdaloop_audio_port_t *port, unsigned muted);
void add_audio_port_passthrough(shoopdaloop_audio_port_t *from, shoopdaloop_audio_port_t *to);
audio_port_state_info_t *get_audio_port_state(shoopdaloop_audio_port_t *port);
// For JACK audio ports only
shoopdaloop_audio_port_t *open_jack_audio_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction);
jack_port_t *get_audio_port_jack_handle(shoopdaloop_audio_port_t *port);

// Midi ports
midi_port_state_info_t *get_midi_port_state(shoopdaloop_midi_port_t *port);
void set_midi_port_muted(shoopdaloop_midi_port_t *port, unsigned muted);
void set_midi_port_passthroughMuted(shoopdaloop_midi_port_t *port, unsigned muted);
void add_midi_port_passthrough(shoopdaloop_midi_port_t *from, shoopdaloop_midi_port_t *to);
// For JACK midi ports only
shoopdaloop_midi_port_t *open_jack_midi_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction);
jack_port_t *get_midi_port_jack_handle(shoopdaloop_midi_port_t *port);

// Decoupled midi ports
shoopdaloop_decoupled_midi_port_t *open_decoupled_midi_port(shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction);
midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port_t *port);
void send_decoupled_midi(shoopdaloop_decoupled_midi_port_t *port, unsigned length, unsigned char *data);

// Helpers for freeing any objects/handles obtained from this API.
// This will always safely destroy, including breaking any made connections to other objects, etc.
void destroy_midi_event(midi_event_t *d);
void destroy_midi_channel_data(midi_channel_data_t *d);
void destroy_audio_channel_data(audio_channel_data_t *d);
void destroy_audio_channel_state_info(audio_channel_state_info_t *d);
void destroy_midi_channel_state_info(midi_channel_state_info_t *d);
void destroy_backend_state_info(backend_state_info_t *d);
void destroy_loop(shoopdaloop_loop_t *d);
void destroy_audio_port(shoopdaloop_audio_port_t *d);
void destroy_midi_port(shoopdaloop_midi_port_t *d);
void destroy_midi_port_state_info(midi_port_state_info_t *d);
void destroy_audio_port_state_info(audio_port_state_info_t *d);
void destroy_audio_channel(shoopdaloop_loop_audio_channel_t *d);
void destroy_midi_channel(shoopdaloop_loop_midi_channel_t *d);
void destroy_shoopdaloop_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *d);
void destroy_loop_state_info(loop_state_info_t *d);
void destroy_fx_chain(shoopdaloop_fx_chain_t *d);
void destroy_fx_chain_state(fx_chain_state_info_t *d);
void destroy_string(const char* s);

// Helpers for allocating data objects
midi_event_t *alloc_midi_event(size_t data_bytes);
midi_channel_data_t *alloc_midi_channel_data(size_t n_events);
audio_channel_data_t *alloc_audio_channel_data(size_t n_samples);

// Logging
shoopdaloop_logger_t *get_logger(const char* name);
void set_global_logging_level(log_level_t level);
void set_logger_level_override(shoopdaloop_logger_t *logger, log_level_t level);
void reset_logger_level_override(shoopdaloop_logger_t *logger);
void shoopdaloop_log(shoopdaloop_logger_t *logger, log_level_t level, const char *msg);

#ifdef __cplusplus
}
#endif