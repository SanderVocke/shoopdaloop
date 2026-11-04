# JACK adapters

This crate isolates the native JACK APIs requiring unsafe implementations from
hosts that forbid unsafe code.

The signal-style shutdown callback only stores a lock-free atomic flag. Other
notifications forward to handlers; real-time restrictions remain the handler's
responsibility.

`port_id` reads the native registration index used by JACK port notifications. It
is a control-thread metadata query. The function borrows the querying client and
checks that the port came from that client's mapping before calling JACK's UUID
accessors. The native index is not an incarnation or persistent identity: providers
must track registration generations separately. This index never crosses the
portable port-graph contract.
