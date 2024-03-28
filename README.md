![Logo](./src/shoopdaloop/resources/logo-small.png)

[![codecov](https://codecov.io/github/SanderVocke/shoopdaloop/graph/badge.svg?token=15RLMBAYV7)](https://codecov.io/github/SanderVocke/shoopdaloop)
![build](https://github.com/sandervocke/shoopdaloop/actions/workflows/build_and_test.yml/badge.svg)
[![Docs](https://github.com/SanderVocke/shoopdaloop/actions/workflows/docs.yml/badge.svg)](https://github.com/SanderVocke/shoopdaloop/actions/workflows/docs.yml)

# ShoopDaLoop - Limitless Live Looping

ShoopDaLoop is an cross-platform advanced live looping application. It offers intuitive looping of audio+MIDI clips in a grid-style layout and some DAW-like features for sequencing.

It is designed for both free-form jamming (solo or together) and pre-scripted looping sessions. In principle, live performance is a good use-case, although it is not battle-tested and stable enough yet to recommend that at this point.

[Documentation](https://sandervocke.github.io/shoopdaloop/) lives here.

> :warning: **There is no release yet. This is in pre-alpha. All there is to see on this repo is a preview of things to come.**

For currently open known issues, check the GitHub Issues page.

# Screenshot

![Screenshot](docs/source/resources/screenshot.png)

# In a nutshell

- **Fast**: can easily handle a large number of loops.
- **Tracks**: loops are organized into tracks, which share inputs/outputs and effects/synthesis.
- **MIDI and audio**: can both be looped, including alongside each other in the same loop.
- **FX/synthesis**: can be inserted into a loop via external JACK connections or by using plugins. The same loop can simultaneously record "dry" and "wet", akin to [Luppp](http://openavproductions.com/luppp/), to save precious CPU during playback.
- **Synchronization**: every loop is synced to the "sync loop", which typically holds a beat, click-track or just fixed-length silence. Loops may also be a multiple of the sync loop length.
- **Click tracks**: can be generated via a dialog in the app.
- **NSM**: Non/New Session Manager support on Linux (experimental).
- **Hackable**: Users can run their own Lua scripts to control the application in new ways.

These features are explained in detail in the [docs](https://sandervocke.github.io/shoopdaloop/).

# Status

ShoopDaLoop is in early development. The basics work but not nearly all of its intended functionality is finished yet (see below).
The intent is for the master branch to always work and pass CI tests, but at the moment, some significant bugs will probably come and go. Check the issues list for currently known issues and please add any you encounter yourself.
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
| Scenes support              | ✅ (5) | ❌              | ✅ (grid row = scene)    | ?                      |
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

# Roadmap

All the basic features needed for a 1.0 release are there. The following items are planned before a first release:

- **Polishing**: Many features are there but some need to be tweaked and improved.
- **Distribution**: Currently the build process is quite convoluted and error-prone. This will be improved and tested for several repos.
- **Bugfixing**: Fix the most critical bugs in the list. Improve test coverage.

# Installation

There is no official release. To test-drive ShoopDaLoop, you can choose between building from source or grabbing an (unsupported) pre-release build for your platform. See [INSTALL](INSTALL.md) for details.

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
