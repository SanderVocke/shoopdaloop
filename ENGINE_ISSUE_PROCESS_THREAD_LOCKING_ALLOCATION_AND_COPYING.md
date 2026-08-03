# Process-thread locking, allocation, and copying

## Issue

Some non-steady-state engine operations still lock shared state, allocate or resize memory, and copy complete data collections on the process thread. These operations do not satisfy the engine's intended real-time constraints.

## Affected behavior

Known categories include:

- capturing dummy audio and MIDI output;
- legacy/non-application control paths that install channel data without the prepared-generation API;
- executing topology, creation, loading, and response commands;
- callback access to driver registration, connection, capture, MIDI, and plugin-host state;
- constructing or resizing callback scratch and event collections.

## Snapshot-migration status

Production audio/MIDI channel publication now uses bounded preallocated update transport, prepared
load/ringbuffer generations, immutable manifests, and off-thread endpoint retirement. Allocation
guards cover process publication, dense MIDI blocks, prepared installation, and pooled endpoint
destruction. This removes complete channel-content publication and application load installation
from the remaining categories above; it does not resolve the broader command/topology and dummy
capture work tracked by this issue.

## Impact

Lock contention can block the process callback. Allocation and collection growth can introduce unbounded latency. Full-content copying makes process time depend on recording or event-set size. Under load, these behaviors can cause delayed processing or audio dropouts.
