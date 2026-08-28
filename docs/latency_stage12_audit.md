# Scalar latency simplification and realtime audit

This audit records the Stage 12 implementation surfaces. It complements direct tests; it does not replace them.

## Representation inventory

| Boundary | Representation | Reason retained |
|---|---|---|
| Authoritative domain | `shoop_latency::{LatencyObservation, LatencyComponentPolicy, ResolvedLatencyRecipe, ScalarFrameMapping, TakeLatencySnapshot}` | Checked provider truth, policy resolution, and the sole raw/logical mapping semantics; native and Wasm compatible. |
| Realtime engine | `RuntimeLatencyObservation`, fixed-array `RuntimeLatencyRecipe`, `LatchedLatencyRecipe`, atomic publications | Copyable, fixed-capacity callback state with bounded encoding and no strings or heap work. |
| Backend/application | `BackendTakeLatencySnapshot`, app API policy/observation/take states | Stable control-thread identities, diagnostics, optimistic state, and UI-ready values. Recipes are still resolved by `shoop_latency`; these types do not reimplement arithmetic. |
| Worklet and Carla wires | `Wire*Latency*` and `WorkerLatencyObservation` | Versioned, bounded serialization with explicit certainty and unsupported states. |
| Persistence | `TakeLatencyDocument`, `TrackLatencyPolicyDocument`, `LatencyObservationDocument` | Checked wider integer domain for transactional validation and deterministic resampling. |

No runtime, backend, wire, persistence, or UI representation contains an interval/region mapping. The repository architecture regression scans Rust production sources for the forbidden type/field tokens.

## Shared semantics

- `ScalarFrameMapping` implements `raw = logical + media_layout_offset + capture_alignment` and the inverse with checked arithmetic.
- Audio and MIDI channel position helpers use this mapping.
- Realtime cyclic dispatch starts from the same mapping.
- Engine/native consolidation and logical audio/MIDI export use the same mapping; complete-content scans and MIDI state derivation remain control-thread work.
- Fake, engine, native, Web Audio, and worklet paths translate policy shapes but call the shared domain constructors and resolver. Unknown automatic observations remain unresolved; disabled unknown components are the only explicit zero contribution.

## Realtime boundedness

| Path | Bound / evidence |
|---|---|
| Observation publication | Seqlock reads attempt at most 16 times, then return truthful unknown/unavailable state. Direct contention and no-allocation tests cover the fallback. |
| Recipe publication | At most 16 components and 16 coherent-read attempts; no vectors, locks, or callback destruction. |
| Port observation history | Preallocated and capped at 4,096 spans. Full history retires the oldest span before insertion. Selection scans only this fixed bound. |
| JACK routes | 512 fixed route slots, 16 fixed connection slots per route, atomic publication, and no connection-list allocation in the latency callback. |
| Callback loop splitting | Session processing retains `MAX_SUB_BLOCKS`; channel latency dispatch crosses at most the current callback/loop boundary and never scans take content. |
| MIDI ordering | Realtime sorting uses existing preallocated bounded block buffers. Complete-content stable ordering for consolidation/export is off callback. |
| Diagnostics | Fixed counters and fixed plot arrays; no callback logging. |

The engine allocation/lock suites cover recording, playback, routing, processors, snapshots, command application, retained-history updates, and tracing guards. Numeric latency publication has a direct test proving graph request/applied generations do not change.

## Stress and regression surfaces

The retained tests cover maximum compensation and component capacities, rapid policy/revision changes, graph churn and port retirement, processor delay changes, driver switching and resampling, loop transition/wrap/restart, transactional session save/load, malformed documents, and logical/raw I/O. The deterministic matrix runs audio and MIDI at mandatory callback/loop boundary values and includes 44.1/48 kHz with 64/127-frame callback cases.

All 134 reference-added tests are accounted for in `latency_reference_test_inventory.csv`: 124 transferred, seven scalar rewrites, and the three named piecewise tests omitted. The cache invalidation, zero-duration import, and source architecture regressions are present. No ignored or expected-failure latency test remains.
