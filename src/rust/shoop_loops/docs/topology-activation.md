# Publishing routing, programs, and logical owners

<!-- rust-source: ../examples/configuration_cut.rs -->

Choose the transaction that matches the change. Timing topology, signal routing, logical owner generations and physical processor templates are different things. The facade can coordinate prepared changes; it cannot construct arbitrary processors or resize their ports in a callback.

## Run a stopped-owner configuration cut

The [configuration example](../examples/configuration_cut.rs) records `[0.5,0.25]`, retains an old export, then prepares a newer logical generation with that content and a new capture route through a half-gain effect:

```sh
cargo run --locked -p shoop_loops --example configuration_cut
cargo run --locked -p shoop_loops --release --example configuration_cut
```

Activation refuses while the old owner plays. It also refuses when Stop was merely submitted for the same frame. After Stop settles, one cut publishes owner/media/timing/routing together; a new recording contains `[1,2]` from inputs `[2,4]`, while old and new-initial exports still contain `[0.5,0.25]`.

Whole, single-frame and split music callbacks agree. A zero call at publication changes nothing. Every callback is heap-guarded; preparation, payload construction, owning-result disposal and final destruction stay off-render. This is mono offline evidence, not a MIDI, nested-program or independent-host demonstration.

## Select the publication path

| Operation | Changes | Actual cut/preconditions |
|---|---|---|
| `TopologyActivate` | Timing topology only; exact logical IDs/kinds, current node pairs/media/graph retained | Immediate delivery; changed programs already stopped. |
| `ProgramActivate` | One selected program and nominal length; fixed graph/node pairs/IDs | Pending owning transaction at stopped or actual zero entry. |
| `GraphActivate` | Signal routes; fixed primitive node pairs/decision bindings | Immediate delivery; preserves domain/media state, including continuing recording. |
| `TopologyGraphActivate` | Timing topology and routes; fixed logical IDs/node pairs | Immediate delivery; changed programs already stopped. |
| `ConfigurationActivate` | Stopped logical owner edits, initial media, matching topology and routes | Edited old owners/changed programs already stopped at delivery; clean capture rows/Idle groups. |
| `ProgramGraphActivate` | One selected program/nominal length and routes; fixed logical IDs/node pairs | Pending owning transaction at stopped or actual zero entry; graph rechecked at the cut. |

All require empty payloads and normal-lane admission. Topology header/chunk operations are the separate payload-bearing preparation path. Scheduled admission, runtime execution and typed domain success remain separate results.

## 1. Prepare independent banks and physical templates

`Engine::prepare_with_staging` adds timing receiver banks through `TopologySettings { candidates, records_per_callback, validation_steps_per_callback }`, followed by an optional observer. All zero disables them; enabled settings are positive. Existing constructors leave them disabled.

For logical owner edits, use `prepare_with_configuration_staging`, passing the same ordinary arguments, TopologySettings, NodePairSettings and optional observer. NodePairSettings reserves candidate banks/source slots. All zero disables it; source count/packet allowance must both be zero or both positive, and sources require candidate banks. Timing-only edits can use zero sources.

Supply physical Binding templates off-render. Each active primitive needs its own playback/capture pair. Additional unassigned node pair templates may occupy unused logical slots but stay outside the initial graph. IDs, signatures, channel/MIDI shapes, smoothing and group storage remain prepared/fixed. Effects are also prepared and included in the initial graph, even if initially unrouted. Changing a logical LoopId generation does not replace physical NodeId generations.

Enable graph-owner slots separately with `engine.prepare_graph_staging(count)`, once before the first nonempty callback. Timing staging does not implicitly enable them. Preparation checks retained metadata/scratch/storage products; allocator overhead, compiler temporaries, payloads and other owners are separate costs, not an exact RSS or deadline bound.

## 2. Make timing and graph candidates Ready

Ingest a compiled timing candidate through `TopologyBegin`, ordered complete-record `TopologyAppend` and incremental `TopologyValidate`. The [staging guide](topology-staging.md) defines LTP1; it is timing metadata, not a complete controller/wire protocol. Each header/record is 64 bytes. Generic payload stride/transfer allowance must fit the chosen chunk too.

