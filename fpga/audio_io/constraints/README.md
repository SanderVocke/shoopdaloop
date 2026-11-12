# CDC implementation constraints

The sample lane memories have independent synchronous write/read ports. Owners
never access a bank concurrently for conflicting operations. Do not replace
these memories with unsynchronized register buses.

Request/acknowledge and enable/fault/reset synchronizers are explicitly marked
`async_reg`. Preserve both stages and place each chain together. Constrain the
path between synchronizer stages normally. Apply asynchronous-input exceptions
only to the first stage's D pin, not to every register in the destination domain.
This keeps accidental cross-domain paths visible to CDC analysis.

The `rx_sequence`/`rx_epoch` and `tx_sequence`/`tx_epoch` arrays are bundled
mailbox data, held from publication through acknowledgement. Constrain their
source-register-to-destination-consumer paths to less than one destination-clock
period, including skew, so that they settle before the request traverses the two
synchronizer stages. The requests/acks must never bypass their synchronizers.
Review the concrete cell collections after hierarchy changes and use the target
vendor's CDC report to classify these deliberate bundled-data crossings.

`epoch` is a configuration bus: change it only during coordinated session reset
and hold it stable through loading and the entire armed epoch. Assert reset long
enough to be sampled in both domains. Audio reset release traverses two flops;
loading must finish before enable is asserted. Both clocks must keep running.
A physical adapter generates `frame_prepare` two audio clocks before `frame_tick`,
with at least four audio-clock cycles per frame. Startup and rearm use frame zero.

No board pinout, clock period or vendor is selected here. The final board build
must supply actual clocks, synchronizer placement, bundled-data max-delay/skew
constraints and independent-clock memory support. Functional simulation and
memory inference checks do not establish physical CDC signoff.
