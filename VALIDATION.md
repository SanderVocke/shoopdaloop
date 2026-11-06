# Testing

Use the environment described in [.agents/info/build.md](.agents/info/build.md).
On Nix/NixOS, run these commands inside `nix develop`; elsewhere use the configured
local or CI toolchain. Run focused package/tests during development and the
applicable broader gates for behavior changes. This page describes how to check
an implementation; it does not certify a particular revision.

## Native workspace

```sh
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_feature_parity.py
python3 scripts/check_dependency_boundaries.py --scope runtime --scope loops --scope engine
python3 -m unittest discover -s scripts/tests
```

The test profile requires cargo-nextest 0.9.116. Omit
`SHOOP_ALLOW_MISSING_BACKENDS=1` when exercising real host facilities. Hardware
acceptance needs the relevant device or server; a skipped device test is not
hardware qualification. See the [application guide](src/rust/shoopdaloop/README.md)
for packaged application and JACK smoke commands.

## Runtime documentation and independent hosts

```sh
python3 scripts/check_rt_runtime_docs.py
python3 scripts/check_rt_runtime_docs.py --run-examples
python3 scripts/run_rt_runtime_hosts.py --native-only --profile debug
python3 scripts/run_loops_hosts.py --native-only --profile debug
```

Repeat relevant hosts/examples with `--profile release`. The
[runtime fixture](tests/rt_runtime_host/README.md) and
[loop fixture](tests/loops_host/README.md) document replay, browser, failure and
resource checks. The documentation checker verifies links and source excerpts;
its example mode executes the crate's examples with their assertions.

## Browser and Wasm

Follow the [application build and smoke guide](src/rust/shoopdaloop/README.md)
and [Wasm runner](scripts/run_wasm_tests.py). Build the application and RT worklet
for `wasm32-unknown-unknown` before running application smokes. Compile-only checks
and native fixture replay do not exercise an actual AudioWorklet. Use the host
fixture instructions for non-isolated offline/live browser runs and compare full
sample/MIDI/journal output; verify no render-time memory growth.

## Miri and measurements

The selected checks and command-line options are available with
`python3 scripts/run_rt_runtime_miri.py --help`. The
[background runner guide](docs/loop_layer/MIRI_BACKGROUND.md) describes supervised
runs. Keep the existing workloads, seeds, ownership assertions and watchdogs;
timeouts and partial results remain incomplete checks. Chrome and Miri runs are
manual under the existing CI capacity decision.

Release benchmark examples live with their owning crates. Report workload,
configuration, callback tails, deadline misses, memory and exhaustion behavior.
Finite work and allocation guards do not establish physical-device deadlines.
Recording normally shares a finite 256 MiB prepared media pool across loops;
exhaustion ends capture, so stress fixtures must budget for their recording duration.

## Results and remaining limits

Put generated logs, provenance, reports and measurements under ignored `target/`,
`artifacts/` or `work/`, or upload them as CI artifacts. Put revision-specific
results and review links on the PR. Do not maintain committed test-count lists,
module tracing dispositions or chronological pass/fail ledgers. Keep reproducible
commands and meaningful behavior/capacity constraints in the owning guide.

Open application gaps and physical USB qualification remain in [PLAN.md](PLAN.md).
Historical evidence is available in Git history; it does not establish checks on
the current implementation. No behavioral test, Miri workload or host integration
gate was retired by the documentation cleanup.
