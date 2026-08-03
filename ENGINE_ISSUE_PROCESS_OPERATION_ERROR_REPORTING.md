# Process-operation error reporting is incomplete

## Issue

Queue admission errors are reported to callers, but errors that occur later when commands execute are not represented consistently. Some are logged, some are published asynchronously, and some process-side calls discard their result.

## Affected behavior

- Fire-and-forget operations can fail after their initiating API call has returned successfully.
- Operations without a status object cannot communicate structured completion or failure to their caller.
- Some session calls ignore returned errors.
- Ringbuffer-content adoption reports successful queueing but only logs an execution failure.
- External connection operations can underreport backend rejection while cached state reflects the requested change.

## Impact

Callers can distinguish queue admission failure from success, but cannot always determine whether the requested operation actually completed. Invalid references and backend failures can appear as silent no-ops or exist only in logs. User-visible state can temporarily or permanently disagree with engine state without a structured error attached to the operation.
