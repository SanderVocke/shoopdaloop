# ShoopDaLoop application

One session service owns musical state and admitted work. The UI and its local Lua
runtime use a confirmed client projection in every deployment. The RT engine is
the sole production backend. Its current core looping capability set is smaller
than the old stack's; [the feature parity ledger](../../../docs/native_session_recovery/feature-parity.md)
tracks each temporary gap, reference implementation/test and restoration criterion.
Feature editors, document/configuration fields and reference behavior are retained
for upcoming parity work. Recovery is independent of disk persistence.

For remote native UI connections, see [trusted-LAN setup and the Cortex-A adapter contract](../../../docs/trusted-lan-networking.md).

## Native roles and delivery

```sh
cargo build -p shoopdaloop -p shoop_host
# Local service and engine: ordinary process-exit behavior.
cargo run -p shoopdaloop
# Launcher starts independent host/UI siblings, then exits.
target/debug/shoopdaloop --session-transport socket
# Independent host, without socket reattachment support.
target/debug/shoopdaloop --session-transport shared_memory
```

The default is `in_process`. **Settings → Audio → Session delivery** selects the
next launch's placement. `--engine-transport` remains a CLI alias. These settings
configure UI/service delivery; the host's RT engine connection stays internal.
The external host owns devices, the service and the existing RT adapter. Detaching
or killing its UI does not reset playback, recording or accepted operations.
Only socket delivery supports reattachment. No host is automatically restarted.

Start roles independently with a JSON host configuration (an empty object selects
the dummy driver and default pools):

```sh
printf '{}\n' > host.json
target/debug/shoop_host --config host.json
# Alternative application role: --host-only --host-config host.json
# Discover protected per-user registrations, then select one explicitly.
target/debug/shoopdaloop --list-hosts
target/debug/shoopdaloop --ui-only --host-file /path/from/list/host.json
```

Host-only also accepts `--registry`, `--endpoint 127.0.0.1:PORT`,
`--credential-file PATH` and `--name`. The application
wrapper uses `--host-registry` for the registry override. Credential files contain
exactly 32 random bytes and require owner-only permissions/ACLs. Registry entries
refer to credentials rather than embedding tokens; never put tokens in arguments.
Registrations carry data/management endpoints and local process metadata.
Authentication and protocol checks precede attachment; a second controller is
refused while the current owner remains healthy. Registrations are discovery hints.

Every connection obtains a fresh snapshot of the endpoint's current state. The
**Connect to endpoint** action uses the same flow after loss or host restart.
Previous pending requests are discarded and commands are not replayed. Configured
scripts remain visible but disabled; start them explicitly in Scripts. Connection
does not apply local preferences or initialize the host's session.

The Session menu's **Connected host settings…** panel queries the host's startup
settings, effective audio status and diagnostics. Local preference saves change
future local launch defaults, independently of the connected host.

For an external host, the Session menu and window/platform close offer:

- **Close UI only (keep host running)** releases control and exits this UI.
- **Close UI and host** waits for authenticated MIDI reset, render stop and cleanup.
- **Cancel** keeps the UI open.

After data loss, shutdown uses a separate authenticated management connection with
the last owner's generation. A replacement UI fences that authority. Failure is
reported with retry and UI-only options; an uncertain retry checks the same operation.
A host with no live UI owner can be closed explicitly with
`shoopdaloop --shutdown-host --host-file PATH`. This authenticates and acquires a new
shutdown generation; a live owner refuses it. It also manages a shared-memory host
after its single data connection is lost, without enabling shared-memory reattachment.
The UI never uses this claim as a fallback for stale-owner shutdown.
Product shutdown does not kill a PID. In-process mode ends with its process; browser
mode ends with its page. Shared-memory mode cannot reattach after UI exit.

## Drivers, controls and files

Default native builds include CPAL+midir. Without default features the RT host uses
the dummy driver; Lua controller MIDI remains client-local. Saved JACK preferences
map to CPAL's JACK host or default device with a diagnostic. Unusable device settings
fall back to dummy with a visible warning and do not overwrite the saved preference.
Driver and capacity changes apply at host startup. Live driver switching is a
tracked temporary gap, as are FX/synths, composite/grab/dry-wet
operations, timing/alignment edits, click generation/preview/insertion and loop duplication.
The authoritative capability checks refuse unsupported operations.

Direct audio/MIDI/trigger tracks, primitive loop controls, shared musical selection,
targets/policies, supported track/bus controls, routes/connections, piano MIDI and
bounded waveform/MIDI detail reads are available. Configured track/loop/channel/media
pools are prepared at startup; exhausted capacity produces a refusal.

Core session save/load and standalone WAV, loop-audio, exact-MIDI and Standard MIDI
import/export use the session service. Save while playing is supported; active or
scheduled recording refuses without stopping it. Load starts restored loops stopped
and asks before resampling or retiming to the prepared sample rate and quantum.
Live length-changing imports retime retained media and sync dependents in one
atomic family cut; other playback, recording and controls remain active. Source
files and the prepared device format are unchanged. Unsupported
FX/dry-wet/composite/script documents and buses exceeding prepared limits refuse as a whole. Audio mapping, channel
selection, optional import length changes and SMF loss warnings remain explicit.
Native output replaces its target atomically; browser completion means download
handoff. Limits and supported fields are in the
[persistence contract](../../../docs/native_session_recovery/persistence-contract.md).
Full feature parity remains tracked by the ledger.

