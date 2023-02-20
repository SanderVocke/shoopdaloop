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
    RecordingDryIntoWet,
    LOOP_MODE_INVALID
} loop_mode_t;

typedef enum {
    DoMute,
    DoUnmute,
    DoMuteInput,
    DoUnmuteInput,
    SetPortVolume,
    SetPortPassthrough,
    PORT_ACTION_INVALID
} port_action_t;

typedef struct _shoopdaloop_loop                shoopdaloop_loop_t;
typedef struct _shoopdaloop_loop_audio_channel  shoopdaloop_loop_audio_channel_t;
typedef struct _shoopdaloop_loop_midi_channel   shoopdaloop_loop_midi_channel_t;
typedef struct _shoopdaloop_audio_port          shoopdaloop_audio_port_t;
typedef struct _shoopdaloop_midi_port           shoopdaloop_midi_port_t;
typedef struct _shoopdaloop_decoupled_midi_port shoopdaloop_decoupled_midi_port_t;

typedef struct {
    loop_mode_t mode;
    unsigned length;
    unsigned position;
    loop_mode_t maybe_next_mode;
    unsigned maybe_next_mode_delay;
} loop_state_info_t;

typedef struct {
    float peak;
    float volume;
    const char* name;
} audio_port_state_info_t;

typedef struct {
    unsigned n_events_triggered;
    const char* name;
} midi_port_state_info_t;

typedef enum { Input, Output } port_direction_t;

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
    unsigned int time;
    unsigned int size;
    unsigned char *data;
} midi_event_t;

typedef struct {
    unsigned int n_events;
    midi_event_t **events;
} midi_channel_data_t;

#ifdef __cplusplus
}
#endif