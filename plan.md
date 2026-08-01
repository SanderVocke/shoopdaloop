# Plan: per-QML-testcase Tracy capture and opt-in CI trace artifacts

## Objective

Extend the tracing work on `tracing_frontend` / PR #660 so that:

1. A developer can run QML self-tests locally with Tracy enabled and receive one finalized `.tracy` capture for each QML test file loaded by `TestFileRunner`.
2. The manually dispatched `build_and_test` workflow exposes a boolean input that is **off by default**. Enabling it schedules a dedicated Linux QML trace-capture job and uploads the resulting captures as an archive, even when QML tests fail.
3. Normal app runs, normal QML tests, pull-request runs, pushes, and scheduled CI runs do not download Tracy tools, launch `tracy-capture`, create trace files, or upload trace artifacts.
4. The implementation is validated locally only in this phase. The new trace-producing CI path is not dispatched or otherwise tested in CI yet.

The capture boundary in this plan is one QML test file (`tst_*.qml`) as advanced by `TestFileRunner`. In the current suite each such file is the isolated QML engine load and contains one `ShoopTestCase`/`ShoopSessionTestCase`, so this provides one trace per QML testcase without restarting the capture process for every individual `test_*` function.

## Mandatory preflight and allowed stop condition

Before editing code, locate and validate a compatible capture executable:

```sh
command -v tracy-capture
tracy-capture --help
```

Also check `TRACY_CAPTURE_TOOL` and common local installation paths if `command -v` fails. The embedded client from `tracy-client-sys 0.28.0` is Tracy **0.13.1**, so the preferred capture tool is Tracy 0.13.1.

If no executable `tracy-capture` is available in the development environment, the executor is explicitly allowed to stop immediately. Report:

- every path/environment variable checked;
- the command output or error;
- the required compatible version;
- that no implementation or CI execution was attempted.

Do not substitute the GUI profiler executable for `tracy-capture`.

## Current state and prior art

- PR #660 already enables the Tracy client with `--tracing` and has frontend spans, plots, and frame marks.
- There is currently no capture-process manager, capture CLI, per-test capture lifecycle, or trace artifact upload.
- `tracing_cxx` contains useful prior art:
  - `src/rust/common/src/tracing_capture.rs`;
  - commits `adb023c7`, `983c057c`, `80695263`, `89348f5c`, and subsequent fixes through `69da8692`;
  - `.github/actions/install_tracy/action.yml`.
- Do **not** cherry-pick those commits wholesale. They contain obsolete C++ backend work, stale CI structure, an unnecessary QML singleton, loose argument parsing, and unrelated changes.
- The old runner restarted capture in `maybe_run_next_test_file`, which means the previous trace was finalized immediately before the next file. Preserve the useful per-file behavior, but make ownership and finalization explicit and error-aware.

## Scope

### In scope

- A Rust capture-process manager in `common`.
- CLI options for enabling capture, selecting the tool, and selecting an output directory.
- Automatic Tracy-layer enablement when capture is requested.
- Deterministic, sanitized, unique per-QML-file capture names.
- Connection readiness checks and bounded graceful shutdown.
- Explicit finalization before the test process reports completion.
- Local unit/integration testing of capture lifecycle.
- A default-off manual workflow input.
- A dedicated Linux trace job using the release portable artifact.
- Installation of a version-compatible Linux `tracy-capture` only in that job.
- Packaging captures, metadata, capture logs, and JUnit output into an archive and uploading it with `actions/upload-artifact`.

### Out of scope

- Any new engine/backend instrumentation.
- Tracy capture for Rust unit tests, screenshots, monkey tests, or ordinary app CI jobs.
- Automatic capture on PR, push, schedule, or release events.
- Automatic capture for every `test_*` function inside a QML testcase.
- A QML-exposed `ShoopRustTracingCapture` singleton. Rust `TestFileRunner` can call the capture manager directly.
- Tracy profiler/capture CI execution during this implementation phase.
- Running `gh workflow run`, manually dispatching the workflow, or otherwise testing the new trace job on GitHub Actions.
- Broadly rewriting `build_tracy.yml` unless the existing pinned capture asset is proven unusable.

## Design decisions

### Capture lifecycle

Use a single process-global capture controller because the application has one Tracy client and can have only one capture consumer at a time. Keep all mutable state behind one `Mutex` and return errors instead of panicking on poisoned locks or invalid lifecycle calls.

Suggested public API in `common::tracing_capture`:

