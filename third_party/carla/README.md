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

## Latency support

ShoopDaLoop does not query, parse, measure, or infer latency from Carla plugins
or graphs. Carla processor latency therefore has an automatic baseline of zero;
users enter the known delay with Manual or Automatic + trim. Recording-input
alignment remains independently configurable. Carla processing remains
available without latency introspection.
