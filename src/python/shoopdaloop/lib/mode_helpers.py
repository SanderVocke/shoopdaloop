from shoop_py_backend import LoopMode

def is_playing_mode(mode):
    return mode in [int(LoopMode.Playing), int(LoopMode.PlayingDryThroughWet)]

def is_recording_mode(mode):
    return mode in [int(LoopMode.Recording), int(LoopMode.RecordingDryIntoWet)]

def is_running_mode(mode):
    return mode in [
        int(LoopMode.Playing),
        int(LoopMode.Replacing),
        int(LoopMode.Recording),
        int(LoopMode.RecordingDryIntoWet),
        int(LoopMode.PlayingDryThroughWet)
    ]