```rust
pub struct CaptureConfig {
    pub tool: PathBuf,
    pub output_dir: PathBuf,
    pub connect_timeout: Duration,
    pub stop_timeout: Duration,
}

pub fn configure(config: CaptureConfig) -> Result<(), CaptureError>;
pub fn is_configured() -> bool;
pub fn start_named_capture(label: &str) -> Result<PathBuf, CaptureError>;
pub fn start_default_capture() -> Result<PathBuf, CaptureError>;
pub fn stop_capture() -> Result<Option<PathBuf>, CaptureError>;
pub fn shutdown() -> Result<(), CaptureError>;
```

Exact names may vary, but preserve these semantics:

- `configure` validates that the tool exists and is executable and creates the output directory.
- `start_named_capture` first finalizes any active capture, sanitizes the label, chooses a unique path, launches `tracy-capture -o <path>`, and waits for `tracy_client::Client::is_connected()`.
- `stop_capture` sends a graceful interrupt, waits with a bounded timeout, force-kills only after timeout, waits for the process, and verifies that the expected output exists and is non-empty.
- `shutdown` is idempotent and safe from a cleanup guard.
- Starting capture while unconfigured is an error, not a warning/no-op.
- Capture failures requested by the user or CI are fatal to that run. Do not silently continue without traces.

### Process I/O and shutdown

Prefer redirecting the child process's stdout and stderr to a capture log in the trace output directory rather than maintaining indefinitely blocking pipe-reader threads. If immediate forwarding is required, use reader threads with bounded joining, but avoid the old helper-thread/detach pattern unless local tests prove it necessary.

On Unix, send `SIGINT` with `libc::kill` so `tracy-capture` finalizes the file. Poll `Child::try_wait()` up to a fixed timeout (five seconds is a reasonable initial value), then use `Child::kill()` as a last resort. Always call `wait()` after termination to reap the child.

The code must still compile on Windows and macOS. The first CI capture job is Linux-only. Use `cfg` blocks for platform termination behavior and return a clear unsupported/graceful-finalization error where reliable behavior has not been implemented, rather than pretending a forcibly terminated capture is valid.

After stopping a capture, wait briefly for `Client::is_connected()` to become false before launching the next capture. After launching, wait until it becomes true before loading the next QML file. This avoids losing the beginning of a testcase or accidentally sending it to the prior capture. Both waits must time out with actionable errors.

### File naming and manifest

Create captures under an explicit output directory, defaulting to `traces` for local runs. Use a monotonic sequence plus a sanitized test stem, for example:

```text
traces/
  0001-tst_TwoLoops.tracy
  0002-tst_MidiControlPort.tracy
  tracy-capture.log
  manifest.tsv
```

Sanitization must:

- use only the final file stem, never a caller-controlled directory;
- replace characters outside `[A-Za-z0-9._-]` with `_`;
- reject `.`/`..` and empty results;
- use the numeric sequence to prevent collisions from duplicate stems.

Append a manifest row only after a capture is finalized successfully. Include sequence, source QML path, capture filename, start/end timestamps, final child status, and if readily available the testcase pass/fail outcome. The CI archive should also include the workflow SHA and Tracy version in a small metadata file.

### CLI behavior

Add developer options in `src/rust/shoopdaloop/src/cli_args.rs`:

- `--tracing-capture`: enable external capture;
- `--tracing-capture-tool <PATH>`: capture executable, falling back to `TRACY_CAPTURE_TOOL`, then `tracy-capture` on `PATH`;
- `--tracing-capture-output-dir <PATH>`: default `traces`;
- optionally a connect timeout only if local testing demonstrates a need to tune it.

Do not expose a free-form shell argument string. Invoke `Command` with separate arguments so output paths with spaces work and no shell parsing is involved.

Capture must imply tracing. Update the pre-logging argument scan in `main_impl.rs` so either `--tracing` or `--tracing-capture` calls `set_tracing_enabled(true)` before `common::init()`. Keep the parsed CLI validation too, and make tool/output options require capture where Clap supports it.

In `lib_impl.rs`:

- configure capture after parsing arguments and after logging/Tracy initialization;
- for an ordinary app run, start one default capture immediately;
- for `--self-test`, configure the controller but let `TestFileRunner` start the first named capture, avoiding an extra startup-only `.tracy` file;
- install an RAII cleanup guard around the application entry point so all normal returns finalize an active capture;
- propagate configuration/startup errors to the top-level error result.

### QML test boundary integration

