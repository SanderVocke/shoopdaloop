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

Carla and its LV2 bundle must be installed and discoverable through the native
LV2 search path in either hosting mode.
