# Scalar latency validation runs

Selected environment: repository Nix development shell on NixOS, 2026-08-28.

## Deterministic and aggregate suites

- Complete native workspace with `SHOOP_ALLOW_MISSING_BACKENDS=1`: 1,664 passed, two declared facility skips.
- Complete shared Wasm suite in pinned Node 22.22.2: 17 packages, 1,378 tests, zero failures.
- Complete shared Wasm suite in Chromium/ChromeDriver 147.0.7727.137: 17 packages, 1,378 tests, zero failures.
- The deterministic action matrix runs 44.1/48 kHz and 64/127-frame callbacks, plus mandatory zero/one/callback/loop boundary values. It covers direct, dry, wet, grab, planned render-ahead, dry-into-wet, state/order, provider changes, and ordinary replay. Persistence/resampling and logical/raw I/O have separate exact tests.

## Native providers

- JACK2 real software server, without missing-backend allowance: all nine integration tests passed, including capture/playback ranges, fixed-capacity route retirement, graph changes, and measured one-period external send/return at 64 and 128 frames. No physical endpoints were available, so cable/converter measurement remains the documented facility limitation.
- Pinned patched Carla 2.5.10: zero-latency Rack/Patchbay/Patchbay16, nonzero RubberBand Rack, zero/nonzero branched Patchbay, unpatched compatibility fallback, dynamic bridge publication, and real application worker tests passed. The complete 33-test Carla-selected engine suite passed.
- OxiSynth native/Wasm characterization and compensation tests passed with the declared phase range `0..=63`.

## Browser application smokes

- Trunk 0.21.14 debug hosted bundle built successfully.
- Hosted Chromium output-only AudioWorklet smoke passed at 900x600 with genuine callback progress.
- Self-contained direct-file Chromium output-only smoke passed at 900x600.
- Firefox 150.0.1 under Xvfb passed with 44 callbacks, 5,632 processed frames, 128-frame quantum, zero command overflows, and zero owned media tracks. Geckodriver 0.36.0 emitted its known recommendation for 0.37.1 but completed successfully.

## Latency panel usability

The production `latency_panel_smoke` example was run under X11/Xvfb with llvmpipe at 1000x760. Direct, External, Carla Patchbay, and Built-in Synth fixtures were visually inspected for readable component modes, range selection, signed trim/manual values, cue identity, scalar totals, bounded diagnostics, current/frozen comparison, retained margins, warnings, and consolidation action. No clipping or hover-only dependency was observed:

- `validation/latency-ui-direct.png`
- `validation/latency-ui-external.png`
- `validation/latency-ui-carla.png`
- `validation/latency-ui-built-in-synth.png`
