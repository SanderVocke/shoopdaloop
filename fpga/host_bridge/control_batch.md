# Staged loop-control batches — mailbox ABI 15

A matching protected generated-audio pair was routed/programmed and passed a
bounded supervised 16/32-loop arming/recording run and two backend restarts.
[Physical results and limits](../../docs/hardware/zturn-v2/2026-11-25/bulk-arming/README.md)
are separate from the simulation measurements below. The tested ABI-14 pair remains
preserved. Application protocol stays 53; the owner requires a matching ABI-15 pair.

## Processing contract

The owner snapshots loop records, prepares edits using the existing transition
resolver, then uploads a bounded compare/masked-write batch while audio continues.
The FPGA accepts at most **32 logical commands / 512 loop-word patches**, with
strictly increasing addresses. A patch carries an expected word, comparison mask,
write mask and replacement. Read-only guards use a zero write mask.

Commit is one dispatcher operation at an idle processing boundary. Before any
mutation it checks completeness, graph revision, frame/deadline, address/width
constraints and **all** guarded words. It then applies all prepared writes locally,
without ARM round trips, releases the boundary and produces one commit response.
Validation refusal performs no active-state writes. Every successful edit shares
the returned frame. A commit consumes the staged batch; repeating COMMIT alone
cannot apply it again. BEGIN explicitly replaces an unfinished staged batch.
Malformed batch operations seal staging rather than leave a valid prefix available
for an accidental later commit.

The existing **16666-cycle** control bound is unchanged and includes both hardware
validation and writes. Unexpected configuration failure during writes faults the
runtime and stops audio; it does not expose a partially updated running program
or claim success. Recovery/reset is required. Hardware stalls still fail closed.

For plan-effective words 10/13, optional rebasing changes the snapshot-relative
frame to the actual commit frame. Overflow/underflow refuses the whole batch before
writes. Pulse skip counts are not decremented during upload. Processing and pulses
can continue while staging; transport fields not replaced by the command are
preserved from the commit-boundary reads.

## Production owner scope

Ordinary groups of **two or more future Transition plans (`now = None`)** use
this path, including the 16-loop extra-sync-cycle recording arm that previously
expired its control window. Duplicate targets are resolved in order in the CPU
shadow, then merged into one set of patches. The group is preflighted in full.
Identity/generation and immutable layout properties are guarded; self-sync mode
and transport-dependent resolver inputs are also guarded where consumed. A
changed guard causes whole-group refusal rather than stale transport overwrite.

Single edits, immediate-`now` controls, gains, mixed command groups and graph
switches retain their existing paths. This is not a claim that every large mixed
control group has been converted. Batch preparation is preallocated for 32 rows;
no new allocation is needed in that owner path. The fixture mirrors the protocol
but is not hardware evidence.

## Wire operations

All fields are little-endian 32-bit payload words; reserved words/bits must be zero.
A deadline value is ignored when its presence flag is unset.
Epoch admission, AXI-safe rejection and exact-tag completion remain unchanged.

- **BEGIN_BATCH (13)**: revision u64, snapshot frame u64, patch count u32,
  logical command count u32, deadline-present bit, deadline u64.
- **STAGE_PATCH (14)**: loop-word address (bits 0–15), rebase flag (bit 16), then
  expected u64, comparison mask u64, write mask u64, replacement u64.
  Rebase is legal only for offsets 10/13 within a 16-word loop record.
- **COMMIT_BATCH (15)**: empty payload. Reply words 0–3 contain commit frame and
  revision. For rejection, word 4 is 1 (frame/deadline), 2 (revision), 3 (guard or
  configuration read), or 0 (incomplete/closed batch). Success word 5 reports
  elapsed control cycles. Faults use the existing fault result and retained record.
- **DISCARD_BATCH (16)**: empty payload; discard staging without state changes.

Datapath reset clears staged ownership. Always-on retained first-fault diagnostics
remain unaffected. The old ABI-13 fault-injection probe expects arming to fault;
do not blindly reuse that expectation to qualify ABI 15.

## Offline regression commands

Run through `nix develop --command`:

```sh
cargo test -p rt_fpga_rtl --features sim,rt-interface --test batch_controls --test batch_dispatch
```

The real AXI pipeline covers 16/32-loop arming, skip 0/1, slow MMIO, and a changed
last target rejecting the entire replacement batch. Dispatcher-level tests cover
continued pacing while staging, a common rebased commit frame, revision/deadline
refusal, malformed/closed staging, frame overflow, the full 512-patch capacity,
read-error refusal, write-error fail-closed behavior and the unchanged control
timeout. CPU preparation also checks capacity, repeated-target merging and
unsupported/missing identities.

### Earlier simulation checkpoint (2026-11-25)

Verilator 5.032, 100 MHz modeled clock; the real pipeline arming cases have audio
turned off, while the dispatcher pacing test supplies independent quantum demand:

| Targets | Pulse skip | Extra modeled cycles per MMIO access | Reported commit control counter |
|---:|---:|---:|---:|
| 16 | 0 | 0 | 1023 |
| 16 | 1 | 20 | 1023 |
| 32 | 1 | 0 | 2047 |
| 32 | 1 | 200 | 2047 |

Including the closing cycle, these are approximately 10.24/20.48 µs locally held
at the modeled 100 MHz, versus the unchanged 166.66 µs bound. The injected gaps
are synthetic (20 cycles = 200 ns, 200 cycles = 2 µs), **not measured ARM latency**.
Validation of an invalid last target uses a different replacement plan, so an
unwanted partial write to earlier targets would be observable. All 17 focused
mailbox/recovery/batch tests passed, plus the two reference rendering tests.
The warning-denying workspace build and complete isolated-JACK Linux suite also
passed (3583 passed, 11 skipped), as did the app WASM check. Both FPGA and fixture
Cortex-A9/watchdog images built; at this earlier checkpoint the private artifacts
had not yet been deployed. Source is based on `e2ff1e882` plus the batch follow-up.

These simulation measurements do not themselves certify physical audio or the
application-to-board workflow. The subsequent linked physical run covers 16/32-loop
future recording arms under the preserved timeout; it is not a soak, arbitrary
mixed-command, physical-output or standalone-SD qualification.
