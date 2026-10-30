New engine backend
------------------

ShoopDaLoop can run on a new audio engine instead of its legacy engine. The new
engine backend is opt-in and supports the core looping workflow only. The
legacy engine stays the default and supports every feature described in this
guide.

Enabling it
~~~~~~~~~~~

Open **Settings → Audio** and set **Audio backend** to **New engine (core
looping only)**. The backend is chosen at startup: save the settings and
restart ShoopDaLoop, or reload the page in the browser.

To choose a backend for one run without changing the setting, start
ShoopDaLoop with ``--audio-backend new_engine`` or ``--audio-backend legacy``.
In the browser, add the query parameter ``?audio-backend=new_engine`` or
``?audio-backend=legacy`` to the URL.

Drivers
~~~~~~~

Native builds run the new engine on the dummy driver or on CPAL, with MIDI
through midir. It uses the preferred audio driver and its settings from
**Settings → Audio**. The new engine has no JACK driver: with JACK preferred,
it uses CPAL's JACK host where available and CPAL's default devices otherwise.
If the configured driver fails to start, the new engine starts on the dummy
driver instead. These fallbacks, and settings that cannot be used as given,
such as a missing device or MIDI port, are shown as notifications at startup.

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
       Master bus with its gain, balance and mute; routes from tracks to
       Master. **Global FX Control MIDI In** can be connected but has no
       effect.
   * - Sessions
     - New session.
   * - Status
     - Sample rate, buffer size, driver state, xruns and load.

Everything else is unavailable. The UI disables or hides it with an
explanation, and a script operation or session that needs it is refused as a
whole instead of being applied partially:

* saving and loading sessions, and saving loop audio or MIDI;
* composite loops;
* grab;
* replacing, and the dry/wet modes;
* Dry + Wet tracks and their processors, and track and bus FX;
* latency compensation and take alignment;
* replacing loop content: imports, click-track generation and cloning;
* editing loop length, start offset and pre-play;
* buses other than Master;
* switching the audio driver while running.

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
