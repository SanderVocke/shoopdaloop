# 16. Recording resources together

<!-- rust-source: ../../examples/coordinated_capture.rs -->

[Contents](../README.md) · [Previous: Staging and coordinating graph activation](15-staging-and-coordinating-graph-activation.md) · [Next: Capacity, work budgets, and diagnostics](../part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md)

A capture group reserves all members before dependent samples render. Consumers stage complete contributions; the planner publishes one common prefix at the reserved end. Audio and MIDI cannot independently run out halfway through that publication.

The group supplies storage coordination, not loop modes, musical timing, rendering, MIDI restoration or rollback of audio already emitted. Single-resource capture from chapter 10 remains a different contract.

## Run the example

The [coordinated capture example](../../examples/coordinated_capture.rs) records literal stereo samples and complete MIDI packets:

```sh
cargo run -p rt_runtime --example coordinated_capture
```

One planner reserves one-sample intervals for a consumer that contributes both members. Host callbacks use maximum four-frame blocks, single frames and irregular partitions across eight samples. One-sample intervals are this synthetic planner's policy, not a runtime subdivision allowance or performance recommendation.

With 24 data pages, it records eight frames. With six, the next preflight refuses at frame 2: the dependent output changes gain **before sample 2**, while an unrelated output remains 7. Both exports retain exactly `[0,2)`. A four-page host-boundary probe separately refuses the first reservation without replacing old media. Every real callback and required export read is heap-guarded.

## 1. Bound staging, data and lifecycle storage separately

The maximum callback length is four. The group declares stereo audio and at most two MIDI events/six payload bytes per interval:

| Member | Staging bound |
|---|---:|
| Stereo LE `f32` | `4 × 2 × 4 = 32` bytes |
| MIDI records | `2 × 12 + 6 = 30` bytes |
| Total | 62 bytes inside a 64-byte group region |

Bind those `CaptureMember`s in audio/MIDI order using `bind_capture_group`. Each has a fixed region and producer bound; MIDI cannot borrow unused audio bytes or vice versa. The example's producer emits at most one three-byte packet and no recovery traffic. A producer capable of emitting the 32-message recovery sequence from chapter 8 needs bounds that cover that traffic too; these small limits would not suffice.

Staging is not the whole store:

- Data pages are 16 bytes each. Retained old audio and MIDI consume one page each; new interval reservations need additional available pages.
- Eight version slots cover old pinned and new prepared/active versions, including retirement until reclamation. Preparing a take does not reserve unlimited future pages.
- Four indexed-MIDI version banks have four checkpoints each and replay span two: capacity is `4 × 2 − 1 = 7` records per indexed version. Retained old MIDI and the new take use different banks. Every interval preflights the full declared event bound even if its eventual contribution is empty.
- Eight snapshot slots cover two retained old exports, two promised final exports and four extra optional leases. These are independent of versions, data pages and checkpoint rows.

The active configuration uses:

```rust
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 2,
            staging_bytes_per_group: 64,
        },
```

## 2. Prepare a take without replacing old content

`prepare_capture_take(group, initial_states)` reserves each new version, its MIDI initial checkpoint and one final snapshot slot per member. Supply states in **MIDI-member declaration order**, not one per audio member. Here the sole MIDI state holds note 70 at velocity 99; it is initial state, not an invented packet in the recorded stream.

The group becomes Prepared. Old active revisions/data remain unchanged and readable. Group claims prevent legacy replacement, removal, capture start/stop or raw append from bypassing it, including while Idle and after freeze.

The setup also demonstrates abandonment: reserve a prepared interval, observe that cancelling the take is InvalidState, cancel the interval first, then cancel the take. The first cancellation returns PreparedReserved → Prepared and returns page promises; the second returns Idle and releases final-lease/version/index reservations. Reclaim retired metadata before assuming it is reusable.

A fresh take has a different checked sequence. Reusing the cancelled take returns InvalidId. Group identity, take sequence and interval sequence identify different lifetimes; copying any of them acquires no owner.

## 3. Reserve the first interval before activation

The runtime lends `PlannerResourceIo::capture()` with its current frame and external ceiling. Only planning has a future positive sample window; operation/finish contexts do not. The example's selection is:

```rust
            let reservation = if first {
                context
                    .resources
                    .capture()
                    .reserve_prepared(self.take, context.frame, 1)
            } else {
                context
                    .resources
                    .capture()
                    .reserve(self.take, context.frame, 1)
            };
```

A successful first reservation makes the group **PreparedReserved**, still without changing old active revisions/descriptions. Staging before activation returns InvalidState. Activation at any frame other than this stamped/reserved frame is InvalidFrame.

Activate at exactly the reserved frame, then give consumers the **same** interval token. The group becomes Reserved; activation carries its already-reserved pages/credits rather than making a second reservation. In the example the interval sequence remains 2—the earlier abandoned interval consumed sequence 1.

