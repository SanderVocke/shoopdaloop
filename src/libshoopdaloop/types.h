#pragma once
#ifdef __cplusplus
extern "C" {
#endif

typedef float audio_sample_t;

// Audio systems.
typedef enum {
    Jack,     // JACK audio
    JackTest, // Internal test backend which mocks part of JACK for connections testing
    Dummy     // Internal test backend aimed at controlled processing
} audio_system_type_t;

// Modes a loop can be in.
typedef enum {
    LoopMode_Unknown,
    LoopMode_Stopped,
    LoopMode_Playing,
    LoopMode_Recording,
    LoopMode_Replacing,
    LoopMode_PlayingDryThroughWet,
    LoopMode_RecordingDryIntoWet,
    LOOP_MODE_INVALID
} loop_mode_t;

typedef enum {
    trace,
    debug,
    info,
    warning,
    error
} log_level_t;

// Modes a channel can be in. They affect how the channel behaves w.r.t.
// the mode its loop is in.
typedef enum {
    // A disabled channel does not record or play back anything, regardless
    // of the loop mode.
    ChannelMode_Disabled,

    // A direct channel is meant for regular looping. It follows the mode
    // of its loop (playback when Playing, record when Recording, etc.).
    ChannelMode_Direct,

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
    ChannelMode_Dry,

    // A wet channel is meant to record a wet signal which has gone through effects/
    // processing/MIDI synthesis. It has some special behaviors:
    // - When Playing, the wet channel will play normally.
    // - When Recording, the wet channel will record normally.
    // - When PlayingDryThroughWet, the wet channel will directly pass through the
    //   incoming signal and not playback anything. That way the effects can be
    //   heard and adjusted live.
    // - When RecordingDryIntoWet, the wet channel will replace its contents.
    //   At the end of the loop, the channel's loop will go into Playing mode.
    ChannelMode_Wet,

    CHANNEL_MODE_INVALID
} channel_mode_t;

typedef enum {
    Carla_Rack, // Carla Rack (1x MIDI, stereo audio w/ fixed internal connections)
    Carla_Patchbay, // Carla Patchbay (1x MIDI, stereo audio w/ flexible internal connections)
    Carla_Patchbay_16x, // Carla Patchbay (1x MIDI, 16x audio w/ flexible internal connections)
    Test2x2x1, // Custom processing chain with 2 audio in- and outputs and 1 MIDI input for testing.
} fx_chain_type_t;

typedef struct _shoopdaloop_loop                shoopdaloop_loop_t;
typedef struct _shoopdaloop_loop_audio_channel  shoopdaloop_loop_audio_channel_t;
typedef struct _shoopdaloop_loop_midi_channel   shoopdaloop_loop_midi_channel_t;
typedef struct _shoopdaloop_audio_port          shoopdaloop_audio_port_t;
typedef struct _shoopdaloop_midi_port           shoopdaloop_midi_port_t;
typedef struct _shoopdaloop_decoupled_midi_port shoopdaloop_decoupled_midi_port_t;
typedef struct _shoopdaloop_backend_instance    shoopdaloop_backend_instance_t;
typedef struct _shoopdaloop_fx_chain            shoopdaloop_fx_chain_t;
typedef struct _shoopdaloop_logger              shoopdaloop_logger_t;

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
    unsigned muted;
    unsigned passthrough_muted;
    const char* name;
} audio_port_state_info_t;

typedef struct {
    unsigned n_events_triggered;
    unsigned n_notes_active;
    unsigned muted;
    unsigned passthrough_muted;
    const char* name;
} midi_port_state_info_t;

typedef struct {
    float dsp_load_percent;
    unsigned xruns_since_last;
    audio_system_type_t actual_type;
} backend_state_info_t;

typedef struct {
    unsigned ready;
    unsigned active;
    unsigned visible;
} fx_chain_state_info_t;

typedef enum { Input, Output } port_direction_t;

typedef struct {
    channel_mode_t mode;
    float volume;
    float output_peak;
    unsigned length;
    int start_offset;
    int played_back_sample; // -1 is none
    unsigned n_preplay_samples;
    unsigned data_dirty;
} audio_channel_state_info_t;

typedef struct {
    channel_mode_t mode;
    unsigned int n_events_triggered;
    unsigned n_notes_active;
    unsigned length;
    int start_offset;
    int played_back_sample; // -1 is none
    unsigned n_preplay_samples;
    unsigned data_dirty;
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
    unsigned length_samples;
} midi_sequence_t;

typedef struct {
    const char* key;
    float n_samples;
    float average;
    float worst;
    float most_recent;
} profiling_report_item_t;

typedef struct {
    unsigned n_items;
    profiling_report_item_t *items;
} profiling_report_t;

typedef struct {
    const char *name;
    unsigned connected;
} port_maybe_connection_t;

typedef struct {
    unsigned n_ports;
    port_maybe_connection_t *ports;
} port_connections_state_t;

#ifdef __cplusplus
}
#endif