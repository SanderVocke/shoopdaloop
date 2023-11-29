#pragma once
#include <RtAudio.h>
#include <memory>

// Hide the RtAudio/RtMidi API behind a class with redirections, such that
// we can mock it out in tests.
// TODO: this does not contain the complete API, only what we use
class RtAudioApi {
    RtAudio m_audio;
public:
    RtAudioApi() {}

    int getDeviceCount() { return m_audio.getDeviceCount(); }
    RtAudioDeviceInfo getDeviceInfo(int idx) { return m_audio.getDeviceInfo(idx); }
    void setStreamCallback();
    // RtError wrapper with printMessage
    void startStream();
    void stopStream();
    void closeStream();
};