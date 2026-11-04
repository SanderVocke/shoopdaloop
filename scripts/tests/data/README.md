# Loop-host parser fixtures

The `*_frames_native.json.gz` files contain current native replay reports for the
frame-driven composite contract. Replay checks every sample, MIDI packet, receipt
and semantic record against independent scalar tables before these reports are
used by the Python parser/refusal tests. Gzip timestamps are fixed at zero.

The earlier `loop_dense_native.json.gz`, `loop_staging_native.json.gz` and
`loop_stalls_native.json.gz` retain the superseded source-tick contract as
historical evidence. Current tests use the explicitly named frame fixtures.
