# Dense Miri timeout: native-verified recovery candidate

Base: `701f6ccd` with the candidate/runner-support changes uncommitted during
verification. The failed full job `20261028T083225-374aa055bf` remains preserved;
this note does not promote its nine partial passes to complete coverage.

Investigation found supplementary off-render assertions repeatedly scanning the
entire event tuple vector for membership, after the complete ordered semantic byte
stream had already been compared. The candidate collects those tuples into a
`BTreeSet` instead. Every membership assertion is unchanged. The prior vector's
membership semantics and set membership agree; order and multiplicity remain
checked by the unchanged full byte comparison. No rendering, packets, frames,
requests, results, partitions, seven replay runs, seed, guard or900s watchdog was
removed or reduced. This is an off-render checker algorithm change, **not a measured
interpreter speedup or proven timeout recovery**. The module is native-only under
`cfg(not(target_arch = "wasm32"))`; production/Wasm Rust is unchanged.

The supervisor now forwards a declared `--shard INDEX/TOTAL` to both frozen inventory
discovery and execution, records it, and checks shard/full-selection/source identity
at collection. Synthetic tests prove identity substitution is refused. Local
`--list --package loop_engine_host_fixture --shard 10/63` selects exactly the failed
`browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled`
with one expected pass/seed0/`-- --exact`. This is the smallest justified retry for
the actual failure, not a routine suite repetition or a waiver of remaining cases.

Native gates inside Nix:57 Python tests,59 fixture tests per profile, locked
warning-denied workspace build, root/fixture formatting, test-usage/boundary/tracing
policies and2141 workspace tests/four existing skips pass. Workspace time33.222s is
warm, not a performance or CI-margin claim. Both native-only profiles rebuild and
pass all17 commands. Complete primitive/graph/export/staging/dense/stall/media/
planner report data compares equal with the prior archived CI-host reports in
both profiles; only wrapper source metadata is excluded from that comparison.

Raw archive: [`dense-miri-recovery-stage7-logs.tar.gz`](dense-miri-recovery-stage7-logs.tar.gz),
60,156 bytes, SHA256
`a06bed18f92d99dc55098bb6acd7772413b38dc08df0960028383d5b43ad3487`.
Root: `target/loop-layer-miri-recovery-stage7/`; includes native gates, exact focused
inventory and implementation patch. Commit the intended inputs before the
supervised retry, then record actual status in [MIRI_JOBS.md](../MIRI_JOBS.md).
No recovery interpreter pass exists yet; Stage7 remains7/8 and the complete
stage/final selection and remote CI obligations remain mandatory.