The Scripts tab manages external built-ins, examples, native user files and run-once
Lua sources. Script source export is available. Lua reads confirmed state; a write
becomes visible on a subsequent confirmed pump. VM, timers, controller connections,
dialogs and file/resource handling stay in the UI. See [Lua dialogs](../../../docs/lua_dialog_api.md).
Session script ownership conversion/resource persistence remains in the parity ledger.

Source builds use a sibling `SHOOP_SRC_TREE` marker to locate built-ins. Native
packages omit it and carry built-ins beside the executable on Linux/Windows or in
`Contents/Resources` on macOS. `shoop_host` ships beside the UI executable, including
inside `Contents/MacOS`. Carla runtime payloads and old worker/worklet payloads are
not shipped; their sources/recipes and behavioral fixtures are preserved for parity.

## Runtime profiling

Settings → Developer provides a persisted runtime on/off control and a plain
Node / Avg / Worst / Stddev table, in microseconds. Every graph rebuild resets the
interval. Xruns automatically retain bounded recent timing while profiling is on;
Live and Latest xrun use the same table. See the [profiling guide](../../../docs/runtime_profiling.md)
for timing semantics, overhead measurements and browser clock availability.

## Output buses

The mixer supports ordinary named buses, persisted display order, gain/mute,
stereo balance and per-channel meters. The startup pool defaults to eight buses
with up to eight channels each; Audio settings configure the prepared count and
width for the next start. The add dialog uses the running host's negotiated limits.
Master is the default stereo bus and can be removed. Zero-bus sessions save/load
normally; New Session creates a fresh default even after every bus was removed.
Track defaults map matching widths one-to-one and mono to every destination
channel; absent/incompatible names leave a track unconnected. Explicit bus and
direct system routes coexist. Lua bus indices follow stable logical order rather
than mixer drag order, including after save/load. Bus FX remains unavailable.

## Browser

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
cargo install --locked wasm-pack --version 0.15.0
cd src/rust/shoopdaloop
trunk serve --open
```

Audio needs an explicit microphone or output-only enable gesture. Web MIDI has a
separate permission action. The app client, Lua and service pump cooperatively on
the main thread; only the prepared RT engine renders in the AudioWorklet. File
codecs run in a dedicated client Worker so decoding/encoding does not block that
service. `offline` disables the session engine; the retired `worker`
mode produces a diagnostic. Audio/MIDI access depends on browser secure-context and
permission policies. Physical port inventory is fixed at engine preparation; reload
when new MIDI endpoints need engine ports. Same-page UI recovery is not provided.

Hosted bundles carry external built-ins, the RT worklet and the client codec module.
Release packaging also embeds them in a single HTML file, subject to browser restrictions on `file:` URLs.

## Verification

```sh
cargo fmt --all -- --check
RUSTFLAGS='-D warnings' cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
python3 scripts/check_rt_engine_boundary.py
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/check_shoop_test_usage.py
python3 scripts/run_wasm_tests.py --runtime node --profile dev
```

The workspace suite includes the isolated `shoop_app_reference` and independent old
engine fixtures for parity evidence. `shoop_engine/app_backend` in that test command
is a reference-test feature, never a production frontend dependency.

A packaged, windowless canary exercises the actual companion, authenticated attach,
recorded audio/MIDI reads, socket reattachment, and confirmed host cleanup:

```sh
target/debug/shoopdaloop --probe-engine-transport --session-transport socket
target/debug/shoopdaloop --probe-engine-transport --session-transport shared_memory
# Separate large-file canaries; the transport canary retains its existing deadline.
target/debug/shoopdaloop --probe-persistence --session-transport socket
target/debug/shoopdaloop --probe-persistence --session-transport shared_memory
```

Browser core workflow after a Trunk build (Chromium and pinned Node 22.22.2):

```sh
NEW_ENGINE=1 CHROME_BIN=chromium node --experimental-websocket browser_smoke.mjs
# After release packaging:
NEW_ENGINE=1 SELF_CONTAINED=1 CHROME_BIN=chromium node --experimental-websocket browser_smoke.mjs
python3 browser_firefox_smoke.py --backend new_engine
# Additional >32 MiB file round-trip through the codec worker and AudioWorklet:
NEW_ENGINE=1 LARGE_MEDIA=1 CHROME_BIN=chromium node --experimental-websocket browser_smoke.mjs
```

`SELF_CONTAINED_URL` can point the self-contained smoke at an already served HTML
URL. Serve a directory containing only that HTML to test its embedded bundle
independently of hosted assets.

The file canaries compare exact PCM/MIDI after save/load/save, check stopped fresh
identities, and resume playback. Native file canaries use actual atomic output and
bounded file input. The socket case also abandons upload/capture and reconciles an
accepted load without replay. Browser canaries cancel the first output, retry by
clicking the actual download link, and require a browser-handoff result. The large
browser case has separate deadlines; normal canary workloads/deadlines are retained.

`--tracing` captures the local UI/control process in Perfetto format; optional
`--tracing-engine-detail` adds available local instrumentation. External hosts and
RT worklets do not become traced application-model realms through UI attachment.
The browser tracing smoke waits for more than 8,192 records from the local UI
capture. After 60 seconds it saves the available trace and reports a failure with
the observed record count.
Forced process termination cannot finalize an in-process trace.
