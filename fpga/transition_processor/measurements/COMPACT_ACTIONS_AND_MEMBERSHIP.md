# Compact actions and ARM-managed membership

This report records the independent milestone 2 checkpoint and the subsequent
ARM-managed membership implementation. [Machine-readable measurements](compact-actions-and-membership.json)
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

## Membership representation decision

Both prototypes ran on the actual resolver, using the same compact records and
execution/reply split. Small directed/seeded and raw publication-guard cases pass
for both. Deployment-capacity comparisons use the same common registered context,
Vivado version, part, capacities and constraints as M1/M2. The frozen prototypes
and manifests remain under `work/fpga-compact/prototypes`; source hashes and
synthesis evidence are included in the machine-readable report. Archived
[linked](compact-actions-and-membership/prototype-linked.patch) and
[flat delta](compact-actions-and-membership/prototype-flat.patch) patches preserve
replay inputs without retaining a selector in production. In a detached checkout
of `708d34a6`, apply the linked patch with `git apply --unidiff-zero`, then
apply the flat delta with the same flag for the flat experiment.
The wire driver includes the full-admission instrumentation used for the stale
workload; it makes no resolver decisions. To reproduce the command comparison:

```sh
# Apply the archived patch(es) in the detached experimental checkout first.
SHOOP_TP_MEMBERSHIP=linked cargo run --locked -p rt_fpga_testbench --features sim -- \
  command-costs --requests 272 --output work/replay-linked
# For the flat-patched checkout, use SHOOP_TP_MEMBERSHIP=flat and a fresh output.
```

Use the ordinary `measure.py --context shared --stage synth` in that checkout for
mapped resources. Both hardware source hashes must match the archived manifests.
These patches are historical measurement inputs, not another installed ABI or
production implementation.

| Prototype metric | Ordered linked nodes | Flat slot rosters |
| --- | ---: | ---: |
| Synthesis LUTs | 12,792 | 12,430 |
| Synthesis flip-flops | 8,556 | 8,683 |
| BRAM36 equivalents | 28.5 | 29.5 |
| Dense R=272 PROCESS | 4,380 cycles | 5,196 cycles |
| Retained terminal self-sync PROCESS | 1,114 cycles | 1,927 cycles |
| Same self-sync after optional compaction | 30 cycles | 30 cycles |
| 8×8 workload peak PROCESS | 2,237 cycles | 2,405 cycles |
| 64-loop chain peak PROCESS | 2,185 cycles | 2,377 cycles |

Choose linked membership: its node and request RAM reads overlap, saving three
cycles per visit, and its two roots per loop consume one fewer BRAM equivalent.
Flat saves 362 synthesis LUTs, but costs 816 extra dense PROCESS cycles and 813
extra stale-entry cycles. Only the linked compiler and resolver remain in the
production sources. Final synthesis/routing is measured again after removing the
prototype parameter and unused states; those changes can alter logic mapping.

Both prototypes also pass the existing 8×8 and 64-member source-chain comparisons
at physical 64/272/400 capacity. Full per-quantum arrays are retained in
[linked](compact-actions-and-membership/prototype-chains-linked.json) and
[flat](compact-actions-and-membership/prototype-chains-flat.json) evidence. To
replay them, copy [the runner](compact-actions-and-membership/prototype-chains.rs)
to `src/rust/rt_fpga_testbench/examples/membership_chains.rs` in the patched checkout,
then run `SHOOP_TP_MEMBERSHIP=linked cargo run --locked -p rt_fpga_testbench --features sim --example membership_chains`
(or `flat`). The effective `FLAT_MEMBERSHIP` parameter is explicit in that runner.

The final contract is [COMPACT.md](../COMPACT.md). ARM prepares complete,
generation-qualified membership in one inactive bank, then atomically commits it
between execution phases. The action is still empty during publication; the final
INSTALL is public admission and rounds against the current device frame.
Publication cannot overwrite countdowns or terminal state. A separate checked u64
stage counter prevents an abandoned writer from reusing a later stage's nonce.
A single synchronous public exchange has at most one outstanding transfer.

Completion/cancel/supersession never edits roots or neighbours. Stale entries use
authoritative valid/generation/outcome checks. Terminal credits and duplicate
sequences remain retained until collection, including when compaction omits their
nodes. No ARM compaction is needed for continued execution. Monotonic physical
slot links bound every walk by R, including malformed traffic; trusted disjoint
forests bound request traversal by 4R visits per quantum, with bounded resolver
work per visit and O(L) loop phases. This is not a full-core deadline bound.

## Command and processing costs

