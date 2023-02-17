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

// Loops
shoopdaloop_loop_t *create_loop();
shoopdaloop_loop_audio_channel_t *add_audio_channel (shoopdaloop_loop_t *loop);
shoopdaloop_loop_midi_channel_t  *add_midi_channel  (shoopdaloop_loop_t *loop);
shoopdaloop_loop_audio_channel_t *get_audio_channel (shoopdaloop_loop_t *loop, size_t idx);
shoopdaloop_loop_midi_channel_t  *get_midi_channel  (shoopdaloop_loop_t *loop, size_t idx);
unsigned          get_n_audio_channels     (shoopdaloop_loop_t *loop);
unsigned          get_n_midi_channels      (shoopdaloop_loop_t *loop);
void              delete_loop              (shoopdaloop_loop_t *loop);
void              clear_loop               (shoopdaloop_loop_t *loop, size_t length);
loop_state_info_t get_loop_state           (shoopdaloop_loop_t *loop);

// Loop channels
void                  clear_audio_channel      (shoopdaloop_loop_audio_channel_t *channel);
void                  clear_midi_channel       (shoopdaloop_loop_midi_channel_t *channel);
void                  delete_audio_channel     (shoopdaloop_loop_t *loop, shoopdaloop_loop_audio_channel_t *channel);
void                  delete_midi_channel      (shoopdaloop_loop_t *loop, shoopdaloop_loop_midi_channel_t  *channel);
void                  delete_audio_channel_idx (shoopdaloop_loop_t *loop, size_t idx);
void                  delete_midi_channel_idx  (shoopdaloop_loop_t *loop, size_t idx);
void                  connect_audio_output     (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port);
void                  connect_midi_output      (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t *port);
void                  connect_audio_input      (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port);
void                  connect_midi_input       (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t *port);
void                  disconnect_audio_outputs (shoopdaloop_loop_audio_channel_t *channel);
void                  disconnect_midi_outputs  (shoopdaloop_loop_midi_channel_t  *channel);
void                  disconnect_audio_output  (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port);
void                  disconnect_midi_output   (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port);
audio_channel_data_t  get_audio_channel_data   (shoopdaloop_loop_audio_channel_t *channel);
audio_channel_data_t  get_audio_rms_data       (shoopdaloop_loop_audio_channel_t *channel,
                                                unsigned from_sample,
                                                unsigned to_sample,
                                                unsigned samples_per_bin);
midi_channel_data_t   get_midi_channel_data    (shoopdaloop_loop_midi_channel_t  *channel);
void                  load_audio_channel_data  (shoopdaloop_loop_audio_channel_t *channel, audio_channel_data_t data);
void                  load_midi_channel_data   (shoopdaloop_loop_midi_channel_t  *channel, midi_channel_data_t  data);
audio_channel_state_info_t get_audio_channel_state  (shoopdaloop_loop_audio_channel_t *channel);
midi_channel_state_info_t  get_midi_channel_state   (shoopdaloop_loop_midi_channel_t  *channel);

// Actions

// Do / plan a loop transition.
// If wait_for_soft_sync is false, delay is ignored and transition happens
// immediately. Other planned transitions are cancelled in this case.
// Otherwise, the transition is planned. Any already planned transitions
// that happen after this one are cancelled, any already planned transitions
// that happen before are left in place.
void loop_transition(shoopdaloop_loop_t *loop,
                      loop_mode_t mode,
                      size_t delay, // In # of triggers
                      unsigned wait_for_soft_sync);
void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop_t **loops,
                      loop_mode_t mode,
                      size_t delay, // In # of triggers
                      unsigned wait_for_soft_sync);

// void do_loops_action (unsigned int      n_loops,
//                       shoopdaloop_loop_t *loops,
//                       loop_action_t     action,
//                       unsigned int      n_args,
//                       float            *maybe_args,
//                       unsigned          wait_soft_sync);

// void do_ports_action(unsigned int             n_audio_ports,
//                      unsigned int             n_midi_ports,
//                      shoopdaloop_audio_port_t  *audio_ports,
//                      shoopdaloop_midi_port_t   *midi_ports,
//                      port_action_t            action,
//                      unsigned int             n_args,
//                      float                   *maybe_args);

// Audio ports
shoopdaloop_audio_port_t *open_audio_port (const char* name_hint, port_direction_t direction);
void close_audio_port (shoopdaloop_audio_port_t *port);
jack_port_t *get_audio_port_jack_handle(shoopdaloop_audio_port_t *port);
audio_port_state_info_t get_audio_port_state(shoopdaloop_audio_port_t *port);

// Midi ports
shoopdaloop_midi_port_t *open_midi_port (const char* name_hint, port_direction_t direction);
void close_midi_port (shoopdaloop_midi_port_t *port);
jack_port_t *get_midi_port_jack_handle(shoopdaloop_midi_port_t *port);
midi_port_state_info_t get_midi_port_state(shoopdaloop_midi_port_t *port);

// Decoupled midi ports
shoopdaloop_decoupled_midi_port_t *open_decoupled_midi_port(const char* name_hint, port_direction_t direction);
midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port_t *port);
void close_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port);
void send_decoupled_midi(shoopdaloop_decoupled_midi_port_t *port, unsigned length, unsigned char *data);

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