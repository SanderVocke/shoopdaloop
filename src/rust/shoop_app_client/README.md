# Application client projection

Client-side attachment state, a confirmed shared projection, operation results and
bounded local/encoded synchronization. Production UI and local Lua integration
share this confirmed projection.

Local delivery uses the typed channels in the app protocol. Poll `pump_local`
cooperatively and use `submit_local` for nonblocking ordered admission. A failed
queue submission releases its local reservation but never reuses its operation ID.
No command is allowed before a coherent snapshot and a queued Ready acknowledgment.

Encoded delivery feeds validated wire messages to `receive_wire`; `command` creates
one request which the delivery owner must enqueue once. Call `unsent` only when the
carrier refused it before enqueueing. Lost acknowledgments must be reconciled from
service state/results, never by replaying a request. Report connection loss with
`disconnect`, create a fresh transport for attachment, and retain the last projection
only as a stale display. The client state gates further commands.

A local ClientId identifies an actor independently of an attachment generation.
The service must fence every command and shutdown against its current lease;
client-side checks provide projection consistency, not service-side authorization.
Lua VM/timers/controller ports belong on the client side. Reattachment must not automatically recreate or start scripts.

Diagnostic records arrive independently of shared snapshots. `profiling()` exposes
the decoded audio view and `diagnostics()` exposes known host reports plus source
status/freshness. Unknown schemas are skipped without interpreting their payloads.
Assemblies are bounded and attachment/source epochs fence late data. Diagnostic
arrival does not advance the shared session revision. Disconnect clears diagnostic
state; optional producer demand ends when the service lease is detached.
