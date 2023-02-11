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
void              add_audio_channel        (shoopdaloop_loop *loop);
void              add_midi_channel         (shoopdaloop_loop *loop);
unsigned          get_n_audio_channels     (shoopdaloop_loop *loop);
unsigned          get_n_midi_channels      (shoopdaloop_loop *loop);
void              delete_audio_channel     (shoopdaloop_loop *loop, unsigned int audio_channel_idx);
void              delete_midi_channel      (shoopdaloop_loop *loop, unsigned int midi_channel_idx);
void              delete_loop              (shoopdaloop_loop *loop);
void              connect_audio_output     (shoopdaloop_loop *loop, unsigned int audio_channel_idx, shoopdaloop_audio_port *port);
void              connect_midi_output      (shoopdaloop_loop *loop, unsigned int midi_channel_idx, shoopdaloop_midi_port *port);
void              disconnect_audio_outputs (shoopdaloop_loop *loop, unsigned int audio_channel_idx);
void              disconnect_midi_outputs  (shoopdaloop_loop *loop, unsigned int midi_channel_idx);
loop_data_t       get_data                 (shoopdaloop_loop *loop);
void              load_data                (shoopdaloop_loop *loop, loop_data_t data);
loop_data_t       get_loop_audio_rms       (shoopdaloop_loop *loop,
                                            unsigned from_sample,
                                            unsigned to_sample,
                                            unsigned samples_per_bin,
                                            audio_sample_t **data_out);

// Actions
void do_loops_action (unsigned int      n_loops,
                      shoopdaloop_loop *loops,
                      loop_action_t     action,
                      unsigned int      n_args,
                      float            *maybe_args,
                      unsigned          wait_soft_sync);

void do_ports_action(unsigned int             n_audio_ports,
                     unsigned int             n_midi_ports,
                     shoopdaloop_audio_port  *audio_ports,
                     shoopdaloop_midi_port   *midi_ports,
                     port_action_t            action,
                     unsigned int             n_args,
                     float                   *maybe_args);

// Audio ports
shoopdaloop_audio_port *open_audio_port (const char* name_hint);
void close_audio_port (shoopdaloop_audio_port *port);
jack_port_t *get_audio_port_jack_handle(shoopdaloop_audio_port *port);

// Midi ports
shoopdaloop_midi_port *open_midi_port (const char* name_hint);
void close_midi_port (shoopdaloop_midi_port *port);
jack_port_t *get_midi_port_jack_handle(shoopdaloop_midi_port *port);

// Slow midi ports
shoopdaloop_slow_midi_port *create_slow_midi_port(const char* name, port_direction_t kind);
void set_slow_midi_port_received_callback(shoopdaloop_slow_midi_port *port, SlowMIDIReceivedCallback callback);
void destroy_slow_midi_port(shoopdaloop_slow_midi_port *port);
void send_slow_midi(shoopdaloop_slow_midi_port *port, unsigned length, unsigned char *data);
void process_slow_midi();

// For freeing any returned allocated pointers
void shoopdaloop_free(void* ptr);

#ifdef __cplusplus
}
#endif