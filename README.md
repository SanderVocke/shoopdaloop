![Logo](./src/shoopdaloop/resources/logo-small.png)

[![codecov](https://codecov.io/github/SanderVocke/shoopdaloop/graph/badge.svg?token=15RLMBAYV7)](https://codecov.io/github/SanderVocke/shoopdaloop)
![build](https://github.com/sandervocke/shoopdaloop/actions/workflows/build_and_test.yml/badge.svg)

# ShoopDaLoop

ShoopDaLoop is a live looping application for Linux with a few DAW-like features. [Documentation](https://sandervocke.github.io/shoopdaloop/) lives here.

> :warning: **There is no release yet. This is in pre-alpha. All there is to see on this repo is a preview of things to come.**

The main intended use is for quickly expressing musical ideas without needing to record full end-to-end tracks for a whole song. It makes for a fun way to jam by oneself.

Live performance could also be a good use-case, although it is not quite battle-tested enough to recommend that.

# In a nutshell

- **Fast**: can easily handle a large number of loops.
- **Tracks**: loops are organized into tracks, which share inputs/outputs and effects/synthesis.
- **MIDI and audio**: can both be looped, including alongside each other in the same loop.
- **FX/synthesis**: can be inserted into a loop via external JACK connections or by using plugins. The same loop can simultaneously record "dry" and "wet", akin to [Luppp](http://openavproductions.com/luppp/).
- **Scenes**: scenes are named groups of loops. They can be started/stopped simultaneously. Typical use is for sections of a song.
- **Synchronization**: every loop is synced to the "master loop", which typically holds a beat, click-track or just fixed-length silence. Loops may also be a multiple of the master loop length.
- **Click tracks**: can be generated via a dialog in the app.
- **NSM**: Non/New Session Manager support (experimental).

These features are explained in detail in the [docs](https://sandervocke.github.io/shoopdaloop/).

# Status

ShoopDaLoop works, but not nearly all of its intended functionality is finished yet (see below). GUI elements relating to these features may already be there but not working yet.
This is in early development. It has not been used for on-stage performing and probably shouldn't until after doing some serious testing.
I am not a performing musician, so for me rigorous testing does not have priority, but I do aim to accomodate and add automated testing.

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
