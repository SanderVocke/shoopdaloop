# ShoopDaLoop application

One session service owns musical state and admitted work. The UI and its local Lua
runtime use a confirmed client projection in every deployment. The RT engine is
the sole production backend. Its current core looping capability set is smaller
than the old stack's; [the feature parity ledger](../../../docs/native_session_recovery/feature-parity.md)
tracks each temporary gap, reference implementation/test and restoration criterion.
Feature editors, document/configuration fields and reference behavior are retained
for upcoming parity work. Recovery is independent of disk persistence.

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
`--credential-file PATH`, `--host-id`, `--session-id` and `--name`. The application
wrapper uses `--host-registry` for the registry override. Credential files contain
exactly 32 random bytes and require owner-only permissions/ACLs. Registry entries
refer to credentials rather than embedding tokens; never put tokens in arguments.
Explicit registrations carry data/management endpoints and host/session identities.
Authentication and identity verification precede attachment; a healthy owner is
refused without eviction. Registrations are discovery hints, not process authority.

Attachment shows progress, coherent snapshot synchronization, failures and a manual
**Reattach to this host** action. The Session menu can select a registered host.
Recovered scripts are visible but inactive because the previous Lua VM was lost;
start them explicitly in Scripts. Reattachment does not reload settings, scripts,
a session file, Master autoconnections or the sync track into the live service.

For an external host, the Session menu and window/platform close offer:

- **Close UI only (keep host running)** releases control and exits this UI.
- **Close UI and host** waits for authenticated MIDI reset, render stop and cleanup.
- **Cancel** keeps the UI open.

After data loss, shutdown uses a separate authenticated management connection with
the last owner's generation. A replacement UI fences that authority. Failure is
reported with retry and UI-only options; an uncertain retry checks the same operation.
Product shutdown does not kill a PID. In-process mode ends with its process; browser
mode ends with its page. Shared-memory mode cannot reattach after UI exit.

## Drivers, controls and files

Default native builds include CPAL+midir. Without default features the RT host uses
the dummy driver; Lua controller MIDI remains client-local. Saved JACK preferences
map to CPAL's JACK host or default device with a diagnostic. Unusable device settings
fall back to dummy with a visible warning and do not overwrite the saved preference.
Driver and capacity changes apply at host startup. Live driver switching is a
tracked temporary gap, as are FX/synths, additional buses, composite/grab/dry-wet
operations, timing/alignment edits, click previews and content save/load/import/export.
The authoritative capability checks refuse unsupported operations.

Direct audio/MIDI/trigger tracks, primitive loop controls, shared musical selection,
targets/policies, supported track/Master controls, routes/connections, piano MIDI and
bounded waveform/MIDI detail reads are available. Configured track/loop/channel/media
pools are prepared at startup; exhausted capacity produces a refusal.

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

## Browser

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
cd src/rust/shoopdaloop
trunk serve --open
```

Audio needs an explicit microphone or output-only enable gesture. Web MIDI has a
separate permission action. The app client, Lua and service pump cooperatively on
the main thread; only the prepared RT engine renders in the AudioWorklet. There is
no application Worker. `offline` disables the session engine; the retired `worker`
mode produces a diagnostic. Audio/MIDI access depends on browser secure-context and
permission policies. Physical port inventory is fixed at engine preparation; reload
when new MIDI endpoints need engine ports. Same-page UI recovery is not provided.

Hosted bundles carry external built-ins and the RT worklet. Release packaging also
embeds them in a single HTML file, subject to browser restrictions on `file:` URLs.

## Verification

```sh
cargo fmt --all -- --check
RUSTFLAGS='-D warnings' cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci
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
```

Browser core workflow after a Trunk build (Chromium and pinned Node 22.22.2):

```sh
NEW_ENGINE=1 CHROME_BIN=chromium node --experimental-websocket browser_smoke.mjs
# After release packaging:
NEW_ENGINE=1 SELF_CONTAINED=1 CHROME_BIN=chromium node --experimental-websocket browser_smoke.mjs
python3 browser_firefox_smoke.py --backend new_engine
```

`--tracing` captures the local UI/control process in Perfetto format; optional
`--tracing-engine-detail` adds available local instrumentation. External hosts and
RT worklets do not become traced application-model realms through UI attachment.
Forced process termination cannot finalize an in-process trace.
