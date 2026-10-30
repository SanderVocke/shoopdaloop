Carla process isolation
-----------------------

Native builds with FX support can run Carla Rack and Patchbay chains in the
application process or in one worker process per chain. Select the global mode
under **Settings → Carla**. It takes effect on the next launch and is not stored
in sessions.

A subprocess authenticates its control connection and uses bounded shared
memory for realtime audio and MIDI blocks. A late or failed worker produces a
bounded failure for that wet block instead of delaying the audio callback.
Other tracks continue. Recovery starts a new worker generation and restores the
last confirmed state and desired active state.

Completion waits sleep rather than spinning for an entire buffer. Hardware
callbacks share one absolute deadline across all Carla chains and nested worker
waits. UI and state mutations quiesce the affected processor; its wet output is
silent while control owns it, without making realtime DSP wait for a control
mutex.

Worker startup verifies scheduling before publishing the realtime endpoint.
Where a backend exposes its callback priority it is used for startup preflight;
otherwise the actual callback supplies the scheduling context on first use.
Linux DSP workers run below their synchronous callers so a callback deadline can
preempt an overrunning plugin. A dedicated bounded budget monitor demotes a
CPU-overrunning DSP worker so it cannot starve healthy workers. Windows uses
scoped Pro Audio MMCSS and macOS inherits the callback's Mach time-constraint
class. Scheduling denial or exhausted priority range is reported explicitly and
uses ordinary scheduling with the same bounded sleeping completion wait.

These mechanisms bound audio handoff failures, not arbitrary plugin code.
Subprocess recovery can terminate a stuck worker. In-process hosting cannot
safely destroy a plugin that never returns, and does not isolate native crashes.
OS scheduling and timer resolution still affect latency.

Processed-track controls expose lifecycle and recovery state. **Carla Process
Logs...** shows bounded stdout and stderr records per generation, including any
dropped-byte count. Closing a plugin UI or unloading a session is normal
shutdown, not a crash.

Release archives include a pinned Carla Native runtime, external UI, and plugin
discovery/bridge helpers. ShoopDaLoop loads this runtime directly rather than
hosting Carla through LV2. Source builds need no Carla SDK; when no runtime is
present the Carla processors are shown as unavailable without affecting External
or Built-in Synth tracks. Developers can use the absolute-path overrides
``SHOOP_CARLA_NATIVE_LIBRARY`` and ``SHOOP_CARLA_RESOURCE_DIR`` to select an
exact runtime and ``--probe-carla-native`` to validate it. ``--probe-carla-native-ui`` additionally
opens, idles, hides, and reopens every external Carla UI before exiting.
