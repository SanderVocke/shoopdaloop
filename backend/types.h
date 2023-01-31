#pragma once

typedef enum {
    Stopped,
    Playing,
    PlayingMuted, // Useful for generating sync while not outputting any sound
    Recording,
    LOOP_STATE_MAX
} loop_state_t;

typedef enum  {
    DoRecord,           // Arg 1 is # of cycles to delay before starting.
    DoRecordNCycles,    // Arg 1 is # of cycles to delay before starting. Arg 2 # of cycles to record. Arg 3 is the next state after finishing.
    DoPlay,             // Arg 1 is # of cycles to delay before starting.
    DoPlayMuted,        // Arg 1 is # of cycles to delay before starting.
    DoStop,             // Arg 1 is # of cycles to delay before starting.
    DoClear,
    SetLoopVolume,      // Arg 1 is the new volume
    LOOP_ACTION_MAX
} loop_action_t;

typedef enum {
    DoMute,
    DoUnmute,
    DoMuteInput,
    DoUnmuteInput,
    SetPortVolume,
    SetPortPassthrough,
    PORT_ACTION_MAX
} port_action_t;

typedef enum {
    Default,
    Profiling,
    Tracing
} backend_features_t;