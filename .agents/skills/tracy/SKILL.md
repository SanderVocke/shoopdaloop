---
name: tracy
description: Capture and investigate ShoopDaLoop Tracy profiles, including live tracing, application and QML-test captures, ShoopDaLoop-specific zones and plots, and analysis with tracy-query. Use when debugging timing, realtime audio, frontend refresh, engine graph/control, scheduling, state publication, xrun, queue, or performance issues from .tracy files.
compatibility: ShoopDaLoop emits Tracy 0.13.1-compatible captures. Querying requires the matching static tracy-query release binary.
---

# Debug ShoopDaLoop with Tracy

Use this skill to acquire a representative ShoopDaLoop trace and interpret its application-specific instrumentation. Use the versioned `tracy-query` skill distributed with `tracy-query` for the query CLI's complete command semantics.

## Obtain `tracy-query` and its skill

Download both from the `tracy-query` v0.1.0 release:

- Release page: <https://github.com/SanderVocke/tracy-query/releases/tag/v0.1.0>
- Released query skill: <https://github.com/SanderVocke/tracy-query/releases/download/v0.1.0/SKILL.md>

Choose the static binary for the current platform:

- `tracy-query-linux-x86_64`
- `tracy-query-linux-arm64`
- `tracy-query-macos-x86_64`
- `tracy-query-macos-arm64`
- `tracy-query-windows-x86_64.exe`
- `tracy-query-windows-arm64.exe`

The GitHub CLI can download the binary and its matching skill alongside a trace:

```sh
TRACE_DIR=traces/investigation
ASSET=tracy-query-linux-x86_64 # select for the current OS and architecture

gh release download v0.1.0 \
  --repo SanderVocke/tracy-query \
  --pattern "$ASSET" \
  --pattern SKILL.md \
  --dir "$TRACE_DIR" \
  --clobber
chmod +x "$TRACE_DIR/$ASSET"
```

Read the downloaded `SKILL.md` before querying. It is the authoritative guide for validating traces, discovering sources, narrowing time ranges, filtering records, comparing captures, and avoiding interpretation errors. Keep the binary and downloaded skill with investigation artifacts or in a temporary tools directory; do not commit release binaries to the repository.

Set a convenient path and validate the tool and capture before analysis:

```sh
TQ="$TRACE_DIR/$ASSET"
TRACE="$TRACE_DIR/capture.tracy"

"$TQ" --version
"$TQ" check "$TRACE"
"$TQ" range "$TRACE"
"$TQ" info "$TRACE"
"$TQ" sources --count "$TRACE"
```

`tracy-query` reads existing Tracy 0.13.1-compatible captures; it does not create them. Follow the released skill for all further query syntax.

## Build and run ShoopDaLoop with tracing

Initialize submodules and build the development launcher:

```sh
git submodule update --init --recursive
cargo build
```

Run through `target/debug/shoopdaloop_dev.sh` so in-source resources and QML are available.

### Live Tracy profiling

Use `--tracing` while a matching Tracy 0.13.1 profiler connects to the application:

```sh
target/debug/shoopdaloop_dev.sh \
  --backend dummy \
  --tracing
```

Add `--tracing-engine-detail` when per-node engine work is needed:

```sh
target/debug/shoopdaloop_dev.sh \
  --backend dummy \
  --tracing \
  --tracing-engine-detail
```

`--tracing-engine-detail` requires either `--tracing` or `--tracing-capture`. It increases callback overhead and trace volume.

### Capture a `.tracy` file

Install the Tracy 0.13.1 `tracy-capture` executable. This is a different program from `tracy-query`. ShoopDaLoop can find it on `PATH`, through `TRACY_CAPTURE_TOOL`, or through `--tracing-capture-tool`.

Capture an application run into a dedicated investigation directory:

```sh
TRACE_DIR=traces/investigation
mkdir -p "$TRACE_DIR"

target/debug/shoopdaloop_dev.sh \
  --backend dummy \
  --no-crash-handling \
  --tracing-capture \
  --tracing-engine-detail \
  --tracing-capture-tool "$(command -v tracy-capture)" \
  --tracing-capture-output-dir "$TRACE_DIR"
```