M1/M2 use one transfer per admission. M3 publishes nodes/roots before INSTALL;
measuring only that final indexed write would hide most of the cost. The matched
one-loop workload has physical capacity 64/272/400 and one configured loop. The
last dense admission therefore sends 272 nodes, one root, BEGIN, COMMIT and INSTALL.

| R=272 workload | M1 | M2 | M3 linked |
| --- | ---: | ---: | ---: |
| Empty admission, sum of command work | 16 | 16 | 18 cycles |
| Last dense admission, sum of command work | 1,103 | 1,103 | 1,102 cycles |
| Last dense admission, transfers | 1 | 1 | 276 |
| Last dense admission, elapsed fabric clocks | 1,105 | 1,105 | 1,654 |
| Dense PROCESS | 5,465 | 5,465 | 4,380 cycles |
| Single-ready-action PROCESS | 44 | 44 | 38 cycles |
| Cancel | 11 | 11 | 5 cycles |
| Collect / inspect | 4 / 4 | 4 / 4 | 4 / 4 cycles |

M3 elapsed clocks are measured across the whole synchronous service call with
zero injected stalls. M1/M2 elapsed values are derived from their measured work
plus the same two handshake/acknowledgement clocks for their single transfer.
At R=11 the corresponding last dense work is 59/59/58 cycles, with M3 taking
88 elapsed clocks over 15 transfers. Every dense sample and native service timing
summary is retained in the JSON evidence.

Dense PROCESS improves by 1,085 cycles (19.9%); the complete command exchange gets
slower. With the current fixed-width fabric payloads, 276 transfers carry 22,080
command bytes and 28,704 reply bytes before framing, versus 80/104 for one
transfer. These are interface payload counts, not a physical ARM bus measurement.
The full L=64/R=272 bound is R+L+3=339 transfers per admission. Static prepared
streams publish one boot forest and reserve actual credits once, then deliver
individual actions at their original boundaries. They do not predict completion
or recycle uncollected credits.

The separate M2 checkpoint replay reproduces every published M1 8×8/64-chain
PROCESS count: [M2 arrays](compact-actions-and-membership/checkpoint-m2-chains.json).
M3 reduces peak work, but retained entries can increase later quanta:

| Workload | M1/M2 | M3 linked |
| --- | ---: | ---: |
| 8×8 peak | 2,621 | 2,237 cycles |
| Later 8×8 transition quanta | 1,201 | 1,705 cycles |
| 64-loop chain peak | 2,569 | 2,185 cycles |
| Steady 64-loop chain with retained terminals | 1,033 | 1,609 cycles |

Thus PROCESS is **not uniformly faster** without compaction. The steady chain's
576-cycle increase (55.8%) is bounded stale traversal, not delayed musical work;
every state/event/result still matches production. Optional ARM cleanup improves
that cost but remains unnecessary for correct continued execution.

The retained-terminal workload keeps self-sync playback running with 271 stale
tick entries. Every measured no-compaction quantum is 1,114 cycles. After
reading authoritative terminal status, optional compaction costs three transfers
for the now-empty one-loop view and reduces that workload to 30 cycles. Compaction
does not release any credit. Host preparation uses preallocated O(R+L) storage and
O(R+L) work, and belongs on the command-service thread. Unoptimized native timing
on this shared machine does not qualify an ARM callback or bus-interference bound.

## Combined physical results and qualification

The final hardware is the `708d34a6` source snapshot (verified by exact hashes).
Its production-source synthesis uses 13,340 LUTs, 8,539 flip-flops and 28.5 BRAM
equivalents, with +1.120/+0.063 ns setup/hold. Removing the prototype branches and
states changed synthesis mapping; the lower prototype LUT count is not
substituted for the production result.

| Routed TP metric | Matched M1 | M2 | M3 linked | M1 → M3 |
| --- | ---: | ---: | ---: | ---: |
| LUTs | 11,320 | 11,356 | 13,312 | +1,992 (+17.6%) |
| Flip-flops | 8,148 | 7,898 | 8,813 | +665 (+8.2%) |
| BRAM36 equivalents | 29 | 25.5 | 28.5 | -0.5 (-1.7%) |
| Setup slack | +0.010 ns | +0.030 ns | +0.105 ns | meets 100 MHz |
| Hold slack | +0.047 ns | +0.006 ns | +0.007 ns | meets hold |

