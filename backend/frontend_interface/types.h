#pragma once
#ifdef __cplusplus
extern "C" {
#endif

typedef float audio_sample_t;

typedef enum {
    Unknown,
    Stopped,
    Playing,
    PlayingMuted, // Useful for generating sync while not outputting any sound
    Recording,
    Replacing,
    PlayingDryThroughWet,
    RecordingFromDry,
    LOOP_STATE_MAX
} loop_mode_t;

typedef enum {
    DoMute,
    DoUnmute,
    DoMuteInput,
    DoUnmuteInput,
    SetPortVolume,
    SetPortPassthrough,
    PORT_ACTION_MAX
} port_action_t;

typedef struct {
    float volume;
    float output_peak;
} loop_audio_channel_state_t;

typedef struct {
    size_t n_events_triggered;
} loop_midi_channel_state_t;

typedef struct {
    loop_mode_t mode;
    size_t length;
    size_t position;
    float volume;
    size_t n_audio_channels;
    loop_audio_channel_state_t *audio_channel_states;
    size_t n_midi_channels;
    loop_midi_channel_state_t *midi_channel_states;
} loop_state_t;

typedef enum { Input, Output } port_direction_t;

typedef enum {
    Profiling = 1
} backend_features_t;

typedef struct _shoopdaloop_loop                shoopdaloop_loop;
typedef struct _shoopdaloop_loop_audio_channel  shoopdaloop_loop_audio_channel;
typedef struct _shoopdaloop_loop_midi_channel   shoopdaloop_loop_midi_channel;
typedef struct _shoopdaloop_audio_port          shoopdaloop_audio_port;
typedef struct _shoopdaloop_midi_port           shoopdaloop_midi_port;
typedef struct _shoopdaloop_decoupled_midi_port shoopdaloop_decoupled_midi_port;
typedef struct {
    unsigned int            n_output_ports;
    _shoopdaloop_audio_port *output_ports;
    unsigned int            n_input_ports;
    _shoopdaloop_audio_port *input_ports;
    float output_peak;
} audio_channel_state_info_t;

typedef struct {
    unsigned int            n_output_ports;
    _shoopdaloop_midi_port *output_ports;
    unsigned int            n_input_ports;
    _shoopdaloop_midi_port *input_ports;
    unsigned int            n_events_processed;
} midi_channel_state_info_t;

typedef struct {
    // Identifying info
    shoopdaloop_loop *self;

    // Basic
    unsigned int length;
    unsigned int position;
    loop_mode_t mode;
    float        volume;

    // Planned mode changes
    unsigned int n_planned_states;
    loop_mode_t *planned_states;
    unsigned int *planned_state_delays;

    // Audio
    unsigned int n_audio_channels;
    audio_channel_state_info_t *audio_channel_infos;

    // Midi
    unsigned int n_midi_channels;
    midi_channel_state_info_t *midi_channel_infos;
} loop_state_info_t;

typedef struct {
    shoopdaloop_audio_port *self;
    float peak;
    float volume;
    unsigned muted;
} audio_port_state_info_t;

typedef struct {
    shoopdaloop_midi_port *self;
    unsigned muted;
    unsigned n_events_processed;
} midi_port_state_info_t;

typedef struct {
    unsigned int sample_rate;
    unsigned int buffer_size;
    unsigned int n_loops;
    loop_state_info_t* loop_states;
    unsigned int n_audio_ports;
    audio_port_state_info_t* audio_port_states;
    unsigned int n_midi_ports;
    midi_port_state_info_t* midi_port_states;
} state_info_t;

typedef struct {
    unsigned int n_samples;
    audio_sample_t *data;
} audio_channel_data_t;
typedef struct {
    unsigned int n_channels;
    audio_channel_data_t *channels_data;
} audio_data_t;

typedef struct {
    unsigned int time;
    unsigned int size;
    unsigned char *data;
} midi_event_t;

typedef struct {
    unsigned int n_events;
    midi_event_t *events;
} midi_channel_data_t;

typedef struct {
    unsigned int n_channels;
    midi_channel_data_t *channels_data;
} midi_data_t;

typedef struct {
    unsigned int length;
    audio_data_t audio_data;
    midi_data_t  midi_data;
} loop_data_t;

#ifdef __cplusplus
}
#endif