Reproduce the issue and quit normally so ShoopDaLoop finalizes the capture. Select the real backend involved in the issue instead of `dummy` when backend behavior matters. `--tracing-capture` enables tracing automatically. Omit engine detail for lower overhead and a smaller coarse trace.

For QML self-tests, combine the capture options with `--self-test`. ShoopDaLoop writes one numbered capture per loaded `tst_*.qml` file, plus `manifest.tsv` and `tracy-capture.log`.

After capture, require all of the following before interpreting it:

1. ShoopDaLoop exited normally and logged `Finalized Tracy capture`.
2. The `.tracy` file is non-empty.
3. `tracy-capture.log` reports that the trace was saved.
4. Application output contains no `Instrumentation failure`.
5. `tracy-query check` succeeds.

## Instrumentation published by ShoopDaLoop

ShoopDaLoop uses fixed, bounded zone names. Runtime labels and audio or MIDI payloads do not become hot-zone names. Coarse tracing covers application/frontend spans, engine control and graph work, and bounded realtime categories. Engine detail adds static per-node and routing stages.

### CPU zones

- `app.*`: process startup, configuration, Qt initialization and event loop, crash handling, lifecycle, and shutdown.
- `frontend.*`: QML and Lua execution, file/session work, control dispatch, rendering, backend state consumption, object updates, and refresh scheduling.
- `engine.control.*`: command enqueueing, queueing, synchronous waits, results, session object creation, loop transitions, and reclamation.
- `engine.graph.*`: graph topology and processing-order construction, schedule building, arm/apply generations, flushes, and scheduler work.
- `engine.composite.*`: composite-loop planning and control work.
- `engine.rt.*`: realtime driver, callback, cycle, session, loop, FX, and state-publication hierarchy.
- `worker.*`: graph scheduler, dummy driver, plugin UI, MIDI connection, and other background workers.
- `tool.*`: instrumented support and packaging tool work when present.

A typical dummy realtime hierarchy is:

```text
engine.rt.driver
  engine.rt.driver.dummy
    engine.rt.callback
      engine.rt.cycle
        engine.rt.session
```

Coarse session categories include loops, FX, and state publication. `--tracing-engine-detail` adds fixed `ports.*`, `channels.*`, `composites.*`, `midi.*`, external-routing, and plugin-processing zones. Numeric zone values carry bounded context such as driver kind, frame count, or arena index.

### Frame sets and plots

Frame marks align the audio and frontend timelines:

- `engine.callback`: audio processing cycles.
- `frontend.refresh`: GUI/backend-state refreshes.

`BackendWrapper/*` plots expose engine health and control-flow state, including:

- callback last/worst duration and callback-budget overruns;
- cycles, processed frames, sample rate, buffer size, and update interval;
- pending/applied commands and command sequence;
- schedule request/applied generation, stale or stuck cycles, and sub-block count;
- graph arms/applies and dropped trace snapshots;
- capture underruns/overruns, xruns, and DSP load;
- audio buffer pool creation and availability.

Object-state plots describe the most recently consumed state:

- `engine.loop.*`: mode, position, length, and iteration-related state.
- `engine.composite.*`: mode, iteration, position, and length-related state.
- `engine.port.*`: input/output peaks and event activity.
- `engine.channel.*`: mode, length, position, and data state.
- `engine.fx.*`: FX-chain state and load-related values.

Several objects may update during one frontend refresh. Correlate object plots with adjacent object update zones rather than assuming a plot identifies one particular object.

### Shoop log messages

Most Rust `log` records appear in Tracy as a `message` whose `text` field has this form:

```text
log.level = <TRACE|DEBUG|INFO|WARN|ERROR>, message = <payload>, log.target = <logical target>, log.module_path = <Rust module>, log.file = <source path>, log.line = <line>
```

For example:

```text
log.level = INFO, message = Transitioning 1 loops to 2 with delay -1, sync at cycle -1, log.target = Frontend.Loop, log.module_path = frontend::cxx_qt_shoop::rust::qobj_loop_gui, log.file = src/rust/frontend/src/cxx_qt_shoop/rust/qobj_loop_gui.rs, log.line = 477
```

The embedded metadata is not promoted to separate `tracy-query` fields; filter it through `message.text`. Anchor metadata values on their labels and delimiters because the payload may itself contain commas or equals signs:

