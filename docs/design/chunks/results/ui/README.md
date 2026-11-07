# Native UI workflow evidence

These screenshots document a real native app attached through a socket host
profile to the descriptor renderer on Xvfb during implementation (`54dea73b`).
They are UI evidence, not timing results; builds were running during some captures.
The final `df4ba416` host runs separately verify public app commands and stereo
media readback after the capture-path fixes.

- [Recorded playback and stereo waveform](playback-waveform.png)
- [History grab and waveform](grab-waveform.png)
- [Additional loop](added-loop.png)
- [Track removed and recreated](recreated-track.png)

The workflow also exercised stop/start, gain, monitoring and meters.
