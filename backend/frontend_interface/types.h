#pragma once
#ifdef __cplusplus
extern "C" {
#endif

typedef float audio_sample_t;

// Modes a loop can be in.
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

// Modes a channel can be in. They affect how the channel behaves w.r.t.
// the mode its loop is in.
typedef enum {
    // A disabled channel does not record or play back anything, regardless
    // of the loop mode.
    Disabled,

    // A direct channel is meant for regular looping. It follows the mode
    // of its loop (playback when Playing, record when Recording, etc.).
    Direct,

    // A dry channel is meant to record a dry signal coming in, which will
    // be fed back around into a wet channel. It has some special behaviors:
    // - When Playing, a dry channel will not playback anything. The assumption
    //   is that a wet signal is already being played from the wet channel, no
    //   point to play the dry one as well.
    // - When Recording, a dry channel records.
    // - When PlayingDryThroughWet, the dry channel will playback the dry signal
    //   so that live effects can be applied.
    // - When RecordingDryIntoWet, the dry channel will playback the dry signal
    //   so that it may be re-recorded into the wet channel. At the end of the
    //   loop, the channel's loop will go into Playing mode.
    Dry,

    // A wet channel is meant to record a wet signal which has gone through effects/
    // processing/MIDI synthesis. It has some special behaviors:
    // - When Playing, the wet channel will play normally.
    // - When Recording, the wet channel will record normally.
    // - When PlayingDryThroughWet, the wet channel will directly pass through the
    //   incoming signal and not playback anything. That way the effects can be
    //   heard and adjusted live.
    // - When RecordingDryIntoWet, the wet channel will replace its contents.
    //   At the end of the loop, the channel's loop will go into Playing mode.
    Wet,

    CHANNEL_MODE_INVALID
} channel_mode_t;

// typedef enum {
//     DoMute,
//     DoUnmute,
//     DoMuteInput,
//     DoUnmuteInput,
//     SetPortVolume,
//     SetPortPassthrough,
//     PORT_ACTION_INVALID
// } port_action_t;

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
    channel_mode_t mode;
    float volume;
    float output_peak;
} audio_channel_state_info_t;

typedef struct {
    channel_mode_t mode;
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