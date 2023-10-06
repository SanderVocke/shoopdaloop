![Logo](./src/shoopdaloop/resources/logo-small.png)

[![codecov](https://codecov.io/github/SanderVocke/shoopdaloop/graph/badge.svg?token=15RLMBAYV7)](https://codecov.io/github/SanderVocke/shoopdaloop)
![build](https://github.com/sandervocke/shoopdaloop/actions/workflows/build_and_test.yml/badge.svg)

# ShoopDaLoop

ShoopDaLoop is a live looping application for Linux with a few DAW-like features. [Documentation](https://sandervocke.github.io/shoopdaloop/) lives here.

> :warning: **There is no release yet. This is in pre-alpha. All there is to see on this repo is a preview of things to come.**

The main intended use is for quickly expressing musical ideas without needing to record full end-to-end tracks for a whole song. It makes for a fun way to jam by oneself.

Live performance could also be a good use-case, although it is not quite battle-tested enough to recommend that.

For currently open known issues, check the GitHub Issues page.

# In a nutshell

To summarize why ShoopDaLoop exists and what the goals and plans are, a short comparison table with similar software gives a good picture:

|                             | ShoopDaLoop               | SooperLooper     | Luppp                  | Ardour                   |
|-----------------------------|---------------------------|------------------|------------------------|--------------------------|
| OS                          | Linux <sup>(2)</sup>      | Linux, Mac       | Linux                    | Linux, Mac, Windows    |
| MIDI looping                | ✅                        | ❌              | ❌                       | ✅                      |
| Audio+MIDI co-recording     | ✅                        | ❌              | ❌                       | ?                      |
| Audio dry+wet co-recording  | ✅                        | manual setup     | ✅                      | ?                      |
| Loop Organization           | Grid                      | Separate loopers | Grid                     | Grid                   |
| Scenes support              | ✅ (any loop combination) | ❌              | ✅ (grid row = scene)    | ?                      |
| Suitable for live use       | ✅                        | ✅               | ✅                      | ❌                      |
| Plugin Host                 | ✅ <sup>(1)</sup>         | ❌               | ❌                      | ✅                      |
| Song/performance sequencing | planned                   | ❌               | ❌                      | ✅                      |
| MIDI controller support     | planned (LUA scripting)   | ✅ (MIDI learn)  | ✅ (not sure of method) | ✅ (not sure of method) |
| NSM Session Management      | ✅                        | ✅               | ✅                      | ✅                      |
| Overdubbing                 | planned                   | ✅               | ✅                      | ?                      |

(1): ShoopDaLoop has built-in support to host Carla through LV2, relying on Carla as a proxy to support other plugin types such as VST(3). <br>
(2): Focus is on Linux for now until it is reasonably feature-complete. The design does not prevent moving to Mac + Windows in the future.

Disclaimers:

- I may have mistakes in this table due to not being completely familiar with these programs. Please raise an issue/PR for corrections or extra products to compare.
- Also note that I filled this in primarily as a comparison of FOSS loopers for Linux. Software like Ardour offers way more other DAW functionality that neither ShoopDaLoop nor the other loopers have.

As seen in the comparison table, ShoopDaLoop is closest to Luppp in what it offers, the main differentiators being built-in MIDI support and the planned features for song construction. And of course, the details of how loops are managed exactly differ between all these tools and are a matter of preference.

# Summary

- **Fast**: can easily handle a large number of loops.
- **Tracks**: loops are organized into tracks, which share inputs/outputs and effects/synthesis.
- **MIDI and audio**: can both be looped, including alongside each other in the same loop.
- **FX/synthesis**: can be inserted into a loop via external JACK connections or by using plugins. The same loop can simultaneously record "dry" and "wet", akin to [Luppp](http://openavproductions.com/luppp/), to save precious CPU during playback.
- **Scenes**: scenes are named groups of loops. They can be started/stopped simultaneously. Typical use is for sections of a song.
- **Synchronization**: every loop is synced to the "master loop", which typically holds a beat, click-track or just fixed-length silence. Loops may also be a multiple of the master loop length.
- **Click tracks**: can be generated via a dialog in the app.
- **NSM**: Non/New Session Manager support (experimental).

These features are explained in detail in the [docs](https://sandervocke.github.io/shoopdaloop/).

# Status

ShoopDaLoop is in early development. The basics work but not nearly all of its intended functionality is finished yet (see below), and there are bugs.
As such, it obviously has not been used for on-stage performing and definitely shouldn't until after doing some serious testing.
Note however that having automated testing with high coverage is among the project goals.

# Roadmap

The following items are being worked on:

- **Scripting**: Integrate a LUA scripting engine so that users can add their own functionality. The first use-case for this is for handling MIDI controllers / MIDI learn.
- **Sequencing**: Add a means to sequence loop actions (e.g. to build a song or a pre-scripted performance session).

# Installation

See [INSTALL](INSTALL.md).

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
   - resampy;
   - boost::ut;
   - qoverage, coverage for QML / Python code coverage, resp.;
   - many others (see submodules and dependencies)
