---
name: tracy
description: Capture and investigate native ShoopDaLoop Tracy profiles, including GUI/application intent flow, engine control and graph work, realtime audio timing, Tiny Synth/FX processing, queues, scheduling, and state publication from .tracy files.
compatibility: The native application emits Tracy 0.13.1-compatible captures. Querying requires the matching static tracy-query release binary.
---

# Debug ShoopDaLoop with Tracy

Use this skill for the native `shoopdaloop` application. It does not describe the legacy frontend, its self-tests, or its CLI and trace data.

Use the versioned `tracy-query` skill distributed with `tracy-query` for complete query syntax. This skill covers ShoopDaLoop-specific capture and interpretation.

## Obtain `tracy-query` and its skill

Download both from the `tracy-query` v0.1.0 release:

- Release page: <https://github.com/SanderVocke/tracy-query/releases/tag/v0.1.0>
- Query skill: <https://github.com/SanderVocke/tracy-query/releases/download/v0.1.0/SKILL.md>

Choose the static binary for the current platform:

- `tracy-query-linux-x86_64`
- `tracy-query-linux-arm64`
- `tracy-query-macos-x86_64`
- `tracy-query-macos-arm64`
- `tracy-query-windows-x86_64.exe`
- `tracy-query-windows-arm64.exe`

For example:

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

Read the downloaded `SKILL.md` before querying. Keep downloaded binaries with investigation artifacts or in a temporary tools directory; do not commit them.

Validate the tool and capture before analysis:

```sh
TQ="$TRACE_DIR/$ASSET"
TRACE="$TRACE_DIR/capture.tracy"

"$TQ" --version
"$TQ" check "$TRACE"
"$TQ" range "$TRACE"
"$TQ" info "$TRACE"
"$TQ" sources --count "$TRACE"
```

`tracy-query` reads captures; it does not create them.

## Build and run the native application

Build the native application:

```sh
cargo build -p shoopdaloop
```

The application executable has three tracing options:

```text
--tracing                enable live Tracy profiling
--tracing-capture        start tracy-capture and write below ./traces
--tracing-engine-detail  add detailed realtime engine zones; requires either mode
```

There are no CLI options for selecting the capture executable or output directory. `TRACY_CAPTURE_TOOL` selects the executable; otherwise it is resolved as `tracy-capture` on `PATH`. The output directory is `traces` relative to the application's working directory.

The audio backend is selected through persisted application settings, not a `--backend` argument. Reproduce with the configured JACK, CPAL+midir, or dummy backend that matters to the issue.

### Live profiling

Connect a matching Tracy 0.13.1 profiler, then run:

```sh
cargo run -p shoopdaloop -- --tracing
```

For detailed engine stages:

```sh
cargo run -p shoopdaloop -- \
  --tracing \
  --tracing-engine-detail
```

### Capture a `.tracy` file

Install the Tracy 0.13.1 `tracy-capture` executable. It is distinct from `tracy-query`.

```sh
TRACY_CAPTURE_TOOL="$(command -v tracy-capture)" \
  cargo run -p shoopdaloop -- \
    --tracing-capture \
    --tracing-engine-detail
```

`--tracing-capture` enables tracing automatically. Omit engine detail for lower callback overhead and smaller captures. Quit normally so the capture can disconnect and finalize.

A capture run creates:

- a non-empty numbered file such as `traces/0001-application.tracy`;
- `traces/manifest.tsv`, with an `application` label row;
- `traces/tracy-capture.log`.

Before interpreting a capture, require all of the following:

1. Application output reports `Finalized Tracy capture`.
2. The selected `.tracy` file is non-empty.
3. `manifest.tsv` has the corresponding successful application row.
4. `tracy-capture.log` reports that Tracy saved the trace.
5. Application output and the trace contain no instrumentation-failure diagnostic.
6. `tracy-query check` succeeds.

## Expected application trace data

ShoopDaLoop uses fixed, bounded zone names. User labels, paths, processor state, MIDI payloads, and audio samples do not become hot-zone names.

### GUI and application zones

Expected native GUI zones include:

