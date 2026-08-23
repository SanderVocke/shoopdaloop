# Bundled Carla runtime

ShoopDaLoop dynamically hosts the Carla Native API from the pinned runtime in
`runtime-lock.json`. Carla itself remains able to load the plugin formats built
into that runtime; Carla is not hosted through its LV2 or VST wrappers.

The distributable component contains only the native-plugin library, its
runtime libraries, discovery/bridge helpers, Rack/Patchbay UI helpers, resources,
and license/source metadata. Standalone Carla applications and wrappers that
expose Carla as another plugin are excluded.

`SHOOP_CARLA_NATIVE_LIBRARY` and `SHOOP_CARLA_RESOURCE_DIR` are absolute-path
development and test overrides. Release archives resolve absolute paths relative
to the ShoopDaLoop executable and do not search the working directory or `PATH`.

Carla is GPL-2.0-or-later. The exact corresponding source archive is identified
by URL and SHA-256 in the lock file. Runtime component generation copies this
lock and Carla's license into the component so every application archive carries
the license and corresponding-source information.

## Shoop aggregate-latency adapter

The development/runtime derivation applies `shoop-latency-adapter.patch` to the
pinned 2.5.10 source. It exports ABI version 1 through
`shoop_carla_latency_adapter_version` and
`shoop_carla_query_native_latency`. Rack reports the checked sum of enabled
plugins' `getLatencyInFrames()` values. Patchbay and Patchbay16 traverse reachable audio connections from host input nodes to host output nodes. Acyclic branches publish the minimum/maximum plugin-latency path sum and bounded path count; disconnected plugins are ignored. Feedback, no reachable audio route, arithmetic/path-count overflow, or an ABI mismatch produce unknown/manual-only capability rather than a guessed serial sum while audio remains usable.

Shoop loads both symbols optionally and validates the ABI before use. The bridge
publishes observation revisions and provider diagnostics to control and realtime
views; subprocess status uses protocol version 3 and validates the same bounded
observation. An unpatched 2.5.10 runtime is covered by a compatibility test and
continues processing with unknown latency.

## Known-latency validation

The normal Carla test loads the packaged zero-latency Audio Gain/MIDI Through fixtures in Rack, Patchbay, and Patchbay16, runs audio/MIDI, and checks queried path totals are zero. The Nix development shell also provides RubberBand's fixed-latency LADSPA plugin through `SHOOP_CARLA_NONZERO_PLUGIN_BINARY`; the test generates a serial Rack and a zero/nonzero branched Patchbay fixture automatically. Other environments may instead provide two raw Carla project XML files:

```text
SHOOP_CARLA_NONZERO_RACK_STATE_XML=/absolute/nonzero-rack.carxp
SHOOP_CARLA_BRANCHED_PATCHBAY_STATE_XML=/absolute/branched-patchbay.carxp
SHOOP_REQUIRE_CARLA_TESTS=1 cargo test -p shoop_engine --features carla \
  real_nonzero_rack_and_branched_patchbay_latency_match_impulse_paths -- --nocapture
```

When XML overrides are used, the Rack file must contain one enabled plugin that reports a fixed nonzero latency and passes an isolated impulse. The Patchbay file must route one host channel through a zero-latency path and another through the same nonzero plugin. The test waits for Carla discovery, requires Rack queried latency to equal measured onset, then requires Patchbay range `0..Rack` and impulse onsets `[0, Rack]`. Use the pinned patched runtime; an unpatched runtime intentionally reports unsupported. Run once in-process and once with `SHOOP_CARLA_HOSTING_MODE=subprocess` through the application/worker test environment.
