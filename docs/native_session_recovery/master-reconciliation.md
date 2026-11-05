# Backend replacement reconciliation with master

Merge inputs: `feat/backend-replacement` at `540e863f` and `master` at
`9cb1fa16`, with common ancestor `25457f16`. A merge preserves the replacement
branch's 233 commits and incorporates all 11 subsequent master commits.

## Decisions

| Master work | Treatment on the replacement branch |
| --- | --- |
| Carla completion starvation (#894) | Preserve the completion policy, shared-memory fix and tests in the independent legacy engine. Production does not host Carla yet (P07/P08); do not wire the old processor ownership model into the session service. Use the bounded deadline/completion behavior when restoring processors. |
| ZIP 9 compatibility (#900) | Upgrade to ZIP 9.2 and adapt filename validation. ZIP 9 brings JavaScript bindings through its wasm randomness dependency, so build the client codec using `wasm-pack` and initialize its generated bindings in the Worker. Ship and embed those bindings with its Wasm. The RT AudioWorklet keeps its import-free contract; the codec artifact test exercises the complete bindings/ABI bundle. |
| egui ecosystem upgrade (#901) | Carry the coordinated versions, Dependabot group and API migrations. Migrate additional capability storage, notification and recovery-test IDs introduced on the feature branch. |
| CPAL 0.18.2 compatibility (#902) | Carry the old driver's fixes and adapt the replacement native host and software-host tests. Stream configurations pass by value; errors use `ErrorKind`. Xruns remain recoverable, route-change/realtime-denial notifications do not fail the stream, and unavailable/invalidated devices remain failures. |
| ehttp, clap and lazy_static updates (#903–#905) | Carry the lockfile updates without changing the replacement architecture. |
| Tracing UI test fonts (#908) | Carry the test initialization fix. |
| Settings, composite and piano guidance (#911) | Carry the UI improvements while preserving capability checks. Timing controls move into Advanced timing and remain disabled when unavailable. Keep startup-only host settings semantics instead of advertising live driver switching in production. |
| Loop range editing (#895) | Carry zoom/fit, lanes, grid/snap, single/all channel tools and tests. Port timing implementation and behavioral tests into `shoop_app_reference` and the legacy backend, where those implementations now live. Add the new intent to both capability gates. Production P12 is complete through authoritative timing commands, RT windows/pre-play, persistence and recovery; see [acceptance evidence](evidence/loop-timing.md). Logical periods retain the fixed-Q alignment qualification. |
| Progress-based browser trace smoke and details tests (#913) | Carry the corrected track-list test calls. Apply the trace record threshold and timeout to the supported local UI capture. The removed Worker/WebAudio controller types and external worklet trace realms are not restored. Retain the old browser implementation fixes in the reference sources. |

The MIDI details view now receives stored length and capture alignment from the
service rather than guessing from event positions. The application protocol moves
from version 10 to 11 because this changes its serialized media shape; host and UI
must be rebuilt together. Typed and serialized media projection tests cover the
metadata. This does not enable timing mutations.

The old Carla worker-entry canary stays retired: it tested the removed launch
architecture. Independent Carla tests remain, and future production integration
continues under P07/P08. Current production documentation and the existing parity
ledger remain authoritative; this merge does not close a parity gap.

## Validation

- `RUSTFLAGS='-D warnings' cargo build --locked --workspace`: passed.
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci`: 2,533 passed, 4 skipped. Local networking was enabled for UDP, socket and LAN-host tests. Real unavailable host audio/plugin facilities retain the suite's explicit allowances.
- Formatting, test-attribute policy, closed tracing inventory and production dependency boundaries: passed.
- Warning-denying `wasm32-unknown-unknown` builds of the app, exported RT worklet and codec worker: passed. The app/service and RT engine also pass no-default-feature checks for this target.
- Node AudioWorklet artifact/MessagePort contract and codec worker bindings/ABI contract: passed.
- Hosted and single-file Chromium core workflows at 900×600: passed, including audio/MIDI recording, media details, save/load/save and resumed playback, with zero xruns. Chromium Perfetto tracing smoke: passed. Browser checks used available Node 24.19.0.

The single-file ZIP 9.2 check uses `SELF_CONTAINED_URL` on localhost with only the
HTML served; this environment's Chromium policy blocks `file://` navigation.
Hosted archive and single-file verification check that the codec bindings are
shipped and embedded exactly. The artifact contract initializes the codec through
those bindings, while the separate RT worklet still rejects every import.

The final native run disables debug symbols and incremental compilation to fit the
environment's disk budget; warning denial and the CI test profile remain enabled.