- `app.egui.run`: native eframe lifetime.
- `frontend.egui.initialize`: settings, widget, backend, and runtime initialization.
- `frontend.egui.update`: one top-level GUI update.
- `frontend.egui.frame`: widget rendering, with revision and bounded state counts.
- `frontend.egui.tracks` and `frontend.egui.track`: track collection and per-track rendering.
- `frontend.egui.intent_dispatch`: composition-root handling of a GUI intent.
- `frontend.egui.settings_action`: native settings actions.
- `frontend.app.intent_dispatch`: application queue submission, with `intent_id`, intent kind, and queue outcome.
- `frontend.app.intent_handle`: actor-side handling with the same `intent_id` and intent kind.
- `frontend.app.intent_apply`: model mutation, revision, and success/error outcome.
- `frontend.app.update`, `frontend.app.backend_advance`, `frontend.app.backend_snapshot_apply`, and `frontend.app.snapshot_publish`: polling, state application, and publication.
- `frontend.app.runtime_start`, `frontend.app.runtime_shutdown`, and `worker.application`: actor lifecycle.

A UI intent normally nests `frontend.app.intent_dispatch` inside `frontend.egui.intent_dispatch` on the GUI thread. Join only the app dispatch zone to `frontend.app.intent_handle` by `intent_id`; the egui zone itself does not carry that ID. `frontend.app.intent_apply` then contains the resulting backend and `engine.control.*` work when it is synchronous.

Some operations continue in later `frontend.app.update` zones. File import, click generation, session I/O, and driver switching therefore need not remain nested under the original intent. Correlate their status messages, revisions, and later engine-control zones rather than assuming one synchronous span.

### Interaction messages

The GUI layer emits sparse structured Tracy messages when actions occur:

- `frontend.egui.intent_created`: one message per application intent, with stable intent kind and snapshot revision;
- `frontend.egui.action_batch`: application/settings action counts and revision;
- `frontend.egui.track_interaction` and `frontend.egui.tracks_interaction`: bounded track-level interaction counts;
- `frontend.app.intent_failed`: failed model application with intent kind and error;
- `frontend.egui.tracing_started`: selected tracing modes.

Intent kinds include stable categories such as `loop.play`, `track.output_gain`, `session.request_save`, and Tiny Synth/FX controls under `track.tiny_synth_fx.*`. Interaction messages do not always include a target object ID. Never invent a track or loop target from the intent kind alone.

UI-only actions such as opening a dialog may have an interaction message but no application intent. Conversely, asynchronous file completion creates a later app intent outside the original UI dispatch span.

### Engine zones

Expected engine categories include:

- `engine.control.*`: command submission, queue/results, waits, object creation, loop transitions, targeted loop-content replacement, reclamation, and graph queries;
- `engine.graph.*`: topology/schedule construction, scheduler start, arm/apply generations, and flushes;
- `engine.driver.*`: non-realtime driver creation, startup, and controlled waits;
- `engine.rt.*`: realtime driver, callback, command, graph-state, cycle, session, loop, FX, and state-publication hierarchy;
- `worker.*`: application actor, graph scheduler, dummy driver, connection caches, plugin UI, and other background work;
- `engine.plugin.*`: non-realtime plugin discovery, state, and UI operations when Carla/LV2 is enabled.

A typical dummy hierarchy is:

```text
engine.rt.driver
  engine.rt.driver.dummy
    engine.rt.callback
      engine.rt.commands
      engine.rt.graph_state
      engine.rt.cycle
        engine.rt.session
          engine.rt.loops
          engine.rt.fx
          engine.rt.state_publication
```

Coarse tracing contains the callback/session categories. `--tracing-engine-detail` adds fixed per-port/channel, composite timeline, MIDI playback, external routing, trace publication, plugin-processing, and `engine.rt.fx.tiny_synth_process` zones. Numeric zone values carry bounded context such as driver kind, frame count, or arena index.

### Frames and plots

The native application emits the `engine.callback` frame set for audio cycles. Use `frontend.egui.frame` or `frontend.egui.update` CPU zones—not a `frontend.refresh` frame set—to align GUI work.

Do not expect legacy `BackendWrapper/*` health plots or legacy frontend object plots in an application capture. Their absence is normal. The application trace exposes only limited snapshot counts and status fields, so use zone fields, structured messages, shell diagnostics, and adjacent engine command/callback evidence. In particular, the lack of an xrun or DSP-load plot is not evidence that no xrun or load problem occurred.

## Tracy message formats

All events emitted through ShoopDaLoop's Tracy event layer begin with an explicit severity:

