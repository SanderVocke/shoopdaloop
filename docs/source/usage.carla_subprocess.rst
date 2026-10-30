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
