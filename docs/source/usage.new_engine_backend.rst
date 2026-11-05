New engine backend
------------------

The RT session engine is ShoopDaLoop's production backend. It supports core
audio/MIDI looping, host-advertised dry/wet processors, session files and
client-side Lua. The feature parity ledger tracks remaining features.

Enabling it
~~~~~~~~~~~

Choose the audio driver, session delivery and prepared capacities in
**Settings → Audio** before starting a host. Existing hosts retain their
configuration when a UI attaches. ``--audio-backend new_engine`` remains a
compatibility option; the legacy backend is unavailable in production.

Drivers
~~~~~~~

Native builds support dummy, CPAL and native JACK drivers according to compiled
features. Native JACK publishes track and bus ports dynamically. Startup
failures and unavailable devices are reported by the host launch flow.

The new engine cannot switch drivers while it runs. Instead, the **Audio
driver** section of **Settings → Audio** offers the dummy and CPAL drivers and
saves their settings for the next start.

In the browser, the new engine runs in its own AudioWorklet and uses the same
permissions dialog as the legacy engine: **Enable microphone audio** or
**Enable output-only audio**, and optionally **Enable Web MIDI + SysEx**. The
application starts once audio is enabled and the engine is connected; until
then it shows only a notice. The engine's audio and MIDI ports are fixed when
audio is enabled:

* Enable Web MIDI before audio. Web MIDI ports allowed or connected later need
  a page reload.
* Output-only audio reserves two capture channels, so the microphone can be
  enabled later without restarting audio.

Capacities
~~~~~~~~~~

The new engine prepares its tracks, loops and loop memory at startup and does
not grow them while it runs. When **Audio backend** is set to the new engine,
**Settings → Audio** also shows these capacity settings. Changes apply after a
restart.

.. list-table::
   :header-rows: 1
   :widths: 25 12 12 51

   * - Setting
     - Default
     - Range
     - Meaning
   * - New engine tracks
     - 16
     - 2–256
     - Tracks, including the sync track.
   * - New engine loops
     - 128
     - 1–4096
     - Loop slots shared by all tracks. Each track usually takes eight.
   * - New engine track channels
     - 2
     - 1–8
     - Most audio channels of a track input or output.
   * - New engine loop memory
     - 256 MiB
     - 16–2047 MiB
     - Memory reserved for loop content. 256 MiB hold about 23 minutes of mono
       audio at 48 kHz.

The browser reserves at most 256 MiB of loop memory and reports a larger
setting in a notification.

Adding a track or loop beyond these capacities, or a track with more audio
channels, is refused with an error notification such as "all 16 prepared track
slots of the engine are in use". The session is left unchanged.

Supported features
~~~~~~~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Area
     - Supported
   * - Tracks
     - Adding and removing Regular tracks with up to the configured audio
       channels and optional MIDI, including MIDI-only tracks; the sync track;
       input gain, balance and monitoring; output gain, balance and mute; the
       on-screen MIDI piano.
   * - Loops
     - Adding loops to tracks; record, play and stop, immediately or after a
       number of sync cycles; clear; loop gain and balance; loop edge
       smoothing.
   * - Views
     - Loop mode, pending transition and its delay, length and position; track
       and loop meters; MIDI activity; waveform and MIDI loop details.
   * - Connections
     - Audio and MIDI connections between track ports and host ports; the
       named buses with gain, stereo balance and mute; per-channel routes from
       tracks to buses and buses to host outputs. Master is an ordinary removable
       default; zero buses is valid. The configured pool supports up to eight buses
       of at most eight channels each. **Global FX Control MIDI In** routes
       supported continuous controls to processed tracks without recording them.
   * - Sessions
     - New session and atomic save/load, including dry/wet role media, processor
       settings, MIDI mappings and recorded-take settings. Loaded loops stop.
   * - Track processors
     - External, built-in FunDSP FX and OxiSynth according to the host catalog.
       Embedded editors, MIDI Learn, activation and recorded-state restoration;
       dry-through-wet playback, wet rerecord and per-track default actions.
   * - Status
     - Sample rate, buffer size, driver state, xruns and load.

Everything else is unavailable. The UI disables or hides it with an
explanation, and a script operation or session that needs it is refused as a
whole instead of being applied partially:

* composite loops;
* grab;
* general replacing mode, Carla and bus FX;
* manual latency compensation and take-alignment controls;
* cloning loop content;
* switching the audio driver while running.

Loop content import/export, click generation, length, start-offset and pre-play
editing are supported within the advertised limits. Processor buffering is
automatic and reported separately from host buffering. Optional processor
features and ``auto``/``on``/``off`` startup policies control availability.

Known limitations
~~~~~~~~~~~~~~~~~

* Loop state in the UI is one audio callback behind the legacy engine.
  Transition timing and recorded content are the same, but a synchronized
  action requested within one callback of a sync loop wrap can take effect one
  cycle later.
* Backend, driver and capacity changes apply only at the next start.
* Natively, separate CPAL input and output devices are not resampled to each
  other; clock drift shows as xruns. MIDI output is sent right after each
  callback rather than placed within it.
* In the browser, Web MIDI ports added after audio starts need a page reload,
  the offline and worker modes are unavailable, and the application starts
  only once the engine is connected.
