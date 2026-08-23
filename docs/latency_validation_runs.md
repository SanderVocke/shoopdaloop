# Latency validation runs

## UI usability pass

The native `latency_panel_smoke` preview (`cargo run -p shoop_egui --example latency_panel_smoke --features latency-panel-preview`) was built and run under X11/Xvfb with software rendering. It cycles one authoritative panel fixture every three seconds. The direct, External, Carla Patchbay, and Built-in Synth captures were visually inspected for clipping, readable component/mode/range/cue controls, signed values, frame/ms totals, diagnostics, frozen/current comparison, warnings, and consolidate action:

- [`validation/latency-ui-direct.png`](validation/latency-ui-direct.png)
- [`validation/latency-ui-external.png`](validation/latency-ui-external.png)
- [`validation/latency-ui-carla.png`](validation/latency-ui-carla.png)
- [`validation/latency-ui-built-in-synth.png`](validation/latency-ui-built-in-synth.png)

The preview and egui tests cover 600×400, 1000×700, mouse-independent standard widgets, no-backend state, missing cue identity, and touch-mode-compatible non-hover controls. The full application could not be used for this visual pass because native ALSA MIDI initialization had no `/dev/snd/seq`; the isolated production panel avoids representing that unrelated host limitation as a UI pass.

## Deterministic action matrix

`latency_characterization` runs the shared audio/MIDI frame oracle on native and Wasm-compatible targets. Its 18 rows cover ordinary playback and record/play, stable/variable grab, insufficient grab, media lead-in, planned prerender, dry-through-wet component variants, stopped/steady/wrap/stop/restart/parallel loops, dry-into-wet audio and MIDI-generated wet audio, first/last/callback/loop boundaries, canonical writes, state ordering, and frozen-take composition. Mandatory frame values include zero, one, `B-1`, `B`, `B+1`, `L-1`, `L`, and `L+1` for callback sizes 1, 7, 64, and 127. Session replay reruns ordinary and dry/wet MIDI timing after same-rate save/load and 48 kHz→32 kHz conversion.

The final manual-equivalent deterministic run additionally executes at 44.1 kHz and 48 kHz with 64- and 127-frame callbacks. Frame-domain behavior is sample-rate independent; frame-to-ms display and resampling have separate exact assertions.

## JACK

The dedicated real JACK software server/provider run executes without `SHOOP_ALLOW_MISSING_BACKENDS`. Latency callback route tests and external send/return measurements pass at 64- and 128-frame periods. The server exposed no physical capture/playback endpoints and `/dev/snd` enumeration tools were unavailable, so the cable/converter loopback is an explicit facility skip. The reproducible physical procedure and acceptance tolerance are in `latency_design_evidence.md`.

## Carla

The pinned patched Carla 2.5.10 derivation was rebuilt from `shoop-latency-adapter.patch`. Zero-latency Audio Gain/MIDI Through fixtures pass in Rack, Patchbay, and Patchbay16. The Nix shell's RubberBand LADSPA fixture reports a fixed nonzero Rack latency; queried latency equals the impulse-response peak frame. A generated branched Patchbay routes one zero path and one RubberBand path; queried range is `0..Rack` and measured peaks match both endpoints. The real application worker repeats the nonzero Rack query/impulse comparison in subprocess mode. The unpatched compatibility runtime remains usable with unknown/manual latency.

Commands and environment are documented in `third_party/carla/README.md`; the Nix shell supplies `SHOOP_CARLA_NONZERO_PLUGIN_BINARY` automatically.

## Final cross-target run

On 2026-08-23 in `nix develop`, both complete shared Wasm suites passed: pinned Node 22.23.2 and Chromium/ChromeDriver 147.0.7727.137 each ran 17 packages and 1336 tests with zero failures. The Wasm application and production worklet builds passed; the application build intentionally uses `--no-default-features` because its default `native-fx` feature is native-only.

A Trunk 0.21.14 debug bundle and self-contained artifact were then exercised by the application crate's three documented browser invocations. Hosted and self-contained Chromium output-only smokes both reached genuine 128-frame AudioWorklet callback progress. Firefox 150.0.1 reached 36 callbacks/4608 frames at a 128-frame quantum with zero overflows and clean teardown. Geckodriver 0.36.0 warned that 0.37.1 is recommended for Firefox 150, but the smoke completed successfully.

Chromium in this environment required an isolated writable `XDG_CONFIG_HOME` because the user's existing crash database caused startup rejection. `scripts/run_wasm_tests.py` now supplies a fresh per-package WebDriver profile/config directory and removes it after each package, preserving deterministic clean browser state.
