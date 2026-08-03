# Object lifecycle and removal are incomplete

## Issue

Pending object creation has explicit lifecycle states, but ownership and removal after an object becomes ready are not fully defined. Dropping the final ordinary frontend handle does not remove the corresponding object from the engine session.

## Affected behavior

- An unreferenced pending handle can cancel creation before insertion.
- Once insertion has completed, final-handle drop does not remove the ready loop, channel, or port.
- Commands queued against an object retain its control until FIFO processing has drained.
- JACK registration records retain port controls independently of ordinary frontend handles.
- A registered port whose insertion never completes can remain pending until driver teardown.

## Impact

Dynamically created objects can remain in the session after they are no longer reachable from ordinary frontend code. Their topology and resources can therefore outlive the handle that appeared to own them. Retained registrations and queued dependencies also make the point at which an object should become closed or removable ambiguous.
