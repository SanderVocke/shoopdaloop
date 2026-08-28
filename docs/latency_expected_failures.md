# Scalar latency expected-failure inventory

This file tracks transferred tests whose production behavior is not implemented yet. Entries are removed only after the named subsystem is implemented and the complete lower-layer suite is rerun. Compilation failures, missing-scaffolding panics, hangs, and ignored tests are not acceptable entries.

## Current expected failures

| Subsystem | Expected result | Current explicit gap |
|---|---|---|
| Application import/export | Manual scalar offsets, raw audio/MIDI export, and empty-media imports preserve scalar semantics | The new intents compile, but manual offsets and raw formats return explicit unsupported errors until Stage 10 tests are transferred. |
| Worklet media details | Browser audio/MIDI chunks preserve and validate scalar take provenance | Temporary client assembly defaults latency metadata pending Stage 9 protocol transfer. |

## Resolved groups

| Subsystem | Initial inventory | Resolution evidence |
|---|---|---|
| Shared latency domain | 11 transferred tests: 3 initially green and 8 explicit behavioral failures | Stage 2 now has 13 native and pinned-Node Wasm tests green, including direct scalar-mapping, certainty/identity, trim/bound, recipe, overlap, cue, path, and frozen-status assertions. |