Modify `src/rust/frontend/src/cxx_qt_shoop/rust/test/qobj_test_file_runner.rs` directly; do not add a QML singleton.

Track enough state in `TestFileRunnerRust` to know the current source test path and whether it owns an active capture. The intended sequence is:

1. `start` discovers and orders test files as it does now.
2. `maybe_run_next_test_file` removes the next path.
3. If capture is configured, start a named capture for that path and wait for connection readiness.
4. Only then emit `reload_qml`.
5. `on_testcase_done` gathers results and unloads QML as it does now; keep capture active during unload so destruction-related activity remains in the same trace.
6. `on_qml_engine_destroyed` finalizes and verifies the current capture before advancing to the next file.
7. After the last file, report results and emit `done` only after the last capture is finalized.
8. On capture start/stop/finalization failure, record a clear error, stop advancing files, and emit a nonzero result. Preserve whatever traces/logs were already finalized.

Keep capture-disabled behavior byte-for-byte close to the current control flow. Do not warn once per testcase when capture is not configured.

If local inspection finds a QML file with more than one `ShoopTestCase` object, document that it produces one trace for the file-level engine lifetime. Do not add per-function restarts in this task.

## Implementation steps

### 1. Add the capture controller

Files:

- `src/rust/common/src/tracing_capture.rs` (new);
- `src/rust/common/src/lib.rs`;
- `src/rust/common/Cargo.toml`;
- workspace dependencies only if needed;
- `.gitignore`.

Actions:

1. Add the module and typed errors.
2. Add only the minimal dependency needed for Unix `SIGINT` (`libc` is already a workspace dependency). Avoid `shlex` by using structured command arguments.
3. Implement configuration, start, readiness wait, graceful stop, output verification, idempotent shutdown, and label sanitization.
4. Ignore `*.tracy` and the local trace output directory.
5. Add focused unit tests for sanitization, unique naming, invalid lifecycle calls, and timeout behavior. Use a temporary fake capture executable/script for process lifecycle tests where possible; serialize tests that touch global state.

### 2. Add CLI and application lifecycle wiring

Files:

- `src/rust/shoopdaloop/src/cli_args.rs`;
- `src/rust/shoopdaloop/src/main_impl.rs`;
- `src/rust/shoopdaloop/src/lib_impl.rs`.

Actions:

1. Add capture options and help text.
2. Make capture imply early Tracy initialization.
3. Resolve the tool path in the order CLI, environment, `PATH`.
4. Configure capture and choose ordinary-app versus self-test ownership.
5. Add an idempotent cleanup guard.
6. Ensure `--help` and `--version` do not launch capture.
7. Ensure invalid tool paths produce a concise nonzero startup failure.

### 3. Integrate capture with `TestFileRunner`

Files:

- `src/rust/frontend/src/cxx_qt_shoop/rust/test/qobj_test_file_runner.rs`;
- `src/rust/frontend/src/cxx_qt_shoop/rust/test/qobj_test_file_runner_bridge.rs` only if additional Rust-side state is required.

Actions:

1. Start a connected named capture before each QML reload.
2. Keep it running through QML unload.
3. Finalize it on engine destruction before the next file.
4. Finalize the last trace before reporting completion.
5. Propagate capture errors into the test runner's exit code.
6. Append manifest information after successful finalization.

### 4. Make the QML test action optionally pass capture arguments

File:

- `.github/actions/test_qml/action.yml`.

Add default-off inputs such as:

```yaml
inputs:
  capture_traces:
    type: boolean
    default: false
  tracy_capture_tool:
    type: string
    default: ''
  tracy_capture_output_dir:
    type: string
    default: traces/qml
```

Build the command in a shell block/array so the tool and output paths remain correctly quoted. When `capture_traces` is false, invoke exactly the existing command. When true, append:

```text
--tracing
--tracing-capture
--tracing-capture-tool <tool>
--tracing-capture-output-dir <dir>
```

Retain `pipefail`, timeout behavior, console teeing, JUnit generation, and result publication. Do not remove the existing `QOVERAGERESULT` filtering from normal runs merely to support tracing.

### 5. Add a Tracy capture installer used only by the opt-in job

Preferred file:

- `.github/actions/install_tracy_capture/action.yml` (new).

For the initial CI integration, support Linux x86_64 only because the dedicated job is Linux-only. The action should:

1. Download the existing pinned 0.13.1 `tracy-capture` build asset from the project's build-assets release using `curl --fail --location --retry`.
2. Extract into a job-local directory.
3. Find exactly one `tracy-capture`, mark it executable, and normalize its path.
4. Verify it can execute and reports/accepts the expected 0.13.1 protocol version.
5. expose `tracy_capture_tool` as an action output rather than relying only on a global environment variable.

