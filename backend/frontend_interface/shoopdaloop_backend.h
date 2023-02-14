#include <jack/types.h>
#include "types.h"

#ifdef __cplusplus
extern "C" {
#endif

// General
void initialize (const char* client_name_hint);
void terminate ();
jack_client_t *get_jack_client_handle();
const char    *get_jack_client_name();
state_info_t   request_state_update();
unsigned       get_sample_rate();

// Loops
shoopdaloop_loop *create_loop();
shoopdaloop_loop_audio_channel *add_audio_channel (shoopdaloop_loop *loop);
shoopdaloop_loop_midi_channel  *add_midi_channel  (shoopdaloop_loop *loop);
shoopdaloop_loop_audio_channel *get_audio_channel (shoopdaloop_loop *loop, size_t idx);
shoopdaloop_loop_midi_channel  *get_midi_channel  (shoopdaloop_loop *loop, size_t idx);
unsigned          get_n_audio_channels     (shoopdaloop_loop *loop);
unsigned          get_n_midi_channels      (shoopdaloop_loop *loop);
void              delete_loop              (shoopdaloop_loop *loop);
void              clear_loop               (shoopdaloop_loop *loop, size_t length);

// Loop channels
void                  clear_audio_channel      (shoopdaloop_loop_audio_channel *channel);
void                  clear_midi_channel       (shoopdaloop_loop_midi_channel *channel);
void                  delete_audio_channel     (shoopdaloop_loop *loop, shoopdaloop_loop_audio_channel *channel);
void                  delete_midi_channel      (shoopdaloop_loop *loop, shoopdaloop_loop_midi_channel  *channel);
void                  delete_audio_channel_idx (shoopdaloop_loop *loop, size_t idx);
void                  delete_midi_channel_idx  (shoopdaloop_loop *loop, size_t idx);
void                  connect_audio_output     (shoopdaloop_loop_audio_channel *channel, shoopdaloop_audio_port *port);
void                  connect_midi_output      (shoopdaloop_loop_midi_channel  *channel, shoopdaloop_midi_port *port);
void                  connect_audio_input      (shoopdaloop_loop_audio_channel *channel, shoopdaloop_audio_port *port);
void                  connect_midi_input       (shoopdaloop_loop_midi_channel  *channel, shoopdaloop_midi_port *port);
void                  disconnect_audio_outputs (shoopdaloop_loop_audio_channel *channel);
void                  disconnect_midi_outputs  (shoopdaloop_loop_midi_channel  *channel);
void                  disconnect_audio_output  (shoopdaloop_loop_audio_channel *channel, shoopdaloop_audio_port* port);
void                  disconnect_midi_output   (shoopdaloop_loop_midi_channel  *channel, shoopdaloop_midi_port* port);
audio_channel_data_t  get_audio_channel_data   (shoopdaloop_loop_audio_channel *channel);
audio_channel_data_t  get_audio_rms_data       (shoopdaloop_loop_audio_channel *channel,
                                                unsigned from_sample,
                                                unsigned to_sample,
                                                unsigned samples_per_bin);
midi_channel_data_t   get_midi_channel_data    (shoopdaloop_loop_midi_channel  *channel);
void                  load_audio_channel_data  (shoopdaloop_loop_audio_channel *channel, audio_channel_data_t data);
void                  load_midi_channel_data   (shoopdaloop_loop_midi_channel  *channel, midi_channel_data_t  data);

// Actions

// Do / plan a loop transition.
// If wait_for_soft_sync is false, delay is ignored and transition happens
// immediately. Other planned transitions are cancelled in this case.
// Otherwise, the transition is planned. Any already planned transitions
// that happen after this one are cancelled, any already planned transitions
// that happen before are left in place.
void loop_transition(shoopdaloop_loop *loop,
                      loop_state_t state,
                      size_t delay, // In # of triggers
                      unsigned wait_for_soft_sync);
void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop **loops,
                      loop_state_t state,
                      size_t delay, // In # of triggers
                      unsigned wait_for_soft_sync);

// void do_loops_action (unsigned int      n_loops,
//                       shoopdaloop_loop *loops,
//                       loop_action_t     action,
//                       unsigned int      n_args,
//                       float            *maybe_args,
//                       unsigned          wait_soft_sync);

// void do_ports_action(unsigned int             n_audio_ports,
//                      unsigned int             n_midi_ports,
//                      shoopdaloop_audio_port  *audio_ports,
//                      shoopdaloop_midi_port   *midi_ports,
//                      port_action_t            action,
//                      unsigned int             n_args,
//                      float                   *maybe_args);

// Audio ports
shoopdaloop_audio_port *open_audio_port (const char* name_hint, port_direction_t direction);
void close_audio_port (shoopdaloop_audio_port *port);
jack_port_t *get_audio_port_jack_handle(shoopdaloop_audio_port *port);

// Midi ports
shoopdaloop_midi_port *open_midi_port (const char* name_hint, port_direction_t direction);
void close_midi_port (shoopdaloop_midi_port *port);
jack_port_t *get_midi_port_jack_handle(shoopdaloop_midi_port *port);

// Decoupled midi ports
shoopdaloop_decoupled_midi_port *open_decoupled_midi_port(const char* name_hint, port_direction_t direction);
midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port *port);
void close_decoupled_midi_port(shoopdaloop_decoupled_midi_port *port);
void send_decoupled_midi(shoopdaloop_decoupled_midi_port *port, unsigned length, unsigned char *data);

// Helpers for freeing any heap allocations associated with data objects
void free_midi_event(midi_event_t e);
void free_midi_channel_data(midi_channel_data_t d);
void free_audio_channel_data(audio_channel_data_t d);

// Helpers for allocating data objects
midi_event_t alloc_midi_event(size_t data_bytes);
midi_channel_data_t alloc_midi_channel_data(size_t n_events);
audio_channel_data_t alloc_audio_channel_data(size_t n_samples);

#ifdef __cplusplus
}
#endif