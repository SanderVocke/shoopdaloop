# Network DSP host

`shoop_dsp_host` is a dedicated processing service and a software source-clock
requester. It supports direct delay-copy DSP and, with the `carla` feature, native
Carla Rack/Patchbay16. Neither mode opens an audio device. The FPGA/session graph
integration is a separate milestone; see the
[rt_runtime integration plan](rt-runtime-integration-plan.md) for the reusable
processing-link node, native network demo, and boundaries for future IPC/Zynq
adapters.

The implemented graph demo and platform binding guide are in
[`shoop_dsp_link_demo`](../../src/rust/shoop_dsp_link_demo/README.md); its
[processing-link contract](link-contract.md) is independent of UDP and native OS APIs.

## Build and run

Use the repository build environment. Default builds have no Carla dependency:

```sh
cargo build --release -p shoop_dsp_host --no-default-features
mkdir -p work/remote-dsp
./target/release/shoop_dsp_host generate-credential work/remote-dsp/key
./target/release/shoop_dsp_host serve --credential-file work/remote-dsp/key
```

The service prints its concrete control endpoint. In another terminal:

```sh
./target/release/shoop_dsp_host client --credential-file work/remote-dsp/key \
  --endpoint 127.0.0.1:PORT --packet-frames 16 --multiplier 4 --allowance 8 \
  --delay 67 --seconds 2 --warmup 0.5 --output work/remote-dsp/result.json
```

The requester generates reproducible multichannel samples, submits timestamped
packets, consumes fixed deadline slots through a bounded return buffer, and
checks received DSP samples against an independent reference. It retains late
responses for diagnostics only. Unavailable output produces silence and a MIDI
reset indication; it never shifts a later slot. Delay-copy delays audio by exactly
`A` samples and passes MIDI through at its original offsets.

Use `--start-frame` to attach at an existing absolute source frame.
Use `--channels`, `--sample-rate`, `--reorder-packets`, `--gap-timeout-us` and
`--mtu` for other stream shapes/budgets. `--impulse` selects an impulse input.
`--drop-every` and `--duplicate-every` are requester-side failure injection, not
properties of the DSP. Invalid static configurations produce a `rejected` JSON
result without contacting the endpoint. The server independently validates them.

For native Carla:

```sh
cargo build --release -p shoop_dsp_host --features carla
./target/release/shoop_dsp_host serve --processor carla --credential-file work/remote-dsp/key
./target/release/shoop_dsp_host client --processor carla --state-file project.xml \
  --credential-file work/remote-dsp/key --endpoint 127.0.0.1:PORT \
  --packet-frames 16 --multiplier 4 --allowance 64 --state-output saved-state.txt
```

`--state-file` accepts a Carla project XML, existing Shoop Carla state, or legacy
serialized state. `--state-output` saves after quiescing processing. Native loading
uses the existing bundled runtime layout and absolute development overrides
`SHOOP_CARLA_NATIVE_LIBRARY` and `SHOOP_CARLA_RESOURCE_DIR`. Build/plugin resources,
ABI checks, supported formats and UI behavior follow the pinned Carla runtime.
`--expected-gain` optionally verifies a known gain-only Carla test project; general
plugins are not assumed to have a known transfer function or zero internal delay.

Loopback is the default. `serve --lan --bind ADDRESS:PORT` enables an explicit
trusted LAN endpoint. Credential files follow the existing session host's owner,
permission/ACL, size and symlink checks. TCP uses the shared token handshake;
control and UDP are bound to the authenticated peer, negotiated port and stream
epoch. Control supports one connection at a time. `--once` exits after that
connection, which is useful for process tests and sweeps.

## DSP and transport boundaries

`shoop_dsp::Processor` owns prepared state exclusively. It processes borrowed
channel-major planar audio with a fixed number of frames and bounded MIDI, plus
source frame and transport context. `DelayCopy` and `CarlaDsp` implement the same
interface. Native algorithms or other plugin hosts can implement it without
changing networking. State/UI support and known/unknown algorithm delay are
metadata, not assumptions built into packet scheduling.

`shoop_remote_dsp` and `shoop_dsp` compile with `no_std` plus `alloc`. Preparation
allocates; wire encoding, stream adaptation, return-slot consumption and direct
DSP processing reuse prepared storage. The native host supplies sockets, elapsed
time and lifecycle. The Carla adapter depends on no legacy engine or worker
protocol; old callers retain compatibility through their engine-side adapter.

