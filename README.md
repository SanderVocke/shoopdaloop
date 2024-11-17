![Logo](./src/shoopdaloop/resources/logo-small.png)

[![codecov](https://codecov.io/github/SanderVocke/shoopdaloop/graph/badge.svg?token=15RLMBAYV7)](https://codecov.io/github/SanderVocke/shoopdaloop)
![build](https://github.com/sandervocke/shoopdaloop/actions/workflows/build_and_test.yml/badge.svg)
[![Docs](https://github.com/SanderVocke/shoopdaloop/actions/workflows/docs.yml/badge.svg)](https://github.com/SanderVocke/shoopdaloop/actions/workflows/docs.yml)

# ShoopDaLoop - Limitless Looping

ShoopDaLoop is a cross-platform live looping application featuring a grid-based interface for audio and MIDI loops with DAW-like sequencing capabilities. Perfect for both free-form jamming and pre-scripted performances.

> :warning: **Current releases (<v1.0.0) are development previews. Feel free to test, but expect some instability.**

[Documentation](https://sandervocke.github.io/shoopdaloop/) | [Known Issues](https://github.com/SanderVocke/shoopdaloop/issues)

![Screenshot](docs/source/resources/screenshot.png)

## Key Features

- **Powerful Looping**: Fast, grid-based organization of audio and MIDI loops with simultaneous recording
- **Advanced Audio**: FX/synthesis plugins support, dry/wet recording, and JACK connectivity
- **Smart Workflow**: Sync-based loop timing, retroactive loop grabbing, and scene/sequence composition
- **Extensible**: Lua scripting support, NSM compatibility (Linux), and MIDI controller integration

See the [documentation](https://sandervocke.github.io/shoopdaloop/) for detailed feature explanations.

## Development Status

ShoopDaLoop is in active development with working core functionality. The project is undergoing a major architectural transition:

1. Current stack: Python (frontend) + C++ (backend) + QML (GUI)
2. Target stack: Pure Rust + QML (+ Lua for scripts)

This transition aims to resolve current challenges with:
- Object lifetime and performance issues (Python/PySide)
- Complex debugging through multiple layers
- Build system complexity
- Potential memory/threading issues in C++

## Feature Comparison

A comparison with similar open-source software to highlight ShoopDaLoop's unique features:

|                             | ShoopDaLoop                        | SooperLooper     | Luppp            | Ardour           |
|-----------------------------|-----------------------------------|------------------|------------------|------------------|
| OS Support                  | Linux, Mac, Windows¹              | Linux, Mac       | Linux           | Linux, Mac, Win  |
| MIDI + Audio Co-recording  | ✅                                | ❌              | ❌              | ✅              |
| Dry/Wet Recording          | ✅                                | Manual          | ✅              | ?               |
| Loop Organization          | Grid                              | Separate        | Grid            | Grid            |
| Live Performance Focus     | ✅                                | ✅              | ✅              | ❌              |
| Plugin Support             | ✅ (via Carla/LV2)                | ❌              | ❌              | ✅              |
| Sequencing                 | ✅ (Composite loops)              | ❌              | ❌              | ✅              |
| MIDI Control              | ✅ (Learn/Script)                 | ✅              | ✅              | ✅              |
| Scripting                 | ✅ (Lua)                          | ❌              | ❌              | ✅              |

¹ JACK audio backend required on Windows/Mac

Key Differentiators:
- Integrated MIDI+Audio workflow
- Powerful composite loop sequencing
- Extensive scripting capabilities
- Modern architecture (transitioning to Rust)

## Installation

See the [Releases](https://github.com/SanderVocke/shoopdaloop/releases) page and [INSTALL](INSTALL.md) for details.

## License / Copyright

Other than Git submodules and files which explicitly mention a different copyright owner, copyright owner for all files in this repo is Sander Vocke (2023).
For copying, see LICENSE.txt.

## Credits

This project is made possible by many libraries and tools, including:
- Qt and Qt for Python
- JACK audio
- mido, numpy, scipy
- soundfile, (lib-)samplerate
- boost::ut
- qoverage (QML/Python code coverage)
- And many others (see submodules and dependencies)
