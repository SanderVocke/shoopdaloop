# Carla standalone A/B package for macOS Catalina

This package runs Carla independently of ShoopDaLoop. It contains complete
Carla and Carla-Control applications, the standalone engine, plugin bridge and
discovery executables, frozen Python/Qt runtimes, and required dependencies.
It targets **Intel x86_64, macOS 10.15 or newer**.

Unzip into a separate directory and open `Carla.app`. Keep your system Carla
installation for comparison. Individual Mach-O files are ad hoc signed; the legacy app bundles are unsigned
and not notarized; macOS may require opening the downloaded application through Finder's
Open command. No system Carla files or libraries need to be replaced.

For an A/B test, run the two installations separately. Use the same audio driver,
sample rate, buffer size, plugin binary, preset, routing and input. Record both
Carla versions and whether JUCE/native VST3 or plugin bridging is used. For the
reported case, start at 48 kHz and 1024 frames with Voyage Voyage. Compare sustained
DSP readings, spikes and xruns over repeated runs with similar CPU/power conditions.
Carla's embedded meter and JACK's global load meter have different scopes.

The macOS runtime bundled by ShoopDaLoop uses the checksummed upstream Carla 2.5.10
universal release, with OpenSSL/liblzma rebuilt for Catalina. This package applies
that same recipe to the complete applications. Carla itself is not recompiled;
the source/build settings of the upstream Carla binary are preserved. Intel slices
are retained, unused QtDesigner/QtHelp bindings are omitted, and frozen Qt library
references in Carla-Control are made application-relative. The native library's
code/data sections are checked against the normalized ShoopDaLoop component.
No DSP algorithm or floating-point policy is changed by this packaging.

`BUILD-INFO.json` records exact upstream/source checksums, rebuilt dependency
versions, the packaging commit, file hashes and the native-library comparison.
`runtime-lock.json` contains the corresponding source URLs and checksums. Carla's
GPL and rebuilt dependency licenses are included under `licenses/`.

The package is audited for every shipped Intel Mach-O's minimum OS and dependency
closure. Complete GUI startup and standalone dummy-driver audio-file → gain
processing are tested on the Intel macOS build runner, including after extraction
to another directory. These checks do not substitute for execution on a physical
Catalina installation or the reporter's audio interface; please report that result
alongside the A/B measurements. `standalone-probe.json` and `compatibility.txt`
contain the build-runner results.

Reproduction uses `.github/workflows/carla-catalina-standalone.yml`,
`scripts/build_macos_catalina_dependencies.sh`, `scripts/carla_runtime.py` and
`scripts/package_carla_catalina.py` from the packaging commit in `BUILD-INFO.json`.
The pinned original sources remain available at:

- Carla: https://github.com/falkTX/Carla/archive/refs/tags/v2.5.10.tar.gz
- OpenSSL: the version/URL/checksum in `runtime-lock.json` under `macos_catalina_dependencies`.
- XZ/liblzma: the version/URL/checksum in the same lock entry.
