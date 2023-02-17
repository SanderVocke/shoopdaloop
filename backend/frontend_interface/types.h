#pragma once
#ifdef __cplusplus
extern "C" {
#endif

typedef float audio_sample_t;

typedef enum {
    Unknown,
    Stopped,
    Playing,
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

typedef struct _shoopdaloop_loop                shoopdaloop_loop_t;
typedef struct _shoopdaloop_loop_audio_channel  shoopdaloop_loop_audio_channel_t;
typedef struct _shoopdaloop_loop_midi_channel   shoopdaloop_loop_midi_channel_t;
typedef struct _shoopdaloop_audio_port          shoopdaloop_audio_port_t;
typedef struct _shoopdaloop_midi_port           shoopdaloop_midi_port_t;
typedef struct _shoopdaloop_decoupled_midi_port shoopdaloop_decoupled_midi_port_t;

typedef struct {
    loop_mode_t mode;
    size_t length;
    size_t position;
} loop_state_info_t;

typedef struct {
    float peak;
    float volume;
    const char* name;
} audio_port_state_info_t;

typedef struct {
    size_t n_events_triggered;
    const char* name;
} midi_port_state_info_t;

typedef struct {
    size_t n_loops;
    loop_state_info_t *loop_states;
    size_t n_audio_ports;
    audio_port_state_info_t *audio_port_states;
    size_t n_midi_ports;
    midi_port_state_info_t *midi_port_states;
} state_info_t;

typedef enum { Input, Output } port_direction_t;

typedef enum {
    Profiling = 1
} backend_features_t;

typedef struct {
    float volume;
    float output_peak;
} audio_channel_state_info_t;

typedef struct {
    unsigned int n_events_triggered;
} midi_channel_state_info_t;

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