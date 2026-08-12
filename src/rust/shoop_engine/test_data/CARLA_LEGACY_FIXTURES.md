# Carla legacy LV2 state fixtures

The `carla_legacy_{rack,patchbay,patchbay16}_loaded_state.json` files were emitted by the removed `CarlaLv2Host::save_state_string` implementation at ShoopDaLoop commit `d903a816` while hosting Carla 2.5.10's installed `carla.lv2` bundle. Before saving, each host restored a Carla project containing the deterministic built-in `audiogain_s` and `midithrough` plugins; Patchbay and Patchbay16 also restored explicit audio and MIDI graph connections. Each host was activated and processed for 100 blocks before saving.

The paired `*_loaded_project.xml` files are the exact Atom String payloads from those saves with the one required trailing NUL removed. Tests compare strict legacy-envelope decoding to these bytes. A real-worker migration test restores a representative loaded Rack envelope through the direct Carla Native descriptor, confirms loaded audio/MIDI processing and project labels, then saves and restores the new chain-tagged state.

The smaller `carla_legacy_rack_state.json` / `carla_legacy_rack_project.xml` pair remains as the original empty-Rack compatibility fixture.
