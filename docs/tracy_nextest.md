# Failure-only Tracy captures under nextest

ShoopDaLoop keeps its ordinary `cargo test -- --test-threads=1` correctness lanes
unchanged. A separate Linux job traces only
`shoop_engine/tests/tracy_collector_contract.rs`. The lane is opt-in:
`SHOOP_TRACY_NEXTEST=1`, nextest's attempt variables, and collector endpoint
variables must all be present before the startup hook does anything. Cargo test,
nextest discovery/listing, and every other test executable neither initializes
Tracy nor contacts a collector.

The job pins cargo-nextest 0.9.116 and tracy-query commit
`b59b6e56db93fd1d8bd9a06d4b348f58717073ab` (the collector implementation under
review in `SanderVocke/tracy-query#1`). Replace that source pin with the first
released collector asset after the upstream PR is merged; the orchestration
command already accepts arbitrary released `--collector` and `--query` paths.
The Rust client is exactly `tracy-client` 0.18.4/`tracy-client-sys` 0.28.0,
compatible with the daemon's Tracy 0.13.1 protocol.

## Architecture

`scripts/run_tracy_nextest.py` starts the suite-scoped daemon and then invokes
cargo-nextest directly. Nextest remains the parent and supervisor of every test;
there is no per-test wrapper. The in-executable support module:

1. checks the explicit opt-in and `NEXTEST_ATTEMPT_ID`;
2. registers run, attempt, binary, test, retry, and stress identity;
3. sets the assigned `TRACY_PORT` before manual/delayed Tracy startup;
4. waits for a completed collector handshake;
5. enables coarse direct engine tracing (detail tracing requires the separate
   `SHOOP_TRACY_DETAIL=1` opt-in).

The profile caps concurrency at one. The initial subset executes real
`BasicLoop` processing and controlled pass, panic, abort, and timeout outcomes.
The timeout is terminated by nextest, not by the collector.

After nextest exits, the script parses authoritative JUnit. One unambiguous pass
is discarded; failure/error outcomes are saved; absent, duplicate, or unknown
outcomes default to save. It finalizes the daemon, validates every saved file
with `tracy-query check`, `range`, `info`, and an attempt-identity message query,
and returns nextest's status. CI passes `--expect-controlled-failures` only
because this fixture deliberately contains failures.

## Run locally

Install cargo-nextest 0.9.116 and obtain matching `tracy-collector` and
`tracy-query` binaries, then run:

```sh
python3 scripts/run_tracy_nextest.py \
  --collector /path/to/tracy-collector \
  --query /path/to/tracy-query \
  --nextest "$(command -v cargo-nextest)" \
  --output artifacts/tracy-nextest \
  --expect-controlled-failures
```

Without `--expect-controlled-failures`, the command propagates nextest's
non-zero suite status. This is the mode to use for a real selected test set.

## Artifacts and resource policy

On `if: always()`, CI uploads only JUnit, finalized `.tracy` files,
`manifest.json`, collector/nextest diagnostics, and `metrics.json`. It never
uploads `.partial` files or captures for successful attempts. `metrics.json`
records traced and ordinary wall time, process peak RSS, finalized trace count
and bytes, nextest status, concurrency, and detail mode.

The first local constrained run used concurrency 1 and coarse tracing:

```json
{
  "baseline_wall_seconds": 0.11,
  "traced_wall_seconds": 18.87,
  "wall_overhead_seconds": 18.76,
  "peak_rss_kib": 113180,
  "trace_count": 3,
  "trace_bytes": 446008
}
```

These fixture numbers include collector startup and intentional timeout. Do not
expand the filter or raise concurrency until CI measurements show acceptable
wall time, CPU/RSS, artifact size, and identical traced/untraced test behavior.
The collector's in-memory captures are not durable across SIGKILL, machine loss,
or runner cancellation.
