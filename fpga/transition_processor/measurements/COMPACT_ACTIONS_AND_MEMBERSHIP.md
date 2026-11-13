# Compact actions and ARM-managed membership

This report records the independent milestone 2 checkpoint before the membership
cutover. [Machine-readable measurements](compact-actions-and-membership.json)
include revisions, source and harness hashes, all dense admission samples,
command/processing cycles, native service characterization and report hashes.
The [evidence directory](compact-actions-and-membership/) contains the shared
registered harness/adapter/constraints/Tcl and routed TP reports.

## Milestone 2: compact records and isolated host replies

M1 is `45d53516`; M2 is `47f1fbcb`. Both use Vivado 2021.1, the same
`xc7z020clg484-1`, 64 loops / 272 requests / 400 events, four threads, 100 MHz
and 0.2 ns clock uncertainty. The same registered harness exposes both interfaces;
a constant connection selector connects the execution ports where they exist.
The adapter, harness, constraints and Tcl hashes match exactly. Resource counts
include the TP hierarchy; stimulus/capture registers are outside it.

| Routed TP metric | Matched M1 | M2 | Change |
| --- | ---: | ---: | ---: |
| LUTs | 11,320 | 11,356 | +36 (+0.32%) |
| Flip-flops | 8,148 | 7,898 | -250 (-3.1%) |
| BRAM36 equivalents | 29 | 25.5 | -3.5 (-12.1%) |
| DSPs | 0 | 0 | 0 |
| Setup slack | +0.010 ns | +0.030 ns | +0.020 ns |
| Hold slack | +0.047 ns | +0.006 ns | -0.041 ns |

Both meet the standalone 100 MHz target. The matched M1 resource/timing result
differs from the earlier report because the common measurement interface adds
registered execution endpoints. It is a fresh controlled comparison; the earlier
M1 result is not substituted for this baseline.

Synthesis uses 11,287 versus 11,362 LUTs, 7,886 versus 7,621 flip-flops and
29 versus 25.5 BRAM36 equivalents. A controlled compact-record-only run before
reply isolation used 11,389 LUTs, 7,621 flip-flops and 25.5 BRAM equivalents.
The adopted M2 request cell is 397 bits plus two temporary list pointers,
417 bits at R=272, versus M1's 660 bits. Loop/event widths and capacities are
unchanged. The terminal cell shares the action's reserved storage, guaranteeing
bounded result retention without a drained completion FIFO.

The significant INSTALL input is 429 bits within the unchanged 640-bit port.
Readback carries the compact 397-bit cell within the unchanged 832-bit port.
Rust reconstructs public request fields from its retained original, combining
them with authoritative device progress/results. These are not bandwidth claims
for a physical transport. [The contract and field audit](../COMPACT.md) document
the exact layouts and the versioned PREPARE guard.

Command and PROCESS cycle counts reproduce M1 exactly at both R=11 and R=272.
At R=272: empty install 16, last dense install 1,103, cancel 11, collect 4,
single-ready-action PROCESS 44 and dense PROCESS 5,465 cycles. List ownership is
deliberately unchanged at this checkpoint. BRAM and register costs improve;
LUT savings are not yet demonstrated.

Fabric PROCESS/FINISH now has a separate start/done/error handshake. An unread
host reply cannot consume an internal completion or gate loop reads/runnable
audio. The host-settlement boundary advances independently of reply consumption.
An integrated test holds an INSTALL reply for 1,024 quanta and compares every
audio quantum, final state/events, retained outcomes and captured media with the
production Rust oracle. Additional tests hold cancel/collect/readback replies
with full credits, apply command backpressure, propagate execution faults and
reset. Response stability is monitored on every simulator clock.

All five capacities pass eight regression groups and 20 seeds each. The 45-case
production comparison passes with stalls, production fault cases pass, bounded
regular-host application replay passes, and prepared Rust/Python ROMs agree.
The instant-memory prepared-demo loader/recovery/held-response-drain suite passes.
The warning-denying workspace build and test policy pass. All five capacities pass lint and generic synthesis; default ECP5 mapping
also passes. The interrupted check was resumed from an immutable M2 snapshot.

## Membership checkpoint and remaining qualification

The membership representation and combined M2–3 measurements are added after
their decision and validation gates. Existing mandatory fences and application
settlement pauses remain. Standalone TP timing does not establish full-core or
board closure, and native service samples do not establish ARM callback bounds.
Final CI remains uninspected by explicit user instruction.
