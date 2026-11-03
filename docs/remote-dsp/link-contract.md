# DSP processing-link contract

The portable link exchanges planar f32 audio and bounded MIDI, identified by an
incarnation and absolute source frame. It has no network addresses, codec,
process handles or OS clock. All allocation/destruction belongs to preparation
and off-render retirement.

## Prepared shape and latency

`Q` is the logical handoff quantum, `B` the DSP block size, `R` the maximum
runtime processing range, and `H` an additional platform service allowance in
frames. `Q` and `B` need not divide one another. The initial conservative rule is

```
L_min = round_up_to_Q(2*R + B + 2*Q + H)
L >= L_min, L % Q == 0
```

The bound reserves a callback for publication and another for early consumption,
one DSP block of assembly lookahead and two handoff boundaries. `H` is additional
service time, not a replacement for those structural costs. This is an admission
bound, not a minimum achievable latency or a scheduling guarantee.

For example Q=3, B=5, R=7, H=11 admits L=36 frames. A UDP binding chooses Q=P,
B=kP and H=N*P, while retaining its own N>=k and MTU checks. The extra generic
guard is explicit: graph latency is not the original benchmark's N*P allowance.

For source origin S, block j covers [S+jQ,S+(j+1)Q). Its return is consumed at
S+jQ+L. A packet/block's whole result is committed when its first output sample
is needed, including fallback for its remaining samples across later calls.
The graph only drains a configured number of ready results at call entry.
Results arriving afterward wait for another call; already committed blocks are
never patched. Startup before S+L is silent. With algorithm delay A, the reference
is output[n]=input[n-L-A], initially zero. MIDI is delayed by L, not audio-only A.

The descriptor carries Q/B/R/H/L, sample rate/channels, MIDI limits, generation,
source origin, bounded queues/return window and algorithm latency (known/unknown).
Arithmetic and memory products are checked during preparation. Source progress
must remain contiguous; unrelated graph epochs do not change link incarnation.

## Endpoints and ownership

A render endpoint acquires an input lease, submits it, receives output leases,
and releases them. Leases expose bounded logical audio/MIDI views, not their
allocation/mapping representation. Submission refusal returns ownership.
Releasing a valid lease must always have reserved capacity and may not wait,
allocate or destroy its storage. An implementation may use owned prepared blocks
or mapped slots, but never sends Rust ownership pointers across a process boundary.

The supplied in-memory endpoint uses separate bounded ready/free SPSC queues in
each direction. Credits are the actual preallocated buffers. A full queue means
refusal/drop, never a grow or retry loop. A platform owner retains the endpoints
and backing mappings until processing stops and the runtime retires the node.
The processor's final destruction is handled off-render through runtime retirement.

The service assembles Q-sized logical inputs into B-sized DSP calls, and splits
results back into Q-sized returns. Missing source ranges are filled with zeros
and MIDI recovery within a configured work bound; a larger discontinuity retires
the incarnation. A common prepared block executor is also used by the UDP
service, whose packet reorder/gap policy remains in the UDP adapter.

## Failure policy

| Condition | Render behavior | Service/platform behavior |
| --- | --- | --- |
| No input credit | Drop that logical input, count it, keep frame cursor | Later gap is concealed within bounds or retires |
| Missing result at first output sample | Commit silence plus bounded MIDI recovery | Late result is rejected; no deadline extension |
| Concealed/invalid MIDI result | Audio quality is reported; ordinary MIDI is suppressed during reset | Never truncate messages silently |
| Duplicate/stale/out-of-window return | Reject and count, retain earlier valid result | Adapter may retire a persistently invalid peer |
| Service unavailable | Keep graph running with fallback | Owner reconnects/prepares a fresh incarnation off-render |
| Source discontinuity/arithmetic exhaustion | Retire node's incarnation, output fallback | Explicit fresh descriptor/node required |
| Graph route activation | Preserve running link and partial blocks | No implicit remote DSP reset |
| Stop/remove | No joining, unmapping or final storage free in render | Stop service and reclaim after node retirement |

The portable layer does not impose TCP or subprocess lifecycle. The UDP owner
retains its authenticated TCP connection and peer binding. A future IPC owner
may instead retain a process, mapping and control pipe. DMA, cache coherency,
notifications and hardware/network servicing remain platform responsibilities.
