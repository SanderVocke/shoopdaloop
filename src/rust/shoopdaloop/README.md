# ShoopDaLoop application

This is the shared native and browser application composition root.

- Native builds use the existing threaded JACK, CPAL+midir, or dummy/offline engine backend selected from persistent settings and provide actor-owned Lua scripting, keyboard control, and script-created native MIDI control ports.
- Browser builds use a repository-owned Web Audio/AudioWorklet backend after an explicit microphone or output-only enable action and run the same omniLua-backed scripting manager cooperatively on the application owner.
- A separate explicit Web MIDI action discovers physical browser endpoints. One bounded main-thread hub serves direct-track MIDI recording/playback and Lua-created control ports while the AudioWorklet retains track-route truth.

## Native

From the repository root:

```sh
# Native drivers with LV2/Carla FX hosting (default).
cargo run -p shoopdaloop

# Native drivers without LV2/Carla FX dependencies.
cargo run -p shoopdaloop --no-default-features
```

Live profiling requires a Tracy 0.13.1 profiler:

```sh
cargo run -p shoopdaloop -- --tracing
```

To write a capture, install the Tracy 0.13.1 `tracy-capture` executable on `PATH` or select it with `TRACY_CAPTURE_TOOL`. Captures are written below `./traces` and finalized after the application exits normally:

```sh
cargo run -p shoopdaloop -- \
  --tracing-capture \
  --tracing-engine-detail
```

`--tracing-engine-detail` requires either `--tracing` or `--tracing-capture` and increases callback overhead and capture volume.

On first run this starts the dummy/offline engine. Open **Settings** and select **Audio** to configure every driver family supported by the build and currently discovered JACK/CPAL devices, then use **Switch** for a confirmation-gated runtime change. The warning identifies the resolved source and target rates; a changed rate explicitly resamples all loop audio, exact MIDI, lengths, offsets, preplay, ring-buffer durations, and cycle timing through the session resampler. Successful switches are saved for the next launch, while unavailable saved drivers fall back to dummy with a diagnostic without overwriting the preference. Native MIDI controller discovery uses the host MIDI service. Select **Scripts** to manage the embedded keyboard/APC scripts or path-based user scripts. This is the only script-management dialog. ``keyboard.lua`` is enabled on first run; bundled toggles and ordered user path/enabled entries are preserved in the application settings document after **Save**. Runtime-only Stop, Restart, and Reload controls plus lifecycle, documentation, logs, callbacks/timers, MIDI connections, dropped messages, and failures are visible in the same tab.

The **Add Track** dialog offers **Regular** and **Dry + Wet**. Native capabilities always advertise External and **Tiny Synth/FX**; builds with `native-fx` also advertise Carla Rack, Patchbay, and Patchbay 16x. External and Carla retain independent dry/wet audio counts and optional dry MIDI. Tiny Synth/FX enforces equal audio counts and one MIDI input, including MIDI-only zero-audio tracks. External tracks expose dry input/send and wet return/output ports in **Connections**; hosted processors keep FX endpoints internal and expose dry inputs, wet outputs, and dry MIDI. Processed track headers show only capabilities the descriptor advertises. Tiny Synth/FX opens an embedded editor with runtime-discovered presets, Panic, smoothed master gain, reverb, and distortion; it never creates a native child window. Loop playback can use recorded wet content or route recorded dry content through the processor, and wet recordings retain compatible restorable processor state.

Bundled Lua sources are compiled into the native binary, so packaged startup does not depend on the source checkout. User-file reads and settings writes stay in this composition root. Source-bearing session scripts are staged before transactional session commit and round-trip in ``.shoop`` files without embedding machine-wide paths.

The Shoop Lua API is versioned at major/minor ``1.0``. Every script must begin its Shoop API use with ``shoop_announce_api_version(1, 0)``; equal-major scripts with an equal or older minor run, while different majors, newer minors, missing calls, and malformed/repeated calls are cancelled before versioned side effects. Scripts may require ``shoop_dialog`` to create any number of named simple or paged windows containing portable rich text and optional-callback buttons. The top bar lists every active definition with a count. Scripts can request opening at startup or from callbacks; users control closing, reopening, and page selection until the owning script stops. See ``../../../docs/lua_dialog_api.md``.

## Application settings

