# Application client projection

Client-side attachment state, a confirmed shared projection, operation results and
bounded local/encoded synchronization. UI/Lua integration and production entry-point
migration are still pending; existing application startup does not use this client.

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
Lua VM/timers/controller ports belong on the client side when their migration is
implemented. Reattachment must not automatically recreate or start scripts.