Before coding against the old URL, inspect the `build-assets-2` release and archive locally. The old branch used nested `.tar.gz.zip` assets; do not assume that layout without checking it.

If the existing asset is missing or incompatible, update the manually dispatched `build_tracy.yml` producer to build/package the `capture` component from pinned Tracy 0.13.1, based on the final `tracing_cxx` workflow. Do not execute that workflow in this phase.

### 6. Add the default-off manual workflow input and dedicated job

File:

- `.github/workflows/build_and_test.yml`.

Add a `workflow_dispatch` boolean input:

```yaml
qml_trace_capture:
  description: 'Capture one Tracy trace per QML testcase and upload an archive (Linux)'
  type: boolean
  default: false
```

Thread it through the existing setup resolver with `QML_TRACE_CAPTURE_DEFAULT: 'false'` and a `setup` output. Emit each resolved output once. When trace capture is true, ensure the prerequisites `linux=true` and `release=true` are resolved so checking the trace input reliably creates the Linux release artifact needed by the trace job. Do not enable ordinary test matrices or other platforms implicitly.

Add a dedicated `trace_qml_linux` job with all of these guards:

```yaml
if: >-
  github.event_name == 'workflow_dispatch' &&
  needs.setup.outputs.qml_trace_capture == 'true'
```

The job should:

1. Depend on `setup` and the Linux release build.
2. Run in the existing Linux runtime container used by `release_debian_stable`.
3. Check out the same commit.
4. Download only the release Linux portable artifact.
5. Install it through `.github/actions/install_package` so `COMMAND_SHOOPDALOOP` is populated consistently.
6. Install/validate `tracy-capture` through the new installer action.
7. Create dedicated `traces/qml`, `reports`, and metadata paths.
8. Invoke `.github/actions/test_qml` with capture enabled and the explicit tool/output paths.
9. Regardless of QML pass/fail, write metadata containing commit SHA, run ID/attempt, runner OS/arch, Tracy version, and command configuration.
10. Regardless of QML pass/fail, archive the trace directory, manifest, capture log, QML console log, and JUnit XML into a deterministic `.tar.gz`.
11. Upload that tarball with `actions/upload-artifact@v4`, a unique name such as `qml-traces-linux-${{ github.run_id }}-${{ github.run_attempt }}`, and a finite retention period (14 or 30 days).
12. After the upload step, fail the trace job if no non-empty `.tracy` files were produced. Keep the upload step under `if: always()` so diagnostics remain available.

Normal `test_linux`, macOS, and Windows jobs must not receive the capture inputs. The installer and trace job must be skipped on every non-manual event and on manual runs where the input remains false.

### 7. Update user-facing documentation

Add concise developer documentation in the most appropriate existing developer document (or a small tracing document if no suitable location exists) covering:

- compatible Tracy/capture version;
- local capture command;
- output naming and test-file boundary;
- how to open `.tracy` files;
- how to manually enable the workflow input;
- Linux-only CI capture scope;
- artifact contents and retention;
- the fact that capture is disabled by default.

Update PR #660's description after implementation so it no longer says capture-process integration and tracing CI are excluded.

## Local testing plan

**Do not dispatch or run the new CI trace path. All validation below is local.**

### A. Static and unit validation

1. Run `cargo fmt --all`.
2. Run `RUSTFLAGS="-D warnings" cargo build` as required by project instructions.
3. Run focused tests for the capture controller, including the fake-process lifecycle tests.
4. Run `RUSTFLAGS="-D warnings" cargo test -p frontend --lib`.
5. Run `git diff --check`.
6. If `actionlint` is already available locally, run it against the changed workflow/action YAML. Do not install a large new toolchain solely for this.

### B. Default-off regression

Use a clean temporary directory and run one targeted QML file without any capture options:

```sh
target/debug/shoopdaloop_dev.sh \
  --self-test \
  --test-files-pattern 'src/qml/test/tst_TwoLoops.qml'
```

Verify:

- tests retain their current result;
- no `.tracy` files or trace directory are created;
- no `tracy-capture` child remains.

### C. Single-test capture

```sh
rm -rf /tmp/shoop-qml-traces-one

target/debug/shoopdaloop_dev.sh \
  --tracing-capture \
  --tracing-capture-tool "$(command -v tracy-capture)" \
  --tracing-capture-output-dir /tmp/shoop-qml-traces-one \
  --self-test \
  --test-files-pattern 'src/qml/test/tst_TwoLoops.qml'
```

