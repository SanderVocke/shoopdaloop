# Independent host fixture (stage 6 design)

Status: implementation next. Prepared submission flags and reusable plan storage are verified; no actual AudioWorklet claim yet. This fixture is test-only, not a production host adapter or a new production abstraction crate.

## Packaging and shared logic

Use a helper manifest under `tests/rt_runtime_host` with its own workspace, `publish = false`, and `cdylib`/`rlib` outputs. Depend on `rt_runtime` through its public API. Wasm bindings are target-specific fixture dependencies, never runtime production dependencies. Reuse the counting allocator from the runtime test support for native and Wasm fixture code.

Share a controller/compiler, bounded wire codec, and render-side bridge between offline, two-thread native, and browser runs. The controller may allocate while compiling/constructing test journals. The render bridge prepares everything before callbacks: runtime slots/resources, cancellation flags, plan pools, chunk payload buffers, input/output arrays, outstanding-request mappings, and response storage. Guard ingress decoding/enqueue, rendering, and completion recycling against allocation/reallocation/deallocation.

A separate main-thread Wasm instance compiles browser plans; the AudioWorklet instantiates the compiled module with independent ordinary memory. Do not pass Rust pointers between instances. Raw pointers may only expose a fixture's own preallocated byte/audio arrays to its local JS wrapper. The wrapper caches views during setup and asserts memory size is unchanged after every callback.

## Proposed finite bounds

Initial fixture configuration:

- 48 kHz, 128-frame quanta; one stable processor with two audio outputs and MIDI input/output. Graph edits select a different audio output without reconstructing it.
- Four normal transferable credits plus one independently reserved recovery credit.
- Fixed 512-byte transfer buffers; at most five ingress frames and five returned buffers per callback. Wire-copy bounds are additional explicit host bounds, not hidden in the core's counters.
- 64-byte resource pages; 4096 pages; 16 resource/version/lease slots.
- 2048 core copy bytes per callback, with 1792 reserved for processor playback/capture; 512 validation/maintenance steps with 16 reserved for processor MIDI capture. Four normal page chunks therefore fit the remaining control copy budget.
- Two private graph-transfer slots, four possible queued graph commits, and one active graph: seven fully prepared plans suffice. Recycle rejected candidates and displaced plans rather than dropping them.
- Prepare payload boxes for the bounded wire payload lengths before starting. Returning completions recycle those owners; even the worklet's control-facing submission uses `submit_prepared`, never ordinary allocating submission.

These are fixture parameters, not changes to A1–A8. Adjust them with recorded evidence if implementation reveals a better small configuration.

## Wire contract

Use a versioned little-endian frame with explicit magic, version, opcode, flags, monotonic sequence, stream-instance epoch, expected graph epoch, absolute frame, declared body length, and credit-slot identity. Resource bodies additionally carry slot/generation, expected revision, and chunk offset. Validate every field/length/count before indexing or mutating staging state. Bound decoding by both frame bytes and declared record counts; reject unsupported versions/opcodes and oversized frames before payload materialization.

Graph transfer has explicit begin/chunk/commit/cancel operations. Begin reserves a private prepared plan and declared step/edge counts. Each chunk must match the exact next offset; decode into fixed temporary records before appending so a malformed chunk cannot partially change the accepted prefix. Commit requires all declared records, then invokes the ordinary expected-epoch/incremental validation/activation path. Staging acknowledgements are not activation success. Resources reuse the core's ordered upload/capture/snapshot/read semantics.

Match wire sequences to core sequences in fixed outstanding slots. A successful enqueue does not return the transferable credit: return it with the reliable terminal result and its owning payload. Malformed/refused frames also return their incoming buffer. Reject duplicate credit use and stale stream identities without altering an existing owner. Normal traffic cannot borrow recovery credit. Cancellation observed before dequeue and cancellation after application remain distinguishable by authoritative completion outcomes.

Scheduling replies distinguish admission/effective frame/lateness from execution. Execution records can be polled through bounded status requests; do not treat future admission as application. Snapshot metadata reports its actual captured cut. All buffers/plan records are relocatable data, never serialized heap addresses.

## Browser execution

Serve fixture assets from localhost with a dedicated basic HTTP handler that does **not** add COOP/COEP headers. Assert `crossOriginIsolated === false`, ordinary non-shared Wasm memory, independent control/worklet instance state, and no memory growth after preparation.

Register a real AudioWorkletProcessor. Its JS MessagePort handler keeps received ArrayBuffers in fixed slots; no unbounded arrays or drain-until-empty loops. The process callback stages at most the ingress limit, calls the guarded Rust runtime, copies its preallocated output into the real Web Audio output, and returns acknowledged buffers by transfer. JS object creation/GC/scheduling is explicitly outside the Rust hard bound; record that limitation rather than claiming hard-real-time JavaScript.

Exercise both OfflineAudioContext and live AudioContext. For deterministic offline journal comparison, suspend/resume at quantum boundaries and wait for ingress-staged acknowledgement before resuming. This coordinates message delivery, not a mock rendering callback. Inspect the actual offline rendered samples. The live case must demonstrate continued callbacks, acknowledged updates, delayed/late messages, transfer saturation, and stop/close; it cannot be replaced by the offline case alone.

Use pinned Chromium/ChromeDriver 147 and wasm-bindgen 0.2.127. A Python runner can drive ChromeDriver through its HTTP WebDriver API using the standard library, collect structured results/browser errors, and fail on timeouts, processor errors, assertion failures, or missing evidence. wasm-pack 0.15.0 can build the test-only helper and obtain its pinned bindings tool. AudioWorklet bindings may need an initialization-only TextDecoder fallback; verify actual browser support before adding one.

## Shared scenario and shutdown

The journal must cover graph staging/activation/rewiring with retained state, exact-frame gain/MIDI changes, late-event reporting, capture and immutable prefix fetching, large audio/MIDI uploads/replacement, stale/malformed/cancelled transfers, held consumers and recovery credit, then release/removal/reclamation. Resources exceed the callback copy budget by at least sixteen times. Compare samples/events and effective revisions/frames for identically admitted journals; do not hide delivery lateness in a hash comparison.

Native two-thread hosting uses bounded nonblocking transport, not a mutex/channel receive inside rendering. Host coordination may wait outside the guarded callback. Include explicit consumer stalls while bounded callbacks continue. Persist journal/results and configurations for later cross-host and benchmark gates.

Stop graph/capture work, remove processors, release their resource leases and exported snapshots, cancel unfinished transfers, and drain bounded reclamation. Report recovered credits/pages before stopping callbacks. Only then destroy Rust owners outside processing. For the worklet, acknowledge a stopped state before an out-of-process-callback dispose operation and AudioContext close. Verify no processing resumes afterward. Retaining an entire live Wasm instance is not a substitute for proving logical pool/credit recovery.
