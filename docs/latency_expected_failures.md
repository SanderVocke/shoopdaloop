# Scalar latency expected-failure inventory

This file tracks transferred tests whose production behavior is not implemented yet. Entries are removed only after the named subsystem is implemented and the complete lower-layer suite is rerun. Compilation failures, missing-scaffolding panics, hangs, and ignored tests are not acceptable entries.

## Current expected failures

None in the transferred shared-domain group. Later Stage 1 subsystem transfers will add entries here before their production behavior is implemented.

## Resolved groups

| Subsystem | Initial inventory | Resolution evidence |
|---|---|---|
| Shared latency domain | 11 transferred tests: 3 initially green and 8 explicit behavioral failures | Stage 2 now has 13 native and pinned-Node Wasm tests green, including direct scalar-mapping, certainty/identity, trim/bound, recipe, overlap, cue, path, and frozen-status assertions. |
