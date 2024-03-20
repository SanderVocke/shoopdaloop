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
} shoop_audio_driver_type_t;

typedef enum {
    ShoopPortDataType_Audio,
    ShoopPortDataType_Midi,
    ShoopPortDataType_Any
} shoop_port_data_type_t;

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
} shoop_loop_mode_t;

typedef enum {
    log_level_debug_trace,  // Will not be printed in release build
    log_level_always_trace, // Same look as trace, but will be printed in release build
    log_level_debug,
    log_level_info,
    log_level_warning,
    log_level_error
} shoop_log_level_t;

typedef enum {
    Success,
    Failure
} shoop_result_t;

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
} shoop_channel_mode_t;

typedef enum {
    Carla_Rack, // Carla Rack (1x MIDI, stereo audio w/ fixed internal connections)
    Carla_Patchbay, // Carla Patchbay (1x MIDI, stereo audio w/ flexible internal connections)
    Carla_Patchbay_16x, // Carla Patchbay (1x MIDI, 16x audio w/ flexible internal connections)
    Test2x2x1, // Custom processing chain with 2 audio in- and outputs and 1 MIDI input for testing.
} shoop_fx_chain_type_t;

// Can be ORed
typedef enum {
    Internal = 1, // Can connect to internal ports
    External = 2, // Can connect to external ports
} shoop_port_connectability_t;

typedef struct _shoopdaloop_loop                shoopdaloop_loop_t;
typedef struct _shoopdaloop_loop_audio_channel  shoopdaloop_loop_audio_channel_t;
typedef struct _shoopdaloop_loop_midi_channel   shoopdaloop_loop_midi_channel_t;
typedef struct _shoopdaloop_audio_port          shoopdaloop_audio_port_t;
typedef struct _shoopdaloop_midi_port           shoopdaloop_midi_port_t;
typedef struct _shoopdaloop_decoupled_midi_port shoopdaloop_decoupled_midi_port_t;
typedef struct _shoopdaloop_backend_session     shoop_backend_session_t;
typedef struct _shoopdaloop_fx_chain            shoopdaloop_fx_chain_t;
typedef struct _shoopdaloop_logger              shoopdaloop_logger_t;
typedef struct _shoopdaloop_audio_driver        shoop_audio_driver_t;

typedef struct {
    shoop_loop_mode_t mode;
    unsigned length;
    unsigned position;
    shoop_loop_mode_t maybe_next_mode;
    unsigned maybe_next_mode_delay;
} shoop_loop_state_info_t;

typedef struct {
    float input_peak;
    float output_peak;
    float gain;
    unsigned muted;
    unsigned passthrough_muted;
    const char* name;
} shoop_audio_port_state_info_t;

typedef struct {
    unsigned n_input_events;
    unsigned n_input_notes_active;
    unsigned n_output_events;
    unsigned n_output_notes_active;
    unsigned muted;
    unsigned passthrough_muted;
    const char* name;
} shoop_midi_port_state_info_t;

typedef struct {
    shoop_audio_driver_t * audio_driver;
} shoop_backend_session_state_info_t;

typedef struct {
    float dsp_load_percent;
    unsigned xruns_since_last;
    void * maybe_driver_handle; // For direct interaction with the driver from the front-end
    const char* maybe_instance_name; // E.g. the client name assigned in JACK'
    unsigned sample_rate;
    unsigned buffer_size;
    unsigned active;
    unsigned last_processed; // Amount of frames processed in most recent process iteration (normally equal to buffer_size)
} shoop_audio_driver_state_t;

typedef struct {
    unsigned ready;
    unsigned active;
    unsigned visible;
} shoop_fx_chain_state_info_t;

typedef struct {
    const char* client_name_hint;
    const char* maybe_server_name;
} shoop_jack_audio_driver_settings_t;

typedef struct {
    const char* client_name;
    unsigned sample_rate;
    unsigned buffer_size;
} shoop_dummy_audio_driver_settings_t;

typedef enum {
    ShoopPortDirection_Input,
    ShoopPortDirection_Output,
    ShoopPortDirection_Any
} shoop_port_direction_t;

typedef struct {
    shoop_channel_mode_t mode;
    float gain;
    float output_peak;
    unsigned length;
    int start_offset;
    int played_back_sample; // -1 is none
    unsigned n_preplay_samples;
    unsigned data_dirty;
} shoop_audio_channel_state_info_t;

typedef struct {
    shoop_channel_mode_t mode;
    unsigned int n_events_triggered;
    unsigned n_notes_active;
    unsigned length;
    int start_offset;
    int played_back_sample; // -1 is none
    unsigned n_preplay_samples;
    unsigned data_dirty;
} shoop_midi_channel_state_info_t;

typedef struct {
    unsigned int n_samples;
    audio_sample_t *data;
} shoop_audio_channel_data_t;

typedef struct {
    // Regular messages have time >= 0. However, a midi sequence can also be used to store
    // the state of a MIDI port at the time of recording start. Such state should be saved
    // as MIDI messages with time = -1.
    int time;
    unsigned int size;
    unsigned char *data;
} shoop_midi_event_t;

typedef struct {
    unsigned int n_events;
    shoop_midi_event_t **events;
    unsigned length_samples;
} shoop_midi_sequence_t;

typedef struct {
    const char* key;
    float n_samples;
    float average;
    float worst;
    float most_recent;
} shoop_profiling_report_item_t;

typedef struct {
    unsigned n_items;
    shoop_profiling_report_item_t *items;
} shoop_profiling_report_t;

typedef struct {
    const char *name;
    unsigned connected;
} shoop_port_maybe_connection_t;

typedef struct {
    unsigned n_ports;
    shoop_port_maybe_connection_t *ports;
} shoop_port_connections_state_t;

typedef struct {
    unsigned n_channels;
    unsigned n_frames;
    audio_sample_t *data; // Channels are not interleaved
} shoop_multichannel_audio_t;

typedef struct {
    shoop_port_data_type_t data_type;
    shoop_port_direction_t direction;
    const char* name;
} shoop_external_port_descriptor_t;

typedef struct {
    unsigned n_ports;
    shoop_external_port_descriptor_t *ports;
} shoop_external_port_descriptors_t;

#ifdef __cplusplus
}
#endif