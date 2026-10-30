# 18. Submitting without allocation

<!-- rust-source: ../../examples/prepared_writes.rs -->

[Contents](../README.md) · [Previous: Capacity, work budgets, and diagnostics](17-capacity-work-budgets-and-diagnostics.md) · [Next: Stopping, disconnecting, and reclaiming ownership](19-stopping-disconnecting-and-reclaiming-ownership.md)

Ordinary `Control::submit` allocates a cancellation flag and belongs on the control thread. `prepare_submissions` lets `submit_prepared` reuse bounded flags instead. Command payloads and their destructors still have separate ownership requirements.

## Run the example

The [prepared writes example](../../examples/prepared_writes.rs) reuses a four-byte upload buffer:

```sh
cargo run -p rt_runtime --example prepared_writes
```

It distinguishes Full from Busy, cancels before dequeue, checks late cancellation has no effect, and submits independent finite recovery while the normal flag is held. A single heap guard covers submission, callbacks, return polling and ownership-preserving recycling. Final construction/destruction and resource publication are outside that scope.

No processor is installed; the empty runtime produces silence while control transfers four known `f32` samples. This is ingress/storage evidence, not an audio-device or deadline demonstration.

## 1. Prepare flags and owning buffers separately

First prepare the runtime, a four-frame mono upload and two four-byte Box buffers. A prepared submit before `control.prepare_submissions()` returns InvalidState with the original request. The example extracts its buffer and checks the pointer is unchanged.

Call `prepare_submissions()` off-render with no outstanding commands. It prepares one reusable flag per lane admission slot. With outstanding work it returns Busy. Repeating preparation later does not revoke flags held by earlier tickets/completions.

The example has one normal credit/flag and one independent recovery credit/flag. It never constructs a Box inside the guarded scope. Its small `write` helper only wraps an existing owner in `Command::resource(Request::Write { ... })`; its `buffer` helper extracts that owner from the returned request.

`submit_prepared` probes at most that lane's fixed flag count. It is not an unbounded retry loop or a multi-producer admission API; the control endpoint still has one producer.

## 2. Preserve the refused value

A submit failure is not an owning completion. `SubmitError` contains both `error` and the original `value`, which must be retained/recycled/disposed under the caller's policy. The example's refusal helper recovers the Box from that value.

Its first command occupies the sole normal admission credit. A second Write returns Full. After the callback cancels the first command, another attempt is **still Full** until the host consumes that unread owning completion.

Do not erase an expected refusal's value merely to propagate its error inside a no-free scope. The example asserts known successful admissions and handles its deliberate pressure cases by extracting every refused owner. A production bounded producer must preserve owners on its ordinary pressure paths too, rather than allocate or spin until admission succeeds.

## 3. Receiving a completion does not release its flag owner

The example receives the cancelled completion but retains it:

```rust
            assert_eq!(completion.outcome, Outcome::Cancelled);
            assert_eq!(control.outstanding(), 0);
            spare = refused(&mut control, write(upload, 0, spare), Error::Busy);
            drop(ticket);
            spare = refused(&mut control, write(upload, 0, spare), Error::Busy);
```

Admission is free, but the completion still owns a reference to the normal cancellation flag—even after dropping the ticket. No new normal prepared submit can reuse that flag yet.

Recovery uses a different range. `Command::ResetMidi` is admitted and completes Applied while the normal completion remains held. A second recovery submit is Full while that credit is occupied. This runtime has no MIDI outputs, so this proves transport independence, **not** downstream reset delivery; [Processing and scheduling MIDI](../part-2-control-a-running-graph/08-processing-and-scheduling-midi.md) demonstrates the latter.

Only after moving the owning request out and leaving the completion's scope does its flag reference go away. The endpoint's base flag owner remains, so these demonstrated ticket/completion drops do not deallocate. This does not make arbitrary generic completion/retirement destructors safe to run under the same guard.

## 4. Reuse the same storage and check cancellation cuts

The recovered first Write was cancelled before dequeue; it wrote no prefix. Four subsequent writes fill offsets 0/4/8/12 with 0.125, 0.25, 0.5 and 1.0. Every return supplies the identical four-byte Box:

```rust
            assert_eq!(bytes.as_ptr(), pointer);
            assert_eq!(bytes.len(), 4);
            assert_eq!(runtime.diagnostics().transfer_bytes, 4);
            drop(ticket);
```

Each iteration cancels its ticket **after** the callback has dequeued/applied the Write. The reliable outcome remains Applied. Reusing a flag requires all external owners to be gone, and submission clears its old cancelled state; a retained old ticket cannot reach a subsequently reused flag.

The normal and spare buffers keep their original addresses. The example stops its finite callback loop, extracts the store, finishes/commits the upload and independently reads back the exact 16 bytes. It releases the snapshot, removes content and checks all declared page/version/snapshot pools recover. Final buffers/endpoints/store destruction occur outside the guard.

Prepared submission is not settlement. Schedule commands still have separate owning completions and eventual execution results; Pending tokens/unread results retain scheduler credit independently of flags. Prepared ingress does not allow structural recovery activation or relax delivery-frame termination from [Application-specific operations](../part-4-coordinate-processors-and-changes/14-application-specific-operations.md).

## Reusing an inactive graph owner

The same ownership principle applies to optional graph materialization. `PreparedPlan::with_capacity(config)` allocates/touches storage off-render. An exclusively owned **inactive** plan supports bounded `begin_descriptor`, `push_step`, `push_connection`, `push_binding` and `set_planning_step`; full capacity returns Capacity without growing vectors. Stamped membership avoids clearing every slot on each begin; stamp exhaustion is explicit.

Materialization is **not validation**. A bad `input_end`, stale physical identity or invalid binding must still be refused by the normal validator/Commit/StageGraph path before execution. Keep the catalog and candidate unchanged during validation. The [lower-level reference](../reference/lower-level-apis.md) describes this distinction; the standalone regression patterns live in [`tests/prepared_ingress.rs`](../../tests/prepared_ingress.rs), not in an invented public framework.

Only mutate a returned inactive owner. A borrowed graph view is not ownership; Ready and Taken owners must use their respective release paths. Descriptor envelope constructors may allocate and may have no spare vector capacity for later pushes. They do not certify safe execution or bypass actual-capacity staging checks.

## Keep local storage local

Flag reuse and Box pointer identity are local Rust ownership properties. They are not serialization, authentication, shared-instance access or a cross-Wasm protocol. Independent instances need bounded checked encoding, local owners and identity/generation routing. Never send raw Box/Vec/trait-object pointers as a substitute for those host boundaries.

[Next: Stopping, disconnecting, and reclaiming ownership](19-stopping-disconnecting-and-reclaiming-ownership.md)
