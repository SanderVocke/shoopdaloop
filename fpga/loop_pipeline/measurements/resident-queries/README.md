# Connected resident delivery and historical queries

The source overlay/manifest freeze this M5 checkpoint. `native-queries-*` logs
cover 1,312 quanta, 41,984 exact stereo frames and 2,624 coherent cuts in each of
instant/fixed/DRAMsim3 memory. A 1,040-quantum ARM pause covers two takes, nine
transitions and five state/export snapshots. The shared word reader resumes
without relabeling query frames or capture source metadata. Audio, every ordered
event, post-FINISH states, pre-audio query states and transition outcomes match
independent production Rust. The earlier `native-hooks-*` logs retain the smaller
pre-query checkpoint and initial state-schema mismatch.

`query-checkpoint-components` covers the real public/query/media/record path,
all eight fixture public slots full across 1,050 advancing cuts, immutable scope
checks, full native execution credits, cancellation reasons, word metadata,
record ACK before row reuse, export leases, borrowed cursors and reset. Separate
record tests include all 256 cells with both 29- and 30-word payloads. Four image
component tests and four media/observer/record tests pass. The large earlier M4
image/audio test is not repeated for this iterative checkpoint.

Strict lint and generic inference are recorded for the default and managed
profiles, including CONTROL_DEPTH=256 for managed storage. Formatting, test policy
and the warning-denying workspace build pass. Initial query dispatch failures
and compiler failures remain in separate logs. No CI was inspected.

Mutable canonical parameters, real graph/topology activation counters, bounded
bulk reads, application/mono clock/service separation, near-capacity migration,
ARM/codec integration and final timing remain open. Real-pipeline query fixtures
use their initial scalar/epoch values; separate component tests change inputs
while older snapshots remain retained. This is not M5 completion or physical
qualification. No ZedBoard is available; hardware tests remain the last dependency.
