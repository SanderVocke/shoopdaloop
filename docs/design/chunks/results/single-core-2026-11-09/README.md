This session began 2026-11-09 UTC and continued on 2026-11-10. Read
[the results overview](../../SINGLE_CORE_SEARCH.md) and
[the experiment log](../../OPTIMIZATION_LOG.md) for retained solutions, rejected
approaches and measurement limits. All performance timings use one CPU on an
AMD x86 VM. ARM64 artifacts establish compile/functional checks; physical Pi 5
timings remain unavailable.

Raw callback/micro CSV.gz files are preserved byte-for-byte inside
`raw-measurements.tar.gz`. Its index records each original name, byte count and
SHA256. Extract with `tar -xzf raw-measurements.tar.gz`.

Build manifests record compiler, flags, source/harness and immutable binary
hashes. JSON summaries identify each rotated comparison. Patches labelled rejected
or contender are evidence and are absent from the retained implementation.
`borrowed-route-evidence.json` contains the final freshly trained host result.

Profile archives retain merged PGO data, training/build drivers, callback records
and manifests. Per-process `.profraw` files and raw sampled stack memory remain
in the ignored workspace. `archive-pruning.json` records their original hashes
and the smaller archive identities; retained members are unchanged. Symbolic
stack text and flat CPU-clock reports remain committed. Recorded `work/` paths
refer to the experiment workspace; extracted tools and recorded build commands
can recreate those paths. The runnable Pi bundle is workspace-local; its build
manifest and runner are archived, and no binary is claimed to have run on a Pi 5.