```text
log.level = <TRACE|DEBUG|INFO|WARN|ERROR>, <event fields>
```

Direct `tracing` events from the GUI/application path generally look like:

```text
log.level = TRACE, message = frontend.egui.intent_created, intent = track.tiny_synth_fx.panic, revision = 42
```

Field order follows the emitting event and should not be used as a schema. Filter on anchored labels. The Rust tracing metadata target (for example `Frontend.Egui`) is not automatically added to direct-event message text.

Records bridged from the Rust `log` facade include call-site metadata and generally look like:

```text
log.level = INFO, message = <payload>, log.target = <logical target>, log.module_path = <Rust module>, log.file = <source path>, log.line = <line>
```

The payload may contain commas or equals signs. Anchor metadata on labels and delimiters:

```sh
# Exact severity.
"$TQ" query --kind message \
  --filter 'message.text=^log\.level = ERROR(,|$)' "$TRACE"

# Warnings or errors.
"$TQ" query --kind message \
  --filter 'message.text=^log\.level = (WARN|ERROR)(,|$)' "$TRACE"

# GUI-created intent events.
"$TQ" query --kind message \
  --filter 'message.text=(^|, )message = frontend\.egui\.intent_created(,|$)' "$TRACE"

# One stable intent family.
"$TQ" query --kind message \
  --filter 'message.text=(^|, )intent = track\.tiny_synth_fx\.' "$TRACE"

# Bridged log target, when that suffix is present.
"$TQ" query --kind message \
  --filter 'message.text=(^|, )log\.target = Backend(\.|,|$)' "$TRACE"
```

Do not treat Tracy's message `color` as severity. Shoop records severity in `message.text`; red may instead indicate an instrumentation diagnostic. Do not infer a level by searching payload words such as `warn`, `error`, or `failed`.

Shell output uses:

```text
[<RFC3339 timestamp>] [<thread name/id>] [<target>] [TRACE|DEBUG|INFO|WARN|ERROR] <fields/message>
```

`SHOOP_LOG`, or `RUST_LOG` when `SHOOP_LOG` is unset, filters shell output. Tracy event capture is filtered independently, so changing the shell filter does not remove enabled Tracy events.

Inspect a message sample before relying on a suffix: direct tracing events and bridged log records intentionally have different fields.

## Investigation workflow

Follow the downloaded `tracy-query` skill's validate, inventory, count-first, and narrow-window workflow. For ShoopDaLoop:

1. Inventory `frontend.egui.intent_created` messages for a sparse user-action timeline.
2. Find the adjacent `frontend.egui.intent_dispatch` and nested `frontend.app.intent_dispatch`.
3. Join app dispatch to `frontend.app.intent_handle` by `intent_id`.
4. Inspect `frontend.app.intent_apply` and nested `engine.control.*` queue/wait outcomes.
5. For asynchronous I/O or driver operations, follow later `frontend.app.update` spans, messages, and snapshot revisions.
6. If topology changed, find the corresponding `engine.graph.*` arm and apply generation.
7. Inspect the next `engine.rt.callback` hierarchy; compare callback duration to the frame budget derived from its frame count and the configured sample rate.
8. For FX issues, inspect `engine.rt.fx`; with engine detail, distinguish Tiny Synth/FX and Carla/plugin processing stages.
9. For state visibility, follow `engine.rt.state_publication` into backend snapshot application and `frontend.app.snapshot_publish`, then the next GUI frame revision.
10. For regressions, capture equivalent scenarios and durations and compare identical normalized windows and filters. Counts alone do not establish a performance regression.

For short-session reconstruction, compare snapshot fields and counts before and after each action. Infer topology only from explicit descriptors or bounded creation-zone evidence, and label inferred conclusions as such.

Distinguish observed records from interpretation. Report the exact capture, query command, normalized range, filters, and relevant output behind each conclusion.

## Safety and interpretation limits

Tracing is a debugging mode, not a transparent realtime measurement mode. Tracy calls may allocate, initialize thread-local state, grow queues, or lock internally. Driver-owned callback threads cannot always be prewarmed. Tracing can cause xruns and alter callback timing.

Prefer coarse tracing first. Enable engine detail only when per-stage resolution is necessary, and do not compare detailed callback durations directly with an untraced run. Use the capture's timer resolution when interpreting short zones.