Record and validation allowances are shared across all banks/operations/internal intervals of one callback. A failed call conservatively consumes the offered allowance. Continue preparation in later callbacks; do not wait/poll a producer or renew the budget per interval. Readiness does not block already admitted music.

Compile signal metadata off-render using the retained control-side catalog:

```rust
    let candidate = control.compile_graph(&graph(true)).unwrap();
    let ticket = control
        .stage_graph(0, Epoch(1), candidate)
        .unwrap_or_else(|e| panic!("stage: {}", e.error));
```

Here `graph(true)` is the example's route-description helper, not a library builder. Compile filters immutable decision bindings to consumers present in that description. Applied StageGraph completion means **Ready**, not active; the changed owning request contains GraphStageId. Refusal/cancellation returns the candidate owner. Actual retained vector capacity is checked, not only populated rows.

GraphStageId/GraphStageStatus are distinct from timing StageId/Status and NodePairStageId/NodePairStatus. Copying them acquires no owner, lock or cross-instance authorization.

## 3. Prepare a logical owner batch when needed

`NodePairSnapshot { resource }` acquires a scoped local source lease. The raw ResourceId requires same-store caller routing; the returned NodePairSourceId additionally carries bank scope/generation checks. Audio is immediately Ready. Indexed MIDI starts Validating; `NodePairSourceValidate` scans packet headers with a callback-shared allowance, including EOF, until Ready reports maximum packet size. Status observes progress; failure stays latched until release. Snapshot/tail-copy acquisition still uses ordinary resource capacity/background allowances.

Begin a bank with `NodePairBegin { slot, changes }`, then append unique-slot edits:

- `previous: None`, `initial: Some(...)`: add an unassigned prepared row.
- Both Some: replace the same slot with a strictly newer logical generation.
- `initial: None`: remove the exact previous owner.

NodePairInitial supplies target, duration, optional audio/MIDI sources, finite gain and `preserve_input`; initial logical revision is zero. Shape/duration/index/packet/current-owner checks precede transfer. Successful NodePairAppend consumes its source IDs into the candidate; rejected append transfers nothing. The example deliberately checks release through a consumed source ID returns InvalidId.

Prepare matching timing IDs/lengths and a graph containing exactly each resulting primitive's prepared pair/bindings. Generic DAG validity cannot prove that domain relationship. Reserve enough **currently free** media metadata for every new initial owner; do not count credits that old-node pair retirement might later free. Old residency, exports and pins remain separate owners.

## 4. Preflight at the actual cut, then publish

The example's payload-free request is:

```rust
    let activate = Operation::ConfigurationActivate {
        node_pairs,
        topology,
        graph,
        graph_epoch: Epoch(1),
        topology_epoch: Epoch(0),
    };
```

Old edited owners and changed programs must already be stopped at delivery. A same-frame Transition::Stop is only registered for later settlement and does not satisfy that check. Capture rows must have no active/prepared/interval/frozen owner, and their groups must be Idle.

Configuration preflight holds a generic graph permit and a media-owner permit across the final fallible resolver/topology publication. After that succeeds, media/graph publication has no ordinary failure path. Failed preflight leaves all candidates Ready and active state unchanged. It is not speculative retirement-credit reuse or general rollback.

Same-frame Play uses the newly published owner/configuration during settlement. State/Export queries observe the post-settlement cut even if delivered earlier at that frame. The example checks graph epoch 2/topology epoch 1, new generation revision zero and refusal of the old LoopId. Context entry epochs are not the returned publication epochs.

`preserve_input: true` retains the existing physical capture projection and is invalid on a previously unassigned row. Otherwise reset precedes the first reservation/consumer call and clears after finish, not every interval. Initial MIDI state is strictly before F; packets at F remain captured originals. Neither setting reconciles arbitrary downstream synth/FX state or sends device cleanup.

## Live fixed node-pair routes and immediate pairs

GraphActivate validates every primitive's playback/capture consumers and exact planner/decision slots, with no missing/extra bindings. It preserves current content/take identities and physical consumer cursors/projections. Routing can change while recording: samples from F follow the new route without restarting the take. It supplies no automatic note/pedal cleanup.

TopologyGraphActivate adds exact-ID/kind timing compatibility. Timing publication is the last fallible step; graph commit follows the preflighted permit. A stopped changed program can then start under the new configuration at that frame. Pure TopologyActivate follows the same timing precondition without changing routes.