The standalone 100 MHz acceptance target is preserved. This is **not a LUT area
reduction**. Action compression and removal of hardware insertion/unlinking reduce
resident data and resolver work, but the two immutable banks, presence masks,
version/identity/bounds guards and bank-read selection add physical cost. Banking
uses three more BRAM equivalents than the M2 checkpoint and almost consumes its
BRAM saving. These costs are included in the same TP hierarchy as the baseline.
Further area optimization requires its own evidence; later roadmap work does not
guarantee LUT savings.

The action/result row is 397 bits, versus M1's 660 and M2's 417. The final
significant INSTALL input is 493 bits in the unchanged 640-bit port; the result is
397 bits in the unchanged 832-bit port. The two immutable membership banks and
presence masks are additional real storage, not hidden result capacity.

Matched complete-core routing uses the unchanged `fpga/synthesis/run.py` harness,
Vivado 2021.1, `xc7z020clg400-1`, 100 MHz / 0.2 ns, four threads and
L=64/R=128/E=256 (SP/JP/program/state depths 512, AXI64, burst memory). This is a
separate system geometry from the standalone TP experiment. Harness/constraints/
Tcl hashes match; M1 sources come from `45d53516`, M3 hardware from `708d34a6`.

| Routed complete core | M1 | M3 | Change |
| --- | ---: | ---: | ---: |
| LUTs | 30,011 | 29,419 | -592 (-2.0%) |
| Flip-flops | 19,327 | 19,502 | +175 (+0.9%) |
| BRAM36 equivalents | 58 | 57.5 | -0.5 |
| Setup slack | -1.353 ns | -1.845 ns | -0.492 ns |
| Hold slack | +0.005 ns | +0.009 ns | +0.004 ns |

**Both complete-core runs miss 100 MHz; the combined change worsens setup slack.**
The reported critical paths remain in the sample-memory DMA logic. The physical
result exposes the integrated impact; it does not isolate one RTL edit as its
cause. The measurement command exits unsuccessfully for those misses and retains
the reports. No such run is counted as passing timing. Complete-core closure and
board/ARM memory-interference qualification remain milestone 6, as scoped in the
plan; the immutable standalone TP 100 MHz gate passes.

## Final local validation

The full native workspace and all Marlin/simulator/application tests ran together:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx,rt_fpga_testbench/sim,shoop_rt_host_fpga_sim/sim \
  --profile ci --test-threads 2
```

That single 3,342-test sweep reports **3,340 passed, two failed, 11 skipped**.
The failures were investigated and corrected with targeted verification:

- A previously interrupted chunk-model build had generated sources but no shared
  library. Remove the inactive incomplete cache and rerun the unchanged complete
  chunk test: all configurations, widths and memory modes pass.
- The continuous bench configured its device directly and skipped the prepared
  boot stream, including the new membership setup. It now sends the same
  `Prepared.membership` transfers before enabling audio. All three continuous
  tests pass, including historical frame/state/media comparisons with stalls and
  overrun. No expected vectors or comparison tolerances changed.

The other 3,340 tests passed, including every simulator/mono/regular-host test,
application replay, retained-take export/reuse, faults, demo recovery, ZedBoard
controls and the complete TP matrix (all five capacities, 100 seeds each). The
long application case records two 240,000-frame takes near store capacity and
passes. Later raw publication/cancel/collect race additions pass directed groups
at all five capacities; the reporting clock metadata now matches the actual
10,000 ps / 100 MHz simulator clock.

The independent stalled production corpus passes 45 cases: 15 directed and
30 seeded. Extended long sessions,
alternate geometry/AXI32, original 64-loop scale, DRAMsim3 streaming, fixed-memory
mixed scale and 16-channel continuous DRAMsim3/stalls pass. The numeric checker
passes 78 saved reference artifacts overall, including the 55 extended cases.
All five capacities pass lint/generic synthesis and default ECP5 mapping. Generated
wrappers/default ROMs, coverage and dependency boundaries check cleanly. The final
workspace build with `RUSTFLAGS="-D warnings"`, simulator/application features,
formatting and Rust test-attribute policy all succeed.

Three integrated 1,024-quantum runs pause all request/membership service after
admission, with ordinary operation, held INSTALL reply, and an incomplete
membership stage. Audio is compared each quantum; resumed results, final state/
ordered journal and captured media match the independent production runtime.
Additional guards cover stale versions, incomplete commits, counter exhaustion,
reset at every stage, collection before/after COMMIT and stale generation reuse.
The explicitly supplied fabric prerequisites are unchanged; these tests do not
claim fence-free progression.

Existing mandatory fences and application settlement pauses remain. Standalone
TP timing does not establish complete-core or board closure. Final CI remains
uninspected by explicit user instruction; neither PR is merged autonomously.