Verify:

- the six targeted QML test functions still pass;
- exactly one non-empty `.tracy` file exists;
- its name contains a sequence and `tst_TwoLoops`;
- the manifest points to the correct QML source;
- the capture log shows a connection and graceful finalization;
- `pgrep -af tracy-capture` shows no child from the test run;
- the process exits cleanly without Tracy TLS/`atexit` panics.

Open the result in the local Tracy profiler and confirm that frontend spans, log messages, plots, and `frontend_state_update` frame marks are present. If `tracy-csvexport` from the same version is available, use it as an additional noninteractive sanity check, not as a replacement for opening one trace.

### D. Multi-test rotation

Run a pattern selecting at least two small QML files. Verify:

- one and only one `.tracy` file per selected QML test file;
- unique deterministic names in execution order;
- each file is finalized before the next starts;
- the manifest has one successful row per trace;
- no startup-only or trailing timestamp trace appears;
- no capture child remains afterward.

### E. Error paths

1. Pass a nonexistent `--tracing-capture-tool` and verify startup fails nonzero before QML execution with an actionable path error.
2. Use a fake capture process that never connects and verify the connection timeout is bounded, the child is reaped, and the run fails.
3. Use a fake process that ignores graceful termination and verify bounded force-kill/reap behavior.
4. Confirm paths containing spaces work because arguments are not shell-split.
5. Confirm an invalid testcase label cannot escape the output directory.

Do not add a permanently failing QML testcase merely to test artifact retention. The CI archive steps should be structured with `if: always()` and reviewed statically in this phase.

## Explicit CI-testing prohibition for this phase

The executor must **not**:

- run `gh workflow run`;
- dispatch `build_and_test` with `qml_trace_capture=true`;
- create a temporary workflow solely to exercise the trace job;
- claim that trace artifact creation has been validated on a GitHub runner.

It is acceptable to edit the workflow and inspect it locally. If changes are pushed and ordinary default-off PR checks run automatically, they do not count as validation of the trace-producing path; do not enable the input or rely on those checks to claim CI capture success. Leave actual manual CI trace execution for a later, explicit user request.

## Acceptance criteria

Implementation is complete when all of the following are true:

- Capture is disabled by default locally and in every automatic CI trigger.
- `--tracing-capture` implies the Tracy layer is initialized early enough.
- A targeted local QML run produces one valid, openable, non-empty `.tracy` file per loaded `tst_*.qml` file.
- Capture starts only after the external capturer is connected and finalizes before test completion/rotation.
- Trace filenames are sanitized, unique, deterministic, and confined to the configured directory.
- Capture-process errors fail an explicitly requested capture run rather than being reduced to warnings.
- Normal shutdown and per-file rotation leave no `tracy-capture` children.
- Local formatting, warning-free build, unit tests, targeted QML tests, and trace inspection pass.
- `workflow_dispatch` exposes `qml_trace_capture` with default `false`.
- The dedicated Linux trace job exists only for a manual true input and installs a compatible capture tool.
- The job archives traces plus manifest/logs/results and uploads under `if: always()`.
- No CI trace run has been dispatched or claimed as tested in this phase.
- No backend instrumentation, obsolete C++ tracing, or unrelated CI changes are introduced.

## Risks and mitigations

- **Capture connects too late:** wait on `Client::is_connected()` before QML load.
- **New capture inherits the old connection:** wait for disconnect after finalizing before spawning the next process.
- **Truncated trace on exit:** use graceful `SIGINT`, bounded wait, and explicit finalization before `done`.
- **Hung child or full output pipe:** redirect output to a file and enforce stop/connect timeouts.
- **Trace files overwrite each other:** sequence names and create-new semantics.
- **Path traversal from test names:** derive from file stem and sanitize.
- **Requested capture silently missing:** propagate errors and validate non-empty outputs.
- **Artifacts disappear when tests fail:** package/upload steps use `if: always()`.
- **Normal CI becomes slower:** installer, capture args, archive, and job are all guarded by a default-false manual input.
- **Protocol mismatch:** pin external capture to Tracy 0.13.1, matching `tracy-client-sys 0.28.0`, and verify it before running tests.
- **Shutdown-order TLS panic:** never log from `atexit`; keep capture cleanup in normal Rust control flow and preserve the existing `reap_server_atexit` logging restriction.