The worker only runs the next source-ordered DSP block, then returns its `P`-frame
pieces immediately. UI pumping, plugin preparation, state serialization and
reconfiguration run on the control/main thread after quiescing the worker.
Carla receives source frame and playing status; it does not invent musical BBT
position when that information is absent. Native UI cleanup remains on its owner
thread. `Ping` works during processing; `Stats`, state and visibility operations
require `Pause`. `Start` resumes the retained stream after pause. State restoration
after processing requires `Configure` with a fresh epoch.

The future FPGA render callback must exchange prepared data with its own network
adapter, without I/O or waiting inside `rt_runtime::Processor::process`. Its actual
submission cadence must provide the negotiated lead; small wire packets alone do
not create earlier input availability within a large callback.

## Wire and timing contract (version 1)

Control uses the repository's 32-byte token handshake followed by bounded JSON
frames (`u32` little-endian byte length, maximum 32 MiB). It negotiates the protocol
version, configuration, nonzero never-reused per-connection epoch, requester UDP
port and algorithm. JSON tags and invalid versions/configurations are rejected.
Clients must choose a fresh epoch for every stream incarnation, including
reconnection and process restart; the requester uses operating-system entropy.
The service rejects repeated epochs within one connection. Partial reads and
connection loss preserve the processing/ownership boundary.
If DSP cannot stop within five seconds, the dedicated process exits before native
cleanup can overlap active processing. Source inactivity retires processing after
two seconds; source-time progress is
never synthesized indefinitely while no source exists.

Audio uses UDP. The 64-byte header and every integer/float payload are explicitly
little-endian. Planar float32 samples follow the header, then zero or more fixed
72-byte MIDI records (frame `u32`, port `u16`, length `u16`, 64 data bytes).

| Offset | Field |
| --- | --- |
| 0 | `SDSP` magic, version `u16`, input/output kind `u8`, flags `u8` |
| 8 | Stream epoch `u64` |
| 16 | Packet sequence `u64` |
| 24 | Source frame `u64`, exactly `start_frame + sequence * P` |
| 32 | Frames, channels, MIDI count and zero reserved word, all `u16` |
| 40 | Optional tempo as float64; zero means absent |
| 48 | Server-local DSP processing duration in ns (`u64`), output only |
| 56 | Missing-input mask for this `k`-packet batch (`u64`), output only |

Flags are input concealment, playing and MIDI reset. MIDI offsets are relative to
one packet on the wire and to `C` inside the DSP. Missing input generates bounded
all-sound-off/all-notes-off events before processing zeros; output concealment or
MIDI overflow requests receiver MIDI reset and suppresses ordinary MIDI for that
packet. Audio continues. Context changes within a `C`-frame batch are refused.

The configured `start_frame` is an absolute frame origin; sequence numbers begin
at zero for each epoch. For packet `[s,s+P)`, nominal readiness is
`r(s)=t0+(s-start_frame+P)/fs`, and consumption is
`d(s)=r(s)+N*P/fs`. Late source dispatch never moves these deadlines. The requester
allows multiple requests in flight and measures deadline completion on its own
monotonic clock; it never subtracts server timestamps from client timestamps.

`C=kP` accumulation consumes `(k-1)*P/fs` after the first packet is ready.
Therefore `N<k` is structurally impossible with positive turnaround and is refused.
`N` is an allowance, not server holding time. The integer packet-period allowance
and exact readiness/consumption boundaries are part of this version's contract.

Reordering storage is `k + reorder_packets + 1` packet slots. A missing batch
waits at most `(k-1)` packet periods plus `gap_timeout_us` from its first observed
input; complete batches run immediately. Expiry substitutes zero input once and
records the missing mask. Duplicates/stale epochs cannot advance the DSP. Excess
future traffic is counted; sustained overflow retires the stream and requires a
new epoch. The first 1024 concealment records are retained for finite experiments;
truncation is explicit and prevents unverifiable sample claims.

The return buffer is independently bounded by the allowance, multiplier and
reorder capacity. It refuses late, old and excess-future output and consumes each
slot once. At its deadline a slot is processed audio, marked concealed audio or
silence. A late packet cannot replace already consumed fallback.

