# 3. Writing processors

[Contents](../README.md) · [Previous: Your first audio graph](02-your-first-audio-graph.md) · [Next: Connecting an audio callback](04-connecting-an-audio-callback.md)

Implement `Processor` to declare ports and process audio/MIDI. Construct the processor before starting callbacks; moving it into a `PreparedNode` transfers ownership to the runtime.

## Ports and audio access

The [first graph example](../../examples/first_graph.rs) declares a mono input and output:

<!-- rust-source: ../../examples/first_graph.rs -->

```rust
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
```

The signature is queried during preparation and remains fixed for that slot. Port numbers within each direction start at zero.

`AudioPorts::pair` borrows an input and output together without constructing temporary buffers:

```rust
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = input * self.0;
        }
        Ok(())
    }
```

Use `input` and `output` individually when a processor has different input/output shapes. Audio views cover the current processing interval, which can be shorter than the host callback because of scheduled actions or planning. Use `block.frame`, `block.frames`, and the supplied slice lengths; MIDI offsets are relative to this interval, not the original host block. Do not retain borrowed views after the call.

## Prepare working storage

A delay needs memory that survives processing calls. The [prepared processor example](../../examples/prepared_processor.rs) uses an optional `PrepareProcessor` implementation to allocate and initialize its delay line:

<!-- rust-source: ../../examples/prepared_processor.rs -->

```rust
impl PrepareProcessor for DelaySetup {
    type Prepared = Delay;

    fn prepare(self, config: PreparedConfig) -> Result<Delay, Error> {
        if self.frames == 0 || self.frames > config.limits().max_frames {
            return Err(Error::InvalidParameter);
        }
        Ok(Delay {
            memory: vec![0.0; self.frames].into_boxed_slice(),
            position: 0,
        })
    }
}
```

The maximum delay here is an example policy, not a runtime restriction on processor storage. Other processors can choose their own prepared capacities. Construct and touch all required memory before rendering; the runtime does not call this preparation trait automatically.

During processing, the delay exchanges samples with its ring buffer:

```rust
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (&sample, output) in input.iter().zip(output) {
            *output = self.memory[self.position];
            self.memory[self.position] = sample;
            self.position = (self.position + 1) % self.memory.len();
        }
        Ok(())
    }
```

The example feeds callbacks of 3, 1, and 4 frames through a two-frame delay. State is retained across those calls:

```rust
    assert_eq!(output, [0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
```

Run it from the repository root:

```sh
cargo run -p rt_runtime --example prepared_processor
```

## Processor types and operations

`GraphRuntime<P>` can use a concrete processor type, an application enum, or `Box<dyn Processor>`. The delay example selects boxed dispatch:

```rust
    let processor: Box<dyn Processor> = Box::new(DelaySetup { frames: 2 }.prepare(config)?);
```

Boxed dispatch allows different implementations in the same runtime. An enum is an alternative when the set of implementations is known. `Processor: Send` permits moving the runtime to its render owner; it does not provide concurrent processor access.

Implement `set_parameter` for numeric control and smoothing, as shown in [Changing parameters at an exact sample](../part-2-control-a-running-graph/07-changing-parameters-at-an-exact-sample.md). [Application-specific operations](../part-4-coordinate-processors-and-changes/14-application-specific-operations.md) cover typed commands. A processor declares its own operation family as the second parameter of `Processor<L, Q>`; it is independent of any planner's operations, and defaults to `()` for processors without typed operations.

For planner consumers, `planning()` declares prepared decision/observation access. An explicit `DecisionBinding` selects the immutable slot borrowed through `block.decision`; `block.observations` stamps the producer and enforces its own reservation. Another consumer's unused allowance cannot be borrowed. These views cannot survive later planner mutation. See [Planning processing intervals](../part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md); ordinary processors need no such declaration.

For required media reads and capture staging, use `block.resources.render()`. Ordinary resource methods use the remaining callback background allowances; a new interval or reborrow does not reset them. See [Loading and reading resources](../part-3-work-with-stored-media/09-loading-and-reading-resources.md) and [MIDI event access](../part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md). Consumers can stage assigned capture data, but the planner retains reservation/commit/freeze authority, explained in [Recording resources together](../part-4-coordinate-processors-and-changes/16-recording-resources-together.md).

## Failures and lifecycle

Processing, parameter and typed-operation handlers must not allocate or free heap objects, lock, wait, perform external I/O, or panic during normal operation. These obligations apply to processor dependencies too.

A propagated processor error generally fails and silences the host block; it does not restore processor state. Handle auxiliary failures locally when unrelated audio should continue—for example, a full recording store. Do not propagate an arbitrary resource `Full` as a MIDI-overflow report. Ordinary MIDI output exhaustion has a specific `Error::Full` recovery contract: finish required audio work and handle reset packets before reporting it. See [Processing and scheduling MIDI](../part-2-control-a-running-graph/08-processing-and-scheduling-midi.md).

Destroy processor-owned buffers only after the processor is returned to the control side or callbacks have stopped. Retained resource leases need explicit release; destroying a processor does not tell the resource store which plain handles it contained.

[Next: Connecting an audio callback](04-connecting-an-audio-callback.md)