Choose **Settings** from the main menu to edit application-wide preferences. The Audio tab retains an independent configuration for dummy, JACK, and CPAL+midir; ordinary Save stores profiles without changing the running backend, while Switch preflights, confirms, transactionally restores the session, and only then updates the preferred startup driver. In `native-fx` builds, the **Carla** tab selects `in_process` (default) or one supervised `subprocess` per chain. This global machine preference is validated and applied before native backend construction on the next launch; Save does not migrate running chains, and the value is never session data. The packaged executable also serves as its hidden Carla worker before GUI startup.

The track defaults control the audio channel count and MIDI state used the next time the Add Track dialog opens. They do not alter existing tracks, an Add Track draft that is already open, or `.shoop` session data. The dialog keeps edits in a draft until **Save**; **Cancel** or closing it discards the draft, and reset actions restore registered defaults.

Native builds store application settings in `settings.json` below the OS configuration directory resolved using the retained `org` / `ShoopDaLoop` / `ShoopDaLoop egui` compatibility identity. The dialog displays the authoritative resolved path. Browser builds use origin-scoped `localStorage` key `org.shoopdaloop.egui.settings`; direct-file persistence is browser-policy-dependent and must not be assumed to carry across URLs or origins.

Missing settings use stereo/MIDI-off defaults. Invalid known values use their defaults with a warning. Malformed, unreadable, or unsupported-version documents are not overwritten automatically; use the explicit replacement action after reviewing the diagnostic. The application never imports predecessor settings formats. See `../../../docs/settings_format_v1.md` for the format, locations, migration boundary, and recovery contract.

## Hosted browser audio