Negotiation reserves the full MIDI capacity when checking MTU in both directions.
It uses a conservative 48-byte IP/UDP allowance for IPv4 or IPv6, with no IP
fragmentation. At a 1500-byte IP MTU, 16 channels with `P=16` fit; `P=32` do not.
Algorithm sample delay remains separate from all scheduling/transport budgets.

## Verification

```sh
cargo test -p shoop_dsp -p shoop_remote_dsp -p shoop_dsp_host -p shoop_transport_socket
cargo build --lib -p shoop_dsp -p shoop_remote_dsp --target thumbv7em-none-eabihf
SHOOP_REQUIRE_CARLA_TESTS=1 cargo test -p shoop_dsp_host --features carla
```

The real-Carla test loads internal stereo Audio Gain at gain 0.5, checks network
samples, saves state and checks another process pair restored from that state.
It requires the real runtime. Other tests inject loss, duplicates, reorder,
retirement and stale epochs and verify independent sample references and fixed
presentation slots, including allocator-guarded prepared operations.

Linux coverage CI builds the pinned headless native library with
`scripts/build_carla_native_tests.py`, verifies source/header checksums from
`third_party/carla/runtime-lock.json`, and requires these real-runtime tests.
The helper accepts a fresh `--directory`, prints the absolute library/resource
overrides and optionally writes them to a GitHub Actions `--environment-file`.
It prepares a test runtime; GUI helpers and distributable components use the
existing packaging workflow.

## Localhost sweeps

```sh
cargo build --release -p shoop_dsp_host --no-default-features
python3 scripts/sweep_remote_dsp.py --profile all --output work/remote-dsp-sweep
python3 scripts/sweep_remote_dsp.py --report-only --output work/remote-dsp-sweep
```

The full profile executes the planned 100-configuration matrix, three repetitions
of admitted configurations, both non-aligned delay cases for every `P/k`, MTU
boundary cases and up to six 60-second follow-ups selected from observed baseline
boundaries. `--profile smoke` is a short correctness-focused CI subset. Other
profiles run just `baseline`, `delays` or `followup`. Results resume only when the
binary and runner hashes match; report regeneration uses stored results.

Each output directory contains the host manifest, per-run logs/results, a CSV,
compressed JSONL with timing histograms, and a Markdown report. Source-time
readiness remains fixed. A 50 ms receive grace captures diagnostic late responses
without extending consumption deadlines; responses absent at that cutoff count as
missing. Timing misses never fail CI on a wall-clock percentile threshold.

## Scheduling and polling comparisons

See the [recorded scheduling results](tuning-results.md) alongside the
[original sweep](localhost-results.md).

Both `serve` and `client` accept `--polling yield|spin`. The default keeps the
original yielding behavior. Spinning avoids voluntary scheduler handoff; it does
not reserve CPU time or prevent preemption. The yielding mode also polls actively,
so neither mode is an energy-saving blocking implementation.

The Linux comparison runner assigns affinities through `taskset`, independently
varies requester/worker polling, probes elevated priority, and compares the
200-microsecond input-gap policy with 1000 microseconds. Select two distinct CPUs
from the current allowed affinity mask; assignment does not isolate those CPUs
from other tasks:

```sh
cargo build --release -p shoop_dsp_host --no-default-features
python3 scripts/tune_remote_dsp.py --client-cpu 1 --server-cpu 2 \
  --profile all --output work/remote-dsp-tuning
python3 scripts/tune_remote_dsp.py --report-only --output work/remote-dsp-tuning
```

The default campaign randomizes three short repetitions of eight configurations
and seven conditions, then runs fresh yielding baselines and selected alternatives
for 60 seconds each. Priority conditions use cooperative yielding and are added
only when the probe confirms the requested setting took effect. Refused priority
requests are preserved in the manifest. `--profile smoke` checks the workflow;
`screen` and `followup` split the
campaign. Use a new output directory for a changed binary or runner.

Reports include child-process CPU time and cgroup-counter deltas. CPU cost covers
startup, warm-up, capture and report generation, as well as measured audio time;
it is not a DSP-only measurement. Cgroup counters include other tasks. The native
host records maximum service-loop gaps and threshold counts; these include its
own processing work and OS scheduling delays. They are not direct scheduler
latency measurements. No builds or test campaigns should run concurrently.
