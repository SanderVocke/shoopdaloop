# 12. Exporting and importing media

<!-- rust-source: ../examples/seek_loop.rs -->

[Contents](../README.md) · [Previous: Programs](../part-5-composites-and-programs/10-programs.md) · [Next: Editing the graph around loops](12-editing-the-graph-around-loops.md)

A loop keeps at most two takes: the one **playing** and the **latest** completed one. When you need a take to outlive that — to save it, hand it to another loop, or query it — you **export** it. Exporting is a control-side request that returns an immutable snapshot handle; the loop keeps playing its own copy. This chapter extends the [seek example](../examples/seek_loop.rs): after the six-frame recording finishes, it exports the latest take.

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin seek_loop
```

## 1. Export a take

Reserve export capacity in `Settings` (`export_slots: 1`) and submit an `Operation::Export`:

```rust
    let export_ticket = control
        .submit_prepared(
            Lane::Normal,
            client.command(
                Frame(10),
                Operation::Export {
                    target,
                    selection: shoop_loops::runtime::Selection::Latest,
                },
            ),
        )
        .unwrap_or_else(|refusal| panic!("export: {:?}", refusal.error));
    let export_sequence = export_ticket.sequence();
```

| `Selection` | Which take |
|---|---|
| `Playback` | The take currently playing. |
| `Latest` | The most recently completed take. |
| `Recording` | The take currently being recorded. |

The execution stream answers with a typed reply carrying the export id, the state at the cut, and the media handle:

```rust
    let Some(Reply::Export { id, cut, media }) = execution.operation_result else {
        panic!("expected export");
    };
    assert_eq!(cut.frame, Frame(10));
    assert_eq!(media.view.description.frames, 6);
    exported = Some((id, media.media));
```

The export is immutable: later recording on the same loop does not change it. Release it with `Operation::Release { export }` when done; unread exports occupy their slots.

## 2. Import media at prepare time

The reverse direction happens before rendering: upload content into the resource store and pass it as `media::Input` to `LoopPlanner::prepare` (chapter 6's example does exactly this for its bass and drums loops via `load_stereo`). An import with `revision: Revision(0)` becomes the loop's initial playback content; the topology's `length` must match its frame count.

## 3. Select an exported take later

`Operation::Select { target, timing, media, preserve_position }` makes a loop switch to a previously exported media id — the same way `coordinated_loop.rs` re-selects its exported original after experimenting. The `media` id must belong to that loop's owner.

## 4. Prepare imported content while rendering continues

File decoding and resampling belong to the host. Upload the resulting canonical
audio/MIDI through the runtime's resource commands; Finish and Commit make a
resource version available, not audible. For MIDI, preserve the initial projection
separately from the original timestamped packets, as explained in
[chapter 9](../part-4-midi/08-midi-state-and-restoration.md).

With node-pair sources enabled in `Options`, `Operation::NodePairSnapshot`
acquires an owned source from the uploaded resource. Its status can be Validating,
Ready, or Failed. Advance `NodePairSourceValidate` across callbacks until Ready;
`source_packets_per_callback` is a shared callback allowance, not a fresh allowance
per query. A source also checks the largest MIDI packet against the destination's
prepared packet capacity. A successful byte upload alone is not that proof.

`Operation::Adopt { target, frames, audio, midi }` creates target-owned media from
Ready source handles. Successful adoption consumes the source ownership; select
the returned media to publish playback, then retire the temporary ledger owner.
On refusal, the caller still owns the sources and must release them with
`NodePairSourceRelease`. Do not release consumed sources a second time. An adopted
medium that was never selected still needs explicit retirement.

Adopt/Select updates an existing owner. Replacing the entire set of owners,
including timing and graph bindings, instead uses the private configuration bank
in [chapter 13](12-editing-the-graph-around-loops.md). Neither path reads files or
provides application-level save/load operation tracking.

[Previous: Programs](../part-5-composites-and-programs/10-programs.md) · [Next: Editing the graph around loops](12-editing-the-graph-around-loops.md)
