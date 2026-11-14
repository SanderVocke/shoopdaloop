# Device-side media cuts (M5 work in progress)

The application capture profile uses `CAPTURE_CHANNELS=1` or `2`, one primitive
loop, 32-frame chunks and reusable physical views. `CAPTURE_CHANNELS=0` preserves
the general kernel's raw media interface and loop count. These are operation
profiles; both use the same TP resolver, store and forward membership layout.
The application/mono Rust consumers have not migrated to this profile yet.

`media_cuts` observes the complete loop row after PROCESS and after FINISH. It
owns eight content cells and four export leases, matching the current adapter.
Before sample execution it chooses playback and allocates any new recording
roots. After FINISH it records the exact completed capture length. A completed
take keeps its actual roots; there is no Slice or walk over its block references
at finalization. Beginning another take creates fresh mutable roots. Removing an
unused completed take detaches those roots and uses the existing bounded
background reclaimer. Validity, lowest-free reuse, checked generations, bank
revision accounting and error ordering remain explicit.

The sample pipeline maps logical channel IDs to the ledger's capture/playback
roots. It range-checks the whole logical ID before narrowing it. Read hints use
playback roots; capture and BeginCapture use the new recording roots. Raw host
media mutation/configuration is disabled in this profile. Completed roots are
private: the eventual bulk-read service must validate an export lease before
reading them and cannot write them. General kernel profiles retain their raw
store operations. The current coordinator runs with observation epoch zero;
final application/transport epoch wiring remains open.

A TP observer has execution/SP priority and uses the existing synchronous RAM
read ports. It returns a complete loop, compact request or event row with its
actual frame and phase, independently of a retained host response. It is a
bounded read of authoritative state, with no second musical resolver. Invalid
indices return zero data and an error. PROCESS/FINISH win over new observations;
an accepted observation completes before the next operation changes its RAM.

The ledger's 1,728-bit cut value is ephemeral until copied by its fabric consumer.
Accepted historical queries must reserve result storage before admission. The
`cut_records` component supplies that storage: 256 cells, 29 payload u64 words
plus a generation/epoch header at word 31. A fabric producer holds its immutable
snapshot through the bounded 30-clock copy. Cells cannot overwrite valid data;
reads and ACKs validate slot, generation and epoch. Full records backpressure new
query admission, never sample processing. Reset invalidates all cells, including
a partial copy. The public action/query controller is not connected yet, so an
optional cut pulse is not a retained public query or an M5 completion claim.

Ledger record fields (little-endian):

| Bits | Value |
| --- | --- |
| 0–831 | Complete TP loop state |
| 832–895 / 896–959 | Actual cut frame / epoch |
| 960–1023 / 1024–1087 | Latest / playback content slot and generation |
| 1088–1151 | Export slot and generation; slot `0xffffffff` means none |
| 1152–1279 | Selected content revision and frames |
| 1280–1343 | Channel 0/1 private physical roots |
| 1344–1471 | Capture start and completion cut |
| 1472–1535 | Snapshot bank revision |
| 1536–1599 | Bank number and reserved flags |
| 1600–1663 | Selected content slot and generation |
| 1664–1727 | Content owner's loop identity |

An empty content ID uses slot `0xffffffff` with a checked monotonically increasing
generation. Empty export leases retain their original revision/generation even
when a later clear changes the current empty identity. A content's references
are export leases; latest/playback are separate roots. Finalization reserves a
free content cell before cleanup, preserving the adapter's capacity precedence.
Service refusals preserve ownership; required cut-time allocation/counter/store
failures contain audio through the coordinator's existing fault path.

The integrated test compares 1,050 quanta, 33,600 stereo sample frames and 2,100
coherent cuts in instant/fixed/DRAMsim3 memory, with no host TP/media/observation
service after setup. Component tests retain leases over repeated retakes, test
empty identities, stale IDs, exhaustion and reset, and bound 240,000-frame
finalization independently of recording length. That long-length component uses
a stalled allocator double; the existing real near-capacity two-take application
test still needs migration and must not be replaced by this shorter sample run.
