# Public/native ownership checkpoint

The source overlay and SHA256 manifest freeze this M5 component checkpoint.
`provenance.json` records capacities and scope. The compressed logs retain initial
fixture failures as well as the final passing tests. Non-quantum-aligned test
seek positions were correctly refused; the corrected valid fixture starts
playback and uses aligned positions. No RTL exception was added for that fixture.

| Evidence | Scope |
| --- | --- |
| `public-native-final-tests.log.gz` | Three tests: real public/TP ownership across 1,040 quanta, held host reply, retained originals, guarded writes, native reuse, cancellation/error precedence, full-cache invalidation and initial activation |
| `seal-store-tests.log.gz` | In-place sealing, range/refusal/ownership/cleanup, four memory models |
| `public-checkpoint-regression.log.gz` | Five image/control tests and four capture/observer/record tests; includes continuous audio in three memory modes |
| `public-checkpoint-rom-parity.log.gz`, `public-checkpoint-rom-runners.log.gz` | Shared Rust ROM parity and simulated demo/deployed runners in three memory modes |
| `public-checkpoint-adapter-tests.log.gz` | Shared ARM adapter owner tests |
| `public-checkpoint-warning-build.log.gz`, `public-checkpoint-test-policy.log.gz` | Warning-denying workspace build and test attribute policy |
| `public-native-lint.log.gz`, generic reports | Strict component/default-wrapper lint and generic inference, no inferred latches/division |

Replay from the frozen overlay with the repository's pinned Rust/Marlin and
Verilator setup. Native suites use `cargo test -p rt_fpga_testbench --features sim
--release --test native_actions`; seal uses `--test media capture_views_seal`.
The complete native suite is deferred to the final behavioral milestone, in
addition to these targeted iterative checks. No CI was inspected.

Native public actions are not enabled in continuous production parents here.
Public query/export integration, canonical mutable parameters, initial/graph
composites, actual application service separation, final timing, firmware and
physical qualification remain open. The separate negative-route archive records
older complete-core and M4-wrapper experiments; none qualifies this final system.
