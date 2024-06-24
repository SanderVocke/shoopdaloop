![Logo](./src/shoopdaloop/resources/logo-small.png)

[![codecov](https://codecov.io/github/SanderVocke/shoopdaloop/graph/badge.svg?token=15RLMBAYV7)](https://codecov.io/github/SanderVocke/shoopdaloop)
![build](https://github.com/sandervocke/shoopdaloop/actions/workflows/build_and_test.yml/badge.svg)
[![Docs](https://github.com/SanderVocke/shoopdaloop/actions/workflows/docs.yml/badge.svg)](https://github.com/SanderVocke/shoopdaloop/actions/workflows/docs.yml)

# ShoopDaLoop - Limitless Looping

ShoopDaLoop is an cross-platform advanced live looping application. It offers intuitive looping of audio+MIDI clips in a grid-style layout and some DAW-like features for sequencing.

It is designed for both free-form jamming (solo or together) and pre-scripted looping sessions. Live performance could be a good use-case, although it is not stable enough yet to recommend that at this point.

[Documentation](https://sandervocke.github.io/shoopdaloop/) lives here.

> :warning: **Current releases (<v1.0.0) are development releases. Feel free to give this a test drive, but don't expect a finished product.**

For currently open known issues, check the GitHub Issues page.

# Screenshot

![Screenshot](docs/source/resources/screenshot.png)

# In a nutshell

- **Fast**: can easily handle a large number of loops.
- **Grid-based**: loops are organized into a grid of tracks, which share inputs/outputs and effects/synthesis.
- **MIDI and audio**: can both be looped, including alongside each other in the same loop.
- **FX/synthesis**: can be inserted into a loop directly (plugins) or via external JACK connections. The same loop can simultaneously record "dry" and "wet" (post-fx) to save precious CPU during playback.
- **Synchronization**: every loop is synced to the "sync loop", which typically holds a beat, click-track or just fixed-length silence. Loops may also be a multiple of the sync loop length.
- **Grabbing**: instead of triggering the recording, you can grab loops retroactively from an always-on recording buffer.
- **Combining**: A powerful system for combining multiple loops into sequences and/or scenes, which can also be controlled as if they were loops.
- **NSM**: Non/New Session Manager support on Linux (experimental).
- **Hackable**: Users can run their own Lua scripts to control the application in new ways.

These features are explained in detail in the [docs](https://sandervocke.github.io/shoopdaloop/).

# Status

ShoopDaLoop is in early development. The basics work but not nearly all of its intended functionality is finished yet (see below).

The intent is for the master branch to always work and pass CI tests, but at the moment, some significant bugs will probably come and go.
Development releases are made now and then, but until a v1.0.0 they are seen as a preview and not expected to be very stable.

As such, it obviously has not been used for on-stage performing and definitely shouldn't until after doing some serious testing.

Note however that having automated testing with high coverage is among the project goals.

# Comparison table

To summarize why ShoopDaLoop exists and what the goals and plans are, a short comparison table with similar open-source software gives a good picture. Of course, the devil is in the details: the other listed software is great and in many ways different and better. The aim here is not to take away from them in any way.

|                             | ShoopDaLoop               | SooperLooper     | Luppp                  | Ardour                   |
|-----------------------------|---------------------------|------------------|------------------------|--------------------------|
| OS                          | Linux <sup>(2)</sup>      | Linux, Mac       | Linux                    | Linux, Mac, Windows    |
| MIDI looping                | ✅                        | ❌              | ❌                       | ✅                      |
| Audio+MIDI co-recording     | ✅                        | ❌              | ❌                       | ?                      |
| Audio dry+wet co-recording  | ✅                        | manual setup     | ✅                      | ?                      |
| Loop Organization           | Grid                      | Separate loopers | Grid                     | Grid                   |
| Scenes support              | ✅ (5)                    | ❌              | ✅ (grid row = scene)    | ?                      |
| Designed for live use       | ✅                        | ✅               | ✅                      | ❌                      |
| Plugin Host                 | ✅ <sup>(1)</sup>         | ❌               | ❌                      | ✅                      |
| Song/performance sequencing | ✅ <sup>(4)</sup>         | ❌               | ❌                      | ✅ (not sure of details) |
| MIDI controller support     | ✅ (learn / script) <sup>(6)</sup> | ✅ (MIDI learn)  | ✅ (not sure of method) | ✅ (not sure of method) |
| NSM Session Management      | ✅                        | ✅               | ✅                      | ✅                      |
| Overdubbing                 | ❌ (planned)              | ✅               | ✅                      | ?                      |
| Plugin scripts              | ✅ <sup>(3)</sup>         | ❌               | ❌                      | ✅                     |
| Transport/tempo system      | None (trigger on sync loop) | None             | JACK transport / MIDI beats | ? |

(1): ShoopDaLoop has built-in support to host Carla through LV2, relying on Carla as a proxy to support other plugin types such as VST(3).<br>
(2): Focus is on Linux for now until it is reasonably feature-complete. Windows and Mac builds already available, but not useful yet due to lacking audio and plugin support.<br>
(3): ShoopDaLoop plug-in scripts are written in LUA. Currently the main goal is to support deep MIDI controller integrations and custom keyboard control scripts and opening/managing additional MIDI control ports. Future goals could be integration with the future song/performance sequencer or integration with the outside world by e.g. network.<br>
(4): ShooDaLoop supports "composite loops", allowing you to combine sequences of loops into other loops hierarchically. Through this method, complex sequences and simple songs can be constructed.<br>
(5): See (4): composite loops can be used as scenes.<br>
(6): Currently this can be used to control ShoopDaLoop. Mapping to plugin parameters is planned for the future.

Disclaimers:

- I may have mistakes in this table due to not being completely familiar with these programs. Please raise an issue/PR for corrections or extra products to compare.
- Also note that I filled this in primarily as a comparison of FOSS loopers for Linux. Software like Ardour offers way more other DAW functionality that neither ShoopDaLoop nor the other loopers have.

As seen in the comparison table, ShoopDaLoop is closest to Luppp in what it offers, the main differentiators being built-in MIDI support and the planned features for song construction. And of course, the details of how loops are managed exactly differ between all these tools and are a matter of preference.

# Installation

See the Releases page and [INSTALL](INSTALL.md) for details.

# License / Copyright

Other than Git submodules and files which explicitly mention a different copyright owner, copyright owner for all files in this repo is Sander Vocke (2023).
For copying, see LICENSE.txt.

# Credits

This project is only made possible due to many libraries and tools, including but not limited to:
   
   - Qt and Qt for Python;
   - JACK audio;
   - mido;
   - numpy;
   - scipy;
   - soundfile;
   - (lib-)samplerate;
   - boost::ut;
   - qoverage, coverage for QML / Python code coverage, resp.;
   - many others (see submodules and dependencies)
