# Full external port graph parity

Implement on top of the shared graph contract. Finish features and local validation
before addressing remote CI failures or merging the PR stack.

- [x] Expand bounded engine host routing beyond 64 ports per direction.
- [x] Add private routing bindings to the provider control interface.
- [x] Implement JACK publication/removal with prepared callback handoff, acknowledgement,
      off-thread registration/reclamation and MIDI cleanup.
- [x] Derive slot capacity from prepared session pools and refuse exhaustion cleanly.
- [x] Automatically publish supported track audio/MIDI and audio-bus outputs.
- [x] Aggregate engine/provider completion; roll back failed creation and clean removal.
- [x] Project observed graph inventory/connections into the existing connection UI.
- [x] Restore saved connection intent and auto-connect behavior, including missing peers
      that appear later and external patchbay changes.
- [x] Preserve master's port naming/lifetime behavior and avoid MIDI bus additions.
- [x] Validate in-process, IPC and TCP behavior with real audio/MIDI, live creation,
      removal, rollback, UI detachment/reconnect, session reload and callback heap guards.
- [x] Push reviewable feature work; leave CI cleanup and merging to the final stage.

Implementation and evidence: `docs/native_session_recovery/port-graph.md`,
`docs/native_session_recovery/jack-graph.md` and
`docs/native_session_recovery/evidence/native-jack.md`.
