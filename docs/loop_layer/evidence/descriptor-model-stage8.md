# Stage 8 descriptor-bank model — native verified, interpreter pending

## Scope and source

Base: `275d58cd3c1c055311e35bb5547a775bfcbb73aa`, with the complete scoped patch
(including the new Rust file) in the archive below. Runtime and loop production
sources are unchanged. The fixture's parent module adds only a `cfg(test)` module;
no production callback body, API, dependency, manifest, lockfile or workload changes.
The fixture README documents the exact test command and its limits.

This is one additional model slice, not closure of the broader Stage 8 model row.
Stage 7 remains7/8 and Stage 8 remains5/8. Program/occurrence, typed-operation,
resource/capture and wider partition/stress coverage, complete applicable interpreter
evidence and remote CI acceptance remain required.

## Independent model and active guards

Exact case:

```text
graph_transfer::model::seeded_descriptor_bank_model_preserves_complete_tables_and_exact_owners_after_every_action
```

Four deterministic seeds each execute512 mixed actions against both prepared banks,
plus explicit final cleanup and maximum-sequence checks. An allocating off-render
script/state oracle predicts accepted record order and errors independently of the
receiver. It does not call the receiver to obtain expected outcomes. White-box
observations compare every stored step/edge/planning/binding table, epoch, count,
planned flag, generation, phase, global sequence and retained owner after each
mixed action. Storage addresses for all three descriptor/planning vectors stay
identical through loan, refused return, successful return, abort and reuse.

The generator interleaves receiving/sealed/loaned/idle banks, partial and aborted
payloads, foreign sessions, stale identities, replayed full-width sequences,
malformed reserved bytes, bad offsets and oversized counts. Every record family
must succeed somewhere; all six refusal classes must occur. Two scenarios start
one bank at generation `u32::MAX - 1` and require retirement at zero without reuse.
High-bit sequences and epochs remain intact; `u64::MAX` sequence succeeds once and
replay refuses without changing state. Every loan is returned, including after a
wrong-generation owning recycle refusal; both prepared owners remain retained at
cleanup, including retired banks.

Actual allocation/reallocation/deallocation canaries must detect all three heap
operations before testing. Every mutating receiver ingest/take/recycle call is
heap-guarded. Reference allocation, expected-table construction and final destruction
are outside those guarded operations. Seal checks payload completeness only;
this model does not claim semantic runtime graph admission, audio execution or
browser behavior.

## Verified commands and results

Commands ran through `nix develop --command`, with `RUSTFLAGS="-D warnings"` and
`RUSTDOCFLAGS="-D warnings"`. Full commands and tool versions are archived.

- Exact native model: one passed in debug and one in release; no filtered test is
  mistaken for execution of the model.
- Complete standalone fixture: **60 passed/profile**, zero failed/ignored; native
  discovery independently matches all60 registered seed0 identities.
- Workspace warning-denied build and standalone debug/release builds pass.
- Full workspace nextest0.9.116: **2144 passed/four existing skips**,33.730s reported
  suite wall time. This is a correctness run, not a callback timing measurement.
- **72 helper tests** pass, including inventory/dispatch/timeout/background/batch,
  loop-host and benchmark verifiers.
- Formatting, test-attribute policy, closed207-module tracing inventory and both
  four-target dependency-boundary checks pass. Graph checks are not remote builds.

Native gates completed2026-10-28T17:02:11Z. Initial focused and full gates both
passed; no assertion, prior workload, seed, guard or watchdog was reduced.

## Additive interpreter registration and outstanding debt

The default selection is now **147 commands/300 executions =136runtime+100loop+
64host**. The sole addition is this exact one-test seed0 host filter, with
`-- --exact`. Removing it reproduces the prior146-command/299-execution selection
byte-for-byte after JSON normalization, retaining every previous case/count/seed
and relative order. The16 whole-command CI batches retain complete selection checks;
the900-second per-command watchdog and interpreter safety flags are unchanged.

No new interpreter invocation is launched for this smaller model slice. Its case
is registered for applicable consolidated Stage 8/final coverage, **not passed**.
The already active frozen Stage 7 job `20261028T153110-784d2380c0` retains source
`3bf1dbca7c93f486be50cd7359555172dad723f3` and its original146/299 selection.
Supported status observed2026-10-28T17:05:49Z was **running**, not terminal success.
The mutable checkout cannot add this new case to that frozen job. Complete current
coverage and remote source-matched artifacts remain mandatory.

No fresh Wasm build/package unit suite, actual-worklet execution, benchmark,
interpreter pass, remote OS result, deadline guarantee or review is claimed here.
The preceding audit/host results retain their own source and execution boundaries.

## Raw evidence

- Archive: [`descriptor-model-stage8-logs.tar.gz`](descriptor-model-stage8-logs.tar.gz)
- **70,781 bytes;15 files**; every member was compared with its local source.
- SHA256: `c8ea86de8190c0a8ac1d6e63c0e5af25867155a4152a706fd86f2fc56bcc4570`.
- Includes initial/full native logs, discovery, complete old/new interpreter
  inventories, verification, reproducible implementation patch/source proof,
  commands/tool versions and the supported running-status checkpoint.
