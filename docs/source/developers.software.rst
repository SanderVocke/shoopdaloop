Software design
---------------

Architecture
~~~~~~~~~~~~

ShoopDaLoop is a Rust workspace with one application composition root:
``shoopdaloop``.

``shoop_egui``
  Presentation widgets consume immutable API snapshots and emit typed intents.
  It does not own audio drivers or session persistence.

``shoop_app_api`` and ``shoop_app``
  Stable application values plus the actor/cooperative runtime that validates
  intents, owns model state, and publishes revisions.

``shoop_backend`` and ``shoop_engine``
  Backend adaptation, native JACK/CPAL+midir/dummy drivers, graph scheduling,
  realtime audio/MIDI processing, processors, and bounded state publication.
  Native driver composition uses ``shoop_engine/app_backend``; the browser uses
  a dedicated AudioWorklet protocol.

``shoop_session`` and ``shoop_settings``
  Versioned session/media codecs, deterministic resampling, typed settings,
  native atomic storage, and browser storage values.

``shoop_scripting``
  The omniLua runtime, control API, bundled sources, callbacks/timers, and
  logical MIDI service.

``shoop_audio_protocol`` and ``shoop_audio_worklet``
  Bounded browser control/audio/MIDI messages and the realtime Web Audio engine.

Realtime ownership
~~~~~~~~~~~~~~~~~~

Timing-authoritative state machines run in the engine. UI refreshes only observe
published state. Topology and content replacements are prepared off the audio
thread and committed through bounded callback-boundary operations. Realtime
allocation and lock guards cover engine and AudioWorklet paths.

Carla hosting
~~~~~~~~~~~~~

Carla processors implement one frontend-independent processor contract. A
pinned ``libcarla_native-plugin`` is loaded by absolute path and Rack/Patchbay
descriptors are instantiated through ``CarlaNative.h``; ShoopDaLoop does not
host Carla through LV2 or another plugin wrapper. In-process hosting owns Carla
on a non-realtime bridge thread. Subprocess mode gives each chain an
authenticated worker generation and bounded shared-memory block transport. The
same executable dispatches hidden worker mode before creating the GUI.

Build and packaging
~~~~~~~~~~~~~~~~~~~

Cargo builds the native workspace. Trunk builds the browser UI and dedicated
AudioWorklet with matching profiles. The application artifact script emits
unsigned native archives, a hosted web archive, and a self-contained HTML file.
Native archives include a manifest- and checksum-verified Carla runtime
component with UI/discovery/bridge helpers, licenses, and corresponding-source
metadata. Browser artifacts contain none of that component.

Testing
~~~~~~~

The main gates are::

  cargo fmt --all -- --check
  RUSTFLAGS="-D warnings" cargo build --workspace
  SHOOP_ALLOW_MISSING_BACKENDS=1 \
    cargo nextest run --workspace --features shoop_engine/app_backend --profile ci
  python3 scripts/check_tracing_coverage.py --require-closed

Web verification additionally builds both Wasm packages, checks browser
dependency isolation, verifies hosted/self-contained artifacts, and runs Chrome
and Firefox workflows where available. The GitHub workflow is authoritative for
Linux, Windows, macOS, and browser release surfaces.