Install the Rust target and [Trunk](https://trunkrs.dev/):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
```

Serve the application from localhost:

```sh
cd src/rust/shoopdaloop
trunk serve --open
```

For reliable browser behavior, use HTTPS or `localhost`, which browsers treat as a secure context. The browser permissions dialog opens at app startup, reports the current audio, microphone, and Web MIDI permission state, and can be reopened from **Settings → Audio**. Click **Enable microphone audio** to create one `AudioContext`, request the default microphone, and configure microphone and destination channels as visible host ports around the AudioWorklet. Click **Enable output-only audio** to skip microphone capture; destination host ports remain available while capture inventory is empty, and microphone access can be requested later from the dialog. Click **Enable Web MIDI + SysEx** independently to request MIDI and SysEx access. Permission denial, unsupported APIs, and driver failure leave the application responsive and expose truthful retry or unavailable state.

The self-contained HTML embeds the application and AudioWorklet Wasm modules plus the worklet script. It may be opened directly through `file:` and attempts both output-only and microphone modes without rejecting the URL. Browser security and media-permission behavior for local files varies, so HTTPS or `localhost` remains the portable option. Both physical-audio modes still require an explicit click because of browser autoplay policies.

The browser requests echo cancellation, noise suppression, and automatic gain control off, but the browser may negotiate different settings. The engine runs at the context's actual sample rate and render quantum. The Connections dialog shows normalized application ports separately from negotiated `webaudio:capture_N`, `webaudio:destination_N`, and stable `webmidi:source|sink:<MIDIPort.id>` host ports. Audio and track-MIDI connection commands mutate authoritative worklet routes. Web MIDI input is assigned to frame zero of the next available quantum and output preserves engine order with browser scheduling latency; sample-exact timing is not claimed. All routed audio tracks sum with final clipping to `[-1, 1]`; input monitoring defaults off. See `../../../docs/port_model.md` and `../../../docs/web_midi_contract.md`.

The browser renders the same Regular/Dry + Wet form and advertises **Tiny Synth/FX** from the AudioWorklet-backed processor catalog. It uses the same matched-channel/required-MIDI contract and embedded editor as native builds, while arbitrary internal channel counts remain independent of the physical Web Audio host boundary. Capture/destination ports are not presented as an External processor. Loading External, Carla, or another unavailable processor fails capability validation before worklet mutation and retains tracks, media, routes, and callback progress. Tiny Synth/FX sessions and compatible recorded-take state transfer unchanged between browser and native builds.

Browser recording storage is hard-bounded per channel to 120 seconds at the actual sample rate. Its full reserve is prepared on the worklet control path when a loop is armed for recording, so dormant loop slots do not exhaust Wasm memory. Exhaustion stops further channel recording work and is reported in diagnostics instead of growing Wasm memory in the render callback. Unexpected render-time memory growth is reported as a warning and the worklet rebinds its audio views without stopping the backend.

The browser application embeds omniLua, Shoop's Lua modules, `keyboard.lua`, and the APC Mini script. Keyboard control is enabled by default and receives GUI press/release events independently of audio permission. The APC script is embedded but disabled by default; after Web MIDI access it autoconnects matching physical endpoints through the same bounded control contract as native. The Scripts settings tab persists those bundled toggles in `localStorage` and reconciles runtime state only after a successful save; path-based startup scripts remain native-only. Native and browser builds can load a UTF-8 `.lua` file through the run-once picker or OS drag and drop after explicit confirmation. Native Wayland drag and drop is unavailable until winit ships Wayland file-drop support; the picker remains available. Run-once sources remain restartable in memory, are independent of session replacement and serialization, and disappear at app shutdown. Loading another version with the same source name stops active matching versions and retains each version under a unique display suffix. Source-bearing `.shoop` scripts use the same syntax-check/transaction/save path as native builds. API-version rejection and `shoop_dialog` definitions/opening/button callbacks use the same application-owner implementation and ordinary application windows as native; no browser popup API is used.

## Loop details and on-screen MIDI piano

The bottom bar has **details** and **piano** buttons selecting one resizable bottom pane. For one selected primitive loop, **details** shows existing audio waveforms followed by a read-only piano-roll lane for each MIDI channel. MIDI lanes display note pitch, timing, duration, loop region, and playback position; their zoom controls and horizontal dragging change only the view. MIDI-only and mixed loops are both supported on native and browser builds. Non-note controller, bend, pressure, program, and SysEx messages remain preserved but are not drawn by this basic view.

The piano covers MIDI notes 0–127 in a horizontally scrollable keyboard, initially centered on middle C (MIDI 60/C4); every C key carries its scientific-pitch octave marking from C-1 through C9.

Pointer press/release sends channel-1 note-on at velocity 100 and zero-velocity note-off. The application fans each press out once to every track whose input monitoring is enabled (input mute is off) and which owns a MIDI input port. Releases follow the tracks that received the press, including when monitoring changes while a note is held; pane close/switch and pointer/focus cancellation release held notes. The pane shows the current destination names or a no-target message.

Piano messages enter ordinary track MIDI input processing at frame zero of the next available engine process iteration. This soft timing is not sample-exact, but the path is driver-independent: native dummy/offline, JACK, and CPAL+midir need no physical MIDI source, while Web Audio and browser offline mode need no Web MIDI permission or host route. The piano creates no host endpoint and changes no connection or session setting.

## Session and loop files

The main menu saves and loads fresh `.shoop` v1 sessions. Loop context menus import/export exact `.shoop-audio` and `.shoop-midi`, float WAV, and standard MIDI. Audio import requires explicit destination mapping; audio export presents an ordered channel selection. Direct, dry, and wet role labels are preserved, including dry-only, wet-only, mixed, and reordered exports; exact/standard MIDI targets dry MIDI. Different-rate assets require confirmation before deterministic audio/MIDI/timing conversion. Predecessor session/media formats are deliberately unsupported.

Right-click a primitive loop and choose **Generate click track...** to create embedded audio or MIDI click content with primary/secondary sounds, fractional tempo, click count, odd-click delay, and loop-length fitting. Audio Preview is non-mutating. Native preview uses the default system playback output on a bounded worker and is disabled with an explanation when no default output is available. Hosted browser preview uses the running Web Audio context; explicit offline mode creates a bounded fallback context subject to browser autoplay policy. Generated content persists as ordinary loop media without storing the dialog draft.

Native picker reads and atomic temporary-file replacement run outside the application actor. Browser pickers use asynchronous upload/download file handles; ordinary hosted and direct-file artifacts do not require the File System Access API. Session/media and generated preview bytes stay outside immutable GUI snapshots. See `../../../docs/session_format_v1.md` for formats, limits, timing, and recovery behavior.

## Builds and artifacts

Trunk builds the UI and dedicated worklet with matching profiles:

```sh
cd src/rust/shoopdaloop
trunk build                 # debug UI and worklet
trunk build --release       # release UI and worklet
python3 build_single_file_app.py dist
```

CI application archives can also be produced locally from already-built outputs:

```sh
# From the repository root after a native debug build.
python3 src/rust/shoopdaloop/package_artifacts.py native \
  --platform linux --arch x86_64 --profile debug \
  --binary target/debug/shoopdaloop --output-dir artifacts

# From src/rust/shoopdaloop after a Trunk debug build.
python3 package_artifacts.py web \
  --profile debug --dist dist --output-dir ../../../artifacts
```

Native CI outputs are unsigned application archives rather than installers or portable dependency-closure packages. The hosted web archive supports physical browser audio and contains the complete UI and AudioWorklet assets. The separately generated profile-named HTML embeds those assets and attempts physical output or microphone audio when directly opened from `file:`. Open it with `?offline=1` to explicitly select the elapsed-time dummy engine instead.

Generated `dist`, worklet, staging, and artifact files are not committed.

## Cross-target CI

`.github/workflows/build_and_test.yml` has one eight-cell matrix: Linux x86_64, Windows x86_64, macOS arm64, and WebAssembly, each in debug and release. Every cell builds, packages, uploads, and then tests. Native cells upload unsigned application archives; web cells upload a hosted bundle archive and a separately downloadable self-contained HTML file. The matrix has no coverage flavor yet.

For fast workflow iteration with `nektos/act` 0.2.89 or newer, run the Linux and web debug cells on a suitable self-hosted development environment:

```sh
act pull_request -W .github/workflows/build_and_test.yml \
  -j build_and_test --matrix target:linux --matrix profile:debug \
  -P ubuntu-24.04=-self-hosted --artifact-server-path .act/artifacts

act pull_request -W .github/workflows/build_and_test.yml \
  -j build_and_test --matrix target:web --matrix profile:debug \
  -P ubuntu-24.04=-self-hosted --artifact-server-path .act/artifacts
```

The web command needs Trunk 0.21.14 and `wasm32-unknown-unknown`; the local workflow skips GitHub cache/upload actions and browser/device automation while still building, packaging, validating, testing, and checking dependency isolation. GitHub-hosted runners remain authoritative for uploads, caches, Chrome/Firefox, Windows, and macOS.

## Browser verification

After building, run Chrome/Chromium with a deterministic generated fake microphone:

```sh
node --experimental-websocket browser_smoke.mjs
WEB_MIDI=1 node --experimental-websocket browser_smoke.mjs
WEB_MIDI=1 WEB_MIDI_DENY_FIRST=1 node --experimental-websocket browser_smoke.mjs
BROWSER_SIZE=360,200 node --experimental-websocket browser_smoke.mjs
DENY_FIRST=1 node --experimental-websocket browser_smoke.mjs
LIFECYCLE=1 node --experimental-websocket browser_smoke.mjs
SATURATE=1 node --experimental-websocket browser_smoke.mjs
STRESS=1 node --experimental-websocket browser_smoke.mjs
OUTPUT_ONLY=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 OUTPUT_ONLY=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 DIRECT_FILE_MIC=1 node --experimental-websocket browser_smoke.mjs
SETTINGS_ONLY=1 node --experimental-websocket browser_smoke.mjs
SETTINGS_ONLY=1 SETTINGS_UNAVAILABLE=1 node --experimental-websocket browser_smoke.mjs
SELF_CONTAINED=1 SETTINGS_ONLY=1 node --experimental-websocket browser_smoke.mjs
xvfb-run -a python3 browser_firefox_smoke.py
STRESS=1 xvfb-run -a python3 browser_firefox_smoke.py
```

The Firefox command also requires Selenium and geckodriver. Set `CHROME_BIN` or `FIREFOX_BIN` when browser executables use non-standard names. The ordinary hosted tests open and paint the Dry + Wet form with the cross-target Tiny Synth/FX processor catalog, open the global connection surface, prove real audio route mutation, record/playback, session replacement, transactional External/Carla rejection with retained media/callback progress, source-bearing Lua, and keyboard control. `WEB_MIDI=1` installs a deterministic browser API before startup while retaining the production adapter: it proves explicit SysEx permission, canonical endpoint publication, user-managed track and owner-managed APC links, exact input recording/control fanout, playback/control output, refusal and saturation counters, hotplug reconnect, worklet restart, and continuing callbacks. Its denial mode proves retry. Stress and lifecycle modes retain the audio/storage gates. Settings modes still cover APC's healthy zero-host state before MIDI permission, Scripts UI, persistence failures, and version rejection. Hosted and self-contained workflows execute the same production assets without a query-selected fake backend.

Compiler-only checks from the repository root:

```sh
cargo check -p shoopdaloop --no-default-features --target wasm32-unknown-unknown
cargo build -p shoop_audio_worklet --target wasm32-unknown-unknown --release
```