An intervening route change invalidates a fixed-graph pending ProgramActivate's expected graph epoch. The old program continues on the independently published new routes; the pending candidate fails rather than executing phantom new zero-actions. Release/restage stale graph candidates against current epochs.

## Owning program requests, with or without a graph

<!-- rust-source: ../examples/program_graph_cut.rs -->

The [program_graph_cut example](../examples/program_graph_cut.rs) supplies the pending recipe: ingest Ready timing and stage Ready routes, start a four-iteration program, then submit this payload-free operation one frame later:

```rust
            Operation::ProgramGraphActivate {
                id: topology,
                target: id(2),
                graph,
                graph_epoch: Epoch(1),
                topology_epoch: Epoch(0),
            },
```

The old route emits 1 until actual zero four frames after start; sample F emits 2, the new program has two iterations, and both returned epochs change. The continuing child retains four cycles rather than restarting. This guarded whole/single/split example uses timing-only node pairs and a separate scalar effect route, not loop-media output. See [program changes](program-changes.md) for run commands and standalone ownership.

ProgramActivate transfers a Ready timing bank into the resolver's single owning program transaction and reserves a normal domain request. Only the selected composite's program/nominal length may change; IDs/kinds, sync/triggers, other programs and node pairs remain fixed. Its timing slot is Taken while pending.

ProgramGraphActivate additionally preflights a compatible Ready graph at admission. The graph remains **Ready and unlocked** while the program waits. A stopped cut or real zero entry qualifies; an idle cursor at zero does not repeatedly qualify, and an idle clock can leave the request pending indefinitely without blocking unrelated music.

At the selected cut, recheck graph scope/generation/readiness/epoch and borrow its permit before real capture preflight. Capture/graph-family refusal drops unused authority and re-previews the old program; each retry adds a new latched structural failure, not an arbitrary subdivision quota. Do not execute old zero-actions merely to discover eligibility.

After successful media/resolver settlement, consume the graph permit before replies/queries/rendering. Sample/event F uses both new configurations. Callback-end F waits for the next nonempty callback; zero calls publish nothing. A pending pair invalidated by independent graph release/reuse/activation fails with program InvalidState; it does not retarget a reused slot or undo that independent publication.

Read ProgramChanged/ProgramGraphChanged as terminal domain results, including their completion and actual working epochs on refusal/cancellation. Requested/effective/delivered/terminal frames differ. The standalone [program transaction](program-changes.md) has no graph authority; raw Transition::Program is not the facade integration API.

## Recycle each owner independently

After successful program finish, internal collection frees its domain slot without waiting for external result consumption. Success retains displaced timing storage until TopologyRecycle, plus a separately Taken graph for a successful pair. Failure/cancellation restores timing Ready; the graph remains in whatever state independent operations left it.

Normal sequence Cancel cancels pending work without silently releasing the graph. Recovery-safe TopologyCancel also requests recycling of the unpublished timing owner: Taken can persist until finish, then becomes Idle. It cannot undo applied publication. NodePairCancel releases unpublished Receiving/Ready edit sources; NodePairRecycle handles Taken node pair metadata. NodePairSourceRelease is separately recovery-safe. None forces graph activation through recovery.

`release_graph` returns unused/displaced graph ownership through a reliable normal completion. Dispose/reuse it off-render. The example separately recycles node pair metadata, topology storage and the retired initial graph while retained old exports remain readable; it later releases every export and checks cleanup twice recovers all declared pools.

## Fault and evidence boundaries

A fault after publication preserves actual epochs/state/time. Do not replay the callback or invent finish/Stop. Consume the engine after callbacks stop; permanent/idempotent Stopped cleanup recovers source/edit/timing/media/capture owners, while generic graph owners are destroyed off-render. Imported resource identities remain caller-owned.

Regression scenarios in [`tests/runtime_topology.rs`](../tests/runtime_topology.rs) and its support modules cover MIDI, program/capture families, cancellation, stale/reused owners, saturated results and post-publication faults beyond this small example. Historical implementation reports and consolidated interpreter evidence live under repository `docs/loop_layer/`; they retain their original source/platform scope. Native example runs are not new Wasm/AudioWorklet, benchmark or deadline evidence. No independent loop host/benchmark is supplied in this snapshot.
