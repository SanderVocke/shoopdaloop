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
plugins' `getLatencyInFrames()` values. Patchbay and Patchbay16 are identified
separately and currently report graph-dependent/unsupported rather than a false
serial sum; feedback, inaccessible routes, overflow, or an ABI mismatch likewise
produce unknown/manual-only capability while audio remains usable.

Shoop loads both symbols optionally and validates the ABI before use. The bridge
publishes observation revisions and provider diagnostics to control and realtime
views; subprocess status uses protocol version 3 and validates the same bounded
observation. An unpatched 2.5.10 runtime is covered by a compatibility test and
continues processing with unknown latency.
