# Four-way native frontend evidence

The normal native frontend attaches through TCP on the container's non-loopback
LAN address to each audio-only renderer. These screenshots document recording,
playback, positive stereo waveform readback and meters. Compact additionally
exercises history grabbing, stop/start, gain, loop creation and track deletion/
recreation. Screenshots are illustrative and are not callback timing evidence;
the full test suite was running concurrently.

- [Current playback](current-playback-waveform.png).
- [Chunks playback](chunks-playback-waveform.png).
- [Descriptors playback](chunks-descriptors-playback-waveform.png).
- [Compact recording/playback](compact-recorded-waveform.png).
- [Compact history grab](compact-grab-waveform.png).
- [Added loop](compact-added-loop.png).
- [Recreated track](compact-recreated-track.png).
- [Compact on the real JACK service](compact-jack.png). All four modes attach
  and create tracks through this service, with dummy hardware backing.

Environment: `ccarenv_b64_Y2NhcmVudl8wYjY0ZjlmMGI1NWM4MTkxYjY4NTU0MDkzYWE5NWUyZg`.
The shared-desktop capability reported enabled/state unknown, but no desktop JS
control tool was exposed. Native windows were inspected on local Xvfb with X11
input and screenshots instead. No physical CPAL sound device is available here;
the existing real-driver CPAL mock covers all four modes and MIDI exclusion.