```sh
# One exact severity.
"$TQ" query --kind message \
  --filter 'message.text=^log\.level = ERROR(,|$)' "$TRACE"

# Warning or error severity.
"$TQ" query --kind message \
  --filter 'message.text=^log\.level = (WARN|ERROR)(,|$)' "$TRACE"

# Severity and logical logging target/category. Repeated filters are ANDed.
"$TQ" query --kind message \
  --filter 'message.text=^log\.level = (DEBUG|INFO)(,|$)' \
  --filter 'message.text=(^|, )log\.target = Frontend\.Loop(,|$)' "$TRACE"

# Rust module path.
"$TQ" query --kind message \
  --filter 'message.text=(^|, )log\.module_path = frontend::cxx_qt_shoop::rust::qobj_loop_gui(,|$)' "$TRACE"

# Source tree or one source file.
"$TQ" query --kind message \
  --filter 'message.text=(^|, )log\.file = src/rust/frontend/src/' \
  --filter 'message.text=qobj_loop_gui\.rs(,|$)' "$TRACE"

# Exact source line.
"$TQ" query --kind message \
  --filter 'message.text=(^|, )log\.line = 477(,|$)' "$TRACE"
```

`log.level` is the event severity. `log.target` is the stable logical category normally shown as names such as `Frontend.Loop` or `Frontend.BackendWrapper`. `log.module_path` is the Rust module that emitted the record. `log.file` and `log.line` identify the call site. Prefer the logical target for durable investigations; module paths and source lines are more exact but change during refactoring.

Do not treat the Tracy message `color` field as severity. Shoop encodes severity explicitly in `message.text`; ordinary records normally use the default color, while red may indicate an instrumentation failure from the Tracy integration. Searching the payload for words such as `warn`, `error`, or `failed` is likewise not a level filter.

The companion application output remains formatted as:

```text
[<RFC3339 timestamp>] [<thread name/id>] [<logical target>] [TRACE|DEBUG|INFO|WARN|ERROR] <payload>
```

`SHOOP_LOG`, or `RUST_LOG` when `SHOOP_LOG` is unset, controls that shell output. For example, `SHOOP_LOG='info,Frontend.Loop=debug'` enables general info output and debug output for that target. Tracy event capture is filtered independently and does not change the shell format.

Captures made before severity was added have messages beginning with `message =` instead of `log.level =` and cannot be filtered reliably by level after capture.

Not every Tracy message must follow the Rust-log suffix format. QML, support tools, or instrumentation diagnostics may emit different text. Inspect a small sample before applying suffix-specific filters. Sampling, scheduler, lock, memory, and hardware-counter collections likewise depend on what Tracy recorded; an absent category is not proof that the corresponding behavior did not occur.

## ShoopDaLoop investigation workflow

Use the released `tracy-query` skill's validate, inventory, count-first, and narrow-window workflow. Apply ShoopDaLoop's instrumentation in this order:

1. Identify the symptom and normalized time window from messages, frames, or a health plot.
2. Correlate a `frontend.*` control span with its `engine.control.*` command sequence and queue or wait result.
3. If topology changed, find the corresponding `engine.graph.*` arm and apply generation.
4. Inspect the next `engine.rt.callback` hierarchy and compare its duration with callback-budget, xrun, schedule-generation, stale/stuck, and sub-block plots.
5. Follow `engine.rt.state_publication` into `frontend.refresh.run`, then inspect object update zones and object/health plots.
6. For realtime overruns, compare coarse and detailed captures. Detailed tracing perturbs the callback more and must not be treated as transparent timing.
7. For regressions, capture equivalent scenarios and durations, assign stable trace labels, and compare identical windows and filters. Counts alone do not establish a performance regression.

Distinguish observed records from interpretation. Report the exact capture, query command, normalized range, filters, and relevant output behind each conclusion.

## Safety and interpretation limits

Tracing is a debugging mode, not a transparent realtime measurement mode. Tracy calls may allocate, initialize thread-local state, grow queues, or lock internally. Driver-owned callback threads cannot always be prewarmed. Tracing can cause xruns and alter callback timing.

Use `--rt-alloc-guard` as a diagnostic aid where appropriate, but do not infer uninstrumented performance from traced callback durations. Prefer coarse tracing first and enable engine detail only when the additional stage resolution is necessary. Use the capture's timer resolution when interpreting short zones.
