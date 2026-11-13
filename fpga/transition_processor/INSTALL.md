# Indexed action installation and ARM request ownership

Opcode 4 is `INSTALL`. Its index is the caller-selected request slot. Payload
bits 1–32 carry the nonzero generation token expected in that empty slot. The
compact command/result fields and versioned PREPARE guard are specified in
[COMPACT.md](COMPACT.md). Port widths remain 640/832 bits; the internal record
layout and significant wire fields differ from the request-slot milestone.
A successful response returns slot/generation
in the low two u32 words. This remains a fabric interface, not an ARM register map.

`rt_fpga_adapter::transition_service::TransitionService` is the owning admission
layer. Public submission keeps its command/reply types and ignores a caller's
wire index: the service selects the lowest empty, non-retired slot. The service
retains original commands and sequence uniqueness until successful terminal
collection, including after cancellation. Pending collection releases nothing.
Prepare starts generations at one; Collect advances the matching generation,
and overflow retires a slot at zero. Coordinated reset creates a new owner.

The service reserves one transfer at a time. Definitive device refusal leaves the
slot available. Successful installation establishes ownership only after the
acknowledged identity matches. A missing, malformed or uncertain acknowledgement
faults the owner; it cannot safely reuse a possibly installed slot.

## Admission and guard checks

For an idle configured service, public request checks preserve this order:

1. Validate target identity, then positioned play/seek alignment.
2. Validate a timed source identity, then its configured target relationship.
3. Refuse a sequence retained by any pending or terminal request.
4. Select a reusable slot or refuse capacity.
5. Let FPGA clamp/round the requested frame against its actual frame and detect
   overflow, then insert the pending action through its existing list logic.

Raw INSTALL applies hardware checks before mutation:

| Condition | Error |
| --- | --- |
| Unconfigured, faulted or non-idle control phase | `invalid_state` (5) |
| Request index or target slot out of bounds | `invalid_id` (3) |
| Stale target/source identity | `invalid_id` (3) |
| Unaligned positioned operation | `invalid_frame` (6) |
| Source differs from the configured sync relationship | `missing_sync` (7) |
| Zero, retired or stale request generation token | `invalid_id` (3) |
| Matching request slot still occupied, including a terminal record | `invalid_state` (5) |
| Effective timestamp rounding overflows | `invalid_frame` (6) |

The FPGA does not scan for free slots or duplicate sequences. Raw INSTALL is a
trusted lower-level action interface; public duplicate/capacity admission belongs
to the Rust service. Hardware initializes outcome, frame, countdown and list
fields itself and retains execution-time operation validation, arbitration,
supersession, stop cancellation and synchronization.

## Producers and tests

The mono and application adapters use the managed device wrapper. Marlin's direct
transition scenarios use the same service over the raw simulator transport.
The native component and production testbench uses the same owner above its
wire driver, including cancellation and collection. Raw guard tests bypass the
owner deliberately. The production oracle remains a separate path. Python input
generators access the owner through the stateful
`rt-backend-testbench transition-service` JSONL bridge.

Prepared audio and ZedBoard ROM generation reserves distinct slots through that
same Rust owner. Streams that never Collect cannot assume credit reuse; they
check the total reserved count before emitting indexed INSTALL packets. Prepared
streams from the previous submission contract must be regenerated.

Pending lists and result retention remain. Records are compact, and fabric
execution uses an independent start/done path so held host replies cannot gate
already-runnable work. Service-pause tests consume acknowledgements before
pausing the owner. Removing per-quantum fences and host settlement pauses remains later work.
