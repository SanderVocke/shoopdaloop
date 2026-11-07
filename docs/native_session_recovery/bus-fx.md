# Built-in bus FX

P08 covers Built-in FX on buses and global-control MIDI destinations. Carla,
external plugin editors/logs and plugin recovery are outside the user-approved
scope. This feature uses existing hosted DSP and does not depend on another
feature integration.

## Signal and resource contract

The graph sums routed track outputs and Master preview into a bus input stage,
passes that sum through the optional processor, and applies bus gain, balance,
mute, meters and output connections afterwards. Global controls connect directly
to each processor once; they never follow audio routes or enter loop capture.

Built-in FX shares the host's prepared instance pool with track FX, including its
candidate-bank and memory reservations. Both catalogs report aggregate usage.
Bus FX accepts matching input/output widths up to the negotiated FX channel limit
and bus channel limit. A host may support wider ordinary buses than its prepared
FX width; processor selection on those buses refuses. Defaults and storage
ceilings are unchanged. Enabling bus support does not enlarge every DSP slot.

Active processing adds the existing one-quantum hosted-processor delay. Bypass
returns the dry sum; removal restores the direct route. Neither changes loop
media or provides general latency compensation. Ordinary bus controls and Master
preview use the same signal order in all three states. Like track processor
files, bus files store canonical DSP settings and CC mappings; editor visibility
and manual runtime bypass are not file state, and loaded processors activate.

## Ownership and confirmation

Processor bindings distinguish track capture/demand from bus ownership. Bus and
processor generations, graph epochs and prepared retirement fence replacement
owners. Structural requests reserve a candidate slot, prepare controls and graph,
activate, then retire the displaced owner. Refusal leaves the prior graph intact.
Creating a bus with FX is one structural admission; failed creation publishes no
bus or ports. UI loss does not replay accepted operations.

The connected host advertises `BUS_PROCESSORS` and a Built-in-only bus catalog.
The service projects confirmed processor state, mappings, errors and the latest
valid global CC for MIDI Learn. Track and bus editor identities are distinct.
Global controls received during bypass retain their latest supported values;
reactivation drains them through the bounded control lane. Saturation keeps the
existing pending-control diagnostics and does not multiply delivery for tracks
routed to the same bus.

Processor state publication rotates across track and bus owners when the reliable
queue fills. Continuous bus MIDI observations cannot starve another processor's
editor state, including with a single-record outbox.

Coherent capture carries bus DSP settings and mappings through the portable image
and existing session archive. Loads validate all settings, mappings and aggregate
track/bus capacity before candidate activation, retain global-port intent, and
allocate fresh live identities. Malformed mappings, unavailable processors and
exhausted reservations refuse the whole load without damaging the current session.

## Verification

- `shoop_host_processors/tests/graph.rs` exercises actual mono/stereo/eight-channel
  DSP, bypass/removal, global fanout, idle controls and repeated slot reclamation
  under callback heap guards.
- `shoop_host_processors/tests/service.rs` exercises public controls, global MIDI,
  recovery without replay, atomic creation/refusal, archive round trips, malformed
  candidate refusal and Master preview through FX.
- `shoop_egui` tests exercise bus MIDI Learn using the confirmed global observation.
  The packaged canary also uses the existing Lua bus controls while FX is active.
- The processor ownership unit test runs under Miri, including typed bus binding,
  stale generation refusal and retirement cleanup.

Run the packaged native workflow after building `shoopdaloop` and `shoop_host`:

```sh
target/debug/shoopdaloop --probe-bus-fx --engine-transport socket
target/debug/shoopdaloop --probe-bus-fx --engine-transport shared_memory
```

Run the actual non-isolated Chromium AudioWorklet workflow:

```sh
cargo build -p shoop_rt_host_worklet --target wasm32-unknown-unknown
CHROME_BIN=chromium node --experimental-websocket tests/bus_fx_worklet/run.mjs
```

The fixture checks actual OfflineAudioContext output against independent dry and
gain oracles across activation, bypass, reactivation and removal. It instruments
the production processor script to detect render-time memory growth. The RT
protocol and configuration versions in the fixture must match their codecs.
Browser startup has the production runner's bounded 60-second readiness allowance;
rendering retains its separate 180-second watchdog and all sample/memory checks.

CI retains production browser session workflow output in
`browser-workflow-<profile>-<run>-<attempt>` artifacts, including failed runs.
Debug production workflows reuse the pinned Chrome installed for library browser
tests; release workflows use current stable Chrome and Firefox. All profiles keep
the same operation watchdogs and workloads.

For mixed track/bus release measurements:

```sh
cargo test -p shoop_host_processors --features builtin-fx --release --test graph bus_fx_mixed_callback_measurement -- --ignored --nocapture
```

The workload reports configured storage, callback distribution, MIDI load and
period misses. Measurements qualify that machine and workload, not a general
hardware deadline guarantee. Keep revision-specific results in PR/CI artifacts.
