# Native JACK restoration evidence

Native JACK now drives audio and MIDI directly in the replacement engine. The
fixed startup port bank is shared by application and independent-host launches;
CPAL's JACK host remains a separate CPAL option. Live driver switching remains
open under P14.

## Behavioral coverage

- `shoop_rt_host_native` direct adapter tests verify actual audio samples and MIDI
  offsets at frame zero, the last frame, equal timestamps, and sub-block
  boundaries. Callback heap guards include a canary that detects allocation and
  destruction. Capacity refusals and output-write failures are counted.
- Real JACK peer clients verify the production callback carries audio and MIDI,
  preserves offsets through split callbacks, and delivers recovery before normal
  stop. Further scenarios cover invalid/duplicate port names, failed startup
  connection cleanup, resolved client-name collisions, buffer-size changes,
  missing runtime libraries/server, and bounded cleanup after server shutdown.
- A sample-rate notification test verifies that the initial matching rate is
  accepted and a changed rate invalidates the prepared host. The callback
  silences output while that failure is active.
- The application startup test verifies direct JACK selection, resolved client
  name, and actual sample rate/buffer metadata without CPAL substitution.
- Preparation tests verify that control lead covers the actual JACK server
  period even when the engine renders it in smaller blocks.
- An engine audio regression changes JACK's published period up and down and
  verifies controls retain at least one current period of lead before changing
  output samples.
- The service acceptance scenario runs with both an in-process engine and a real
  independent `shoop_host` subprocess. It records and plays known audio/MIDI,
  verifies peer output, reconnects the UI during playback, reads recorded media,
  saves/loads through the production `.shoop` codec and persistence transfer,
  and verifies fresh identities, retained PCM/MIDI payloads, and routing intent.
- The host-process adapter test verifies custom port names, JACK driver identity,
  and resolved instance metadata over socket and shared-memory setup.
- Profile/codec/settings tests cover client-name-only profiles, port defaults and
  validation, saved settings, appended driver tag 3, and bounded setup messages.

## Reproduction

From the repository's prescribed development environment, with `jackd` and its
dummy backend installed:

```sh
python3 scripts/run_native_jack_tests.py
```

This runner creates an isolated named JACK server, removes
`SHOOP_ALLOW_MISSING_BACKENDS`, verifies readiness, runs the scenarios above, and
captures logs in `target/native-jack`. Failure to provide JACK is a test failure.
The `Native JACK audio and MIDI acceptance` CI job runs the same strict runner
and checks JACK-only, CPAL-only, combined, and driver-free native configurations.
The Linux coverage job also runs it with LLVM instrumentation and merges those
profiles with the retained workspace suite, so real JACK paths contribute to
the existing coverage gates.

The general workspace suite can separately use its established missing-backend
policy for unrelated hardware; it does not replace strict JACK acceptance.

## Compatibility and scope

The engine protocol is version 4, host setup is version 3, and the application
protocol is version 13. These versions combine native JACK with output-bus
parity from the target branch. Existing driver tags remain unchanged. Version checks
reject older peers before exchanging the expanded JACK configuration and
instance metadata. Existing JSON JACK profiles receive backward-compatible port
defaults.

The native host keeps `forbid(unsafe_code)`. A small native-only notification
adapter isolates the JACK trait's required unsafe shutdown declaration; its body
only stores an atomic flag. A control thread publishes server-loss status.

Names in the engine projection use a stable `jack:` prefix, independent of a
resolved JACK client-name suffix. The host registers the configured port bank
once, and Master initially routes to its first audio outputs. External
connections are configured startup pairs or JACK patchbay connections. Port-bank
and sample-rate changes require host restart. Dynamic per-track port publishing,
live driver switching, JACK transport/timebase, and JACK session management remain
outside this restoration.

## Local validation results

- Full warning-denying native workspace build passed.
- Full retained workspace suite after merging output-bus target `e68b15ac`:
  2,562 passed, four pre-existing skips, including the JACK preparation and
  buffer-change regressions.
- Strict JACK runner passed with hardware skips disabled, including both host
  transports and production service/session scenarios.
- All four native feature combinations passed build checks and host library tests.
- All 143 Python script tests passed; formatting, tracing inventory, parity,
  test-attribute, and four-target dependency audits passed.
- Browser application build, generated codec/worklet Node contracts, and hosted
  Chromium core workflow passed. The browser smoke completed with zero xruns,
  including save/load/resave and restored playback at 900×600 (Node v24.19.0,
  Chromium 154.0.8037.57).

## Pull request validation checkpoints

[PR #925](https://github.com/SanderVocke/shoopdaloop/pull/925) targets
`feat/backend-replacement`, including output-bus target commit `e68b15ac`.
Before that target update,
[Source CI at `60262f2e`](https://github.com/SanderVocke/shoopdaloop/actions/runs/41645635124)
passed all scheduled jobs: Linux, Windows and macOS debug/release; browser
debug/release with Node/Wasm, Chromium and Firefox checks; strict native JACK;
coverage; and bare-metal builds. Documentation, CodeQL and both Codecov checks
also passed. Patch coverage is 93.04% against the unchanged 80% gate.

The combined output-bus/JACK source passed the local gates above, including
strict JACK service/session acceptance and the expanded browser bus workflow
with zero xruns. Protocol versions 4/3/13 distinguish the combined wire layouts
from both preceding branches. Current CI results are linked from the PR.

Real JACK provider acceptance uses Linux JACK2 with its dummy backend. The
Windows/macOS results establish distribution build and test compatibility;
JACK runtime providers on those platforms were not exercised.