The four-page probe has two pages free after retaining old media. A one-frame stereo append needs one page and the declared MIDI bound needs two. The all-member three-page reservation fails Capacity, leaves zero reserved pages and preserves both old revisions/descriptions. No activation or dependent render follows that refusal.

The probe calls standalone `store.planner_io(frame, ceiling)` while the host owns the stopped store; those stamps are the caller's responsibility. Ordinary processor `ResourceIo` cannot obtain runtime capture lifecycle authority.

## 4. Stage each member once

A validated `DecisionBinding` gives the consumer the planner's interval token and member ordinals. Its ordinary processing view can stage data but cannot activate, reserve, commit or freeze the shared group.

Audio is interleaved and must exactly fit `interval_frames × channels`. The consumer stages its sample and its negation:

```rust
            let [audio, midi] = decision.members;
            block
                .resources
                .render()
                .stage_audio(interval, audio, &[sample, -sample])?;
```

`stage_midi` accepts complete packets with offsets in `[0, interval_frames)`. Even a silent positive interval needs its complete empty MIDI contribution so duration advances. The example records at take-relative frames 0/2/4/6, with note/velocity pairs 60/64, 61/66, 62/68 and 63/70.

Staging copies data into group storage without publishing a prefix. The consumer compares resource metadata before/after both contributions and observes no change. It also checks duplicate audio staging returns InvalidState, and a three-event/nine-byte MIDI probe returns Capacity without consuming the two-event/six-byte promise; the subsequent valid contribution still fits.

Missing, duplicate, malformed, wrong-member, stale or over-bound contributions are interface failures, not ordinary late page exhaustion. An incomplete commit must not publish one member and leave the other behind. A caller must fix/cancel that reservation; it cannot drop a packet and describe the result as coordinated.

## 5. Commit at the reserved end, then freeze the common cut

`finish` commits the interval after both contributions. Its stamped frame equals the reserved end. The store validates the complete set before appending any member, advances their common duration/next frame and returns unused conservative page promises. A repeated commit is InvalidState; using the preceding interval after a fresh reservation is InvalidId.

After every callback, `reserved_pages` is zero. Final free-page assertions match actual audio/MIDI byte lengths, not the larger reserved MIDI bound. The eight-frame export has 64 audio bytes and four 15-byte records; the two-frame prefix has 16 audio bytes and one 15-byte record, including one trailing silent MIDI frame.

At the common stop boundary, `freeze(take, frame, incomplete, output)` publishes the previously promised final leases into one **empty** `Option<Snapshot>` slot per member. Wrong output shape or occupied slots are refused rather than overwriting an owner. Freeze needs no tail copy, new optional lease slot or index rebuild; the group becomes Idle, not permanently frozen or released.

The example occupies every unrelated optional snapshot slot before rendering and consumes all 16 background transfer bytes at each callback's start using old-prefix reads. Optional snapshot acquisition returns Capacity, yet required staging/commit and final freeze succeed. Internal intervals never replenish that background allowance. The final snapshots agree on duration/cut and retain the same incomplete reason.

In the successful case `[0,8)` is committed by `finish` at 8, but freeze at 8 is settled by the **next nonempty callback**. A zero call there does nothing. `finish` is not the next boundary's planning pass.

## Refuse before the affected sample, not after it

With six pages, old exports consume two pages and the first two committed frames consume another two. At frame 2 there are two free pages. Stereo needs one more; the full 30-byte MIDI promise needs two more from the existing 15-byte prefix. Three do not fit, even though the next actual packet would have used less than its bound.

Reservation refuses all members. The planner keeps the common prefix at 2, freezes it with `Some(Error::Capacity)`, clears the interval decision and sets the dependent gain to two. The processor sees that decision before rendering sample 2; its unrelated output continues unchanged. The failure is latched and not retried on every interval.

No rendered frame is replayed or rolled back. A domain coordinating several groups must cancel provisional reservations on refusal before re-previewing its decisions; termination follows from finite prepared structural failures, not an arbitrary musical retry quota.

## Export and release every owner

The example independently checks LE sample bytes, serialized MIDI headers/packets, initial/EOF MIDI states and retained old media. A pinned old snapshot may be Stale and remain readable. State lookup still emits no restoration packets.

After callbacks stop, `into_planner_and_resources()` preserves the ledger containing old, optional and final snapshots. `into_resources()` alone would drop that planner without releasing its copied plain handles. Extraction does not complete a half-rendered reservation, invent a Stop or undo a published prefix.

Release the Idle group explicitly; frozen content and outside leases survive that release. Release each actually acquired snapshot once, remove both resources, and reclaim their retired pages/versions. The example checks all data pages, version slots, snapshot slots, MIDI banks and the group slot recover. Disconnect or ordinary handle destruction supplies none of those release calls.

The safe media path is copying, not zero-copy: input → staging → 64-byte stack scratch → resource pages. These are three counted media copies; checkpoint-state copies have separate diagnostics. Required work is independent from background credit, but finite storage and saturating counters are not a deadline certificate.

[Next: Capacity, work budgets, and diagnostics](../part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md)
