# Shoop Lua compatibility contract

This document defines the Shoop Lua compatibility target for the native and browser application. Both use the pinned omniLua 0.7.1 Lua 5.4 runtime described in `omnilua_runtime.md`. The semantic version handshake and script-owned dialog API are specified in `lua_dialog_api.md`.

## Value and selector rules

- A loop coordinate is a two-integer Lua sequence `{track, row}`. Main tracks are zero-based; the sync loop is `{-1, 0}`.
- A loop selector is one coordinate, a sequence of coordinates, or `nil`. Missing coordinates select no object. Results follow current track/row order.
- A track selector is a zero-based integer, a sequence of integers, or `nil`; track `-1` is the sync track. Missing indices select no object.
- Setters accept exactly the documented argument count. Unsupported selector/value types are errors. Setters return `nil` unless noted.
- Multi-object getters return Lua sequences in selector order. A singular-looking selector still produces a sequence for getters documented as `list[...]`.
- Mode, event, key, and modifier values are integers. `nil` represents an absent queued transition or target.
- Gain-factor APIs use linear amplitude. Balance is clamped to `[-1, 1]`; fader positions are clamped to `[0, 1]` and use the same conversion curve as the application controls.

## `shoop_control` function inventory

| Family | Signature | Result or effect |
|---|---|---|
| Loop query | `loop_count(selector)` | Integer number of existing selected coordinates. |
| Loop query | `loop_get_all()` | All loop coordinates. |
| Loop query | `loop_get_which_selected()` | Selected loop coordinates. |
| Loop query | `loop_get_which_targeted()` | Target coordinate or `nil`. |
| Loop query | `loop_get_by_mode(mode)` | Coordinates whose current mode equals `mode`. |
| Loop query | `loop_get_mode(selector)` | Sequence of current modes. |
| Loop query | `loop_get_next_mode(selector)` | Sequence of queued modes or `nil` entries. |
| Loop query | `loop_get_next_mode_delay(selector)` | Sequence of queued cycle delays or `nil` entries. |
| Loop query | `loop_get_length(selector)` | Sequence of lengths in frames. |
| Loop query | `loop_get_by_track(track)` | Coordinates in the selected track. |
| Loop transition | `loop_transition(selector, mode, cycles_delay, align_to_sync_at)` | Explicit transition. `Loop_DontWaitForSync` and `Loop_DontAlignToSyncImmediately` disable the corresponding behavior. |
| Loop transition | `loop_trigger(selector, mode)` | Applies the same sync, solo, fixed-cycle, target, and play-after-record policy as the GUI trigger. |
| Loop transition | `loop_trigger_grab(selector)` | Applies the same ringbuffer-grab policy as the GUI grab action. |
| Loop query | `loop_get_gain(selector)` | Sequence of linear output gains. |
| Loop query | `loop_get_gain_fader(selector)` | Sequence of fader positions. |
| Loop query | `loop_get_balance(selector)` | Sequence of stereo balances. |
| Loop transition | `loop_record_n(selector, n_cycles, cycles_delay)` | Queues synchronized fixed-length recording. |
| Loop transition | `loop_record_with_targeted(selector)` | Records against the targeted loop as sync source. |
| Loop mutation | `loop_set_gain(selector, gain)` | Sets linear output gain. |
| Loop mutation | `loop_set_gain_fader(selector, position)` | Sets output gain through the fader curve. |
| Loop mutation | `loop_set_balance(selector, balance)` | Sets stereo output balance. |
| Loop mutation | `loop_select(selector, deselect_others)` | Selects matching loops and optionally clears all others. |
| Loop mutation | `loop_target(selector)` | Targets one deterministic match, or clears the target for an empty selector. |
| Loop mutation | `loop_clear(selector)` | Clears matching primitive loop content. |
| Loop mutation | `loop_clear_all()` | Clears all loops. |
| Loop mutation | `loop_untarget_all()` | Clears the target. |
| Loop mutation | `loop_toggle_targeted(selector)` | Toggles one deterministic matching target. |
| Loop mutation | `loop_toggle_selected(selector)` | Toggles every matching loop's selection. |
| Loop mutation | `loop_adopt_ringbuffers(selector, reverse_cycle_start, cycles_length, go_to_cycle, go_to_mode)` | Adopts recent channel ringbuffers with the supplied timing and post-adoption mode. |
| Composite | `loop_compose_add_to_end(target, add, parallel)` | Creates/extends the target regular composition; adds matches serially or parallel to the current end. |
| Loop mutation | `loop_set_repeat_sync(selector, active)` | Enables/disables waiting for sync when playback repeats. |
| Track query | `track_get_gain(selector)` | Sequence of linear output gains. |
| Track query | `track_get_balance(selector)` | Sequence of output balances. |
| Track query | `track_get_gain_fader(selector)` | Sequence of output fader positions. |
| Track query | `track_get_input_gain(selector)` | Sequence of linear input gains. |
| Track query | `track_get_input_gain_fader(selector)` | Sequence of input fader positions. |
| Track query | `track_get_muted(selector)` | Sequence of output mute states. |
| Track mutation | `track_set_muted(selector, muted)` | Sets output mute. |
| Track query | `track_get_input_muted(selector)` | Sequence of inverse input-monitoring states. |
| Track mutation | `track_set_input_muted(selector, muted)` | Sets inverse input monitoring. |
| Track mutation | `track_set_gain(selector, gain)` | Sets linear output gain. |
| Track mutation | `track_set_balance(selector, balance)` | Sets output balance. |
| Track mutation | `track_set_gain_fader(selector, position)` | Sets output gain through the fader curve. |
| Track mutation | `track_set_input_gain(selector, gain)` | Sets linear input gain. |
| Track mutation | `track_set_input_gain_fader(selector, position)` | Sets input gain through the fader curve. |
| Global | `set_apply_n_cycles(n)` / `get_apply_n_cycles()` | Sets/gets the non-negative fixed recording cycle count; zero disables it. |
| Global | `set_solo(active)` / `get_solo()` | Sets/gets solo policy. |
| Global | `set_sync_active(active)` / `get_sync_active()` | Sets/gets synchronized-trigger policy. |
| Global | `set_play_after_record(active)` / `get_play_after_record()` | Sets/gets recording completion policy. |
| Global | `set_default_recording_action(value)` / `get_default_recording_action()` | Sets/gets `"record"` or `"grab"`; other values are ignored for compatibility. |
| Subscription | `register_loop_event_cb(callback)` | Registers a script-owned loop callback. |
| Subscription | `register_global_event_cb(callback)` | Registers a script-owned global callback. |
| Subscription | `register_keyboard_event_cb(callback)` | Registers a script-owned keyboard callback. |
| Timer | `register_one_shot_timer_cb(time_ms, callback)` | Calls once after a non-negative monotonic delay. |
| MIDI | `auto_open_device_specific_midi_control_input(regex, message_callback)` | Opens a logical input, connects matching external outputs, and forwards exact message byte sequences. |
| MIDI | `auto_open_device_specific_midi_control_output(regex, opened_callback, connected_callback, msg_rate_limit_hz)` | Opens a logical output; callbacks receive `{ send = function(bytes) }`; output connects to matching external inputs. |

## Constants

The compatibility table contains:

- every engine loop mode under `LoopMode_*`;
- `LoopEventType_ModeChanged`, `LengthChanged`, `SelectedChanged`, `TargetedChanged`, and `CoordsChanged`;
- `GlobalEventType_GlobalControlChanged`;
- `KeyEventType_Pressed` and `KeyEventType_Released`;
- stable numeric `Key_*` and `KeyModifier_*` values, including all values used by bundled scripts;
- `Loop_DontWaitForSync = -1` and `Loop_DontAlignToSyncImmediately = -1`.

The GUI key translator must emit these stable numeric values. Unsupported platform keys may be absent from events, but their constants remain available to scripts.

## Callback payloads and ordering

- Loop callback: `{ coords, type, mode, length, selected, targeted }` after the corresponding application change commits.
- Global callback: `{ type = GlobalEventType_GlobalControlChanged }` after a global control commits.
- Keyboard callback: `{ type, key, modifiers }`; auto-repeat is omitted and both press and release are delivered.
- MIDI input callback: one exact byte sequence per received message, in endpoint/arrival order.
- A callback is never recursively re-entered. Changes made by a callback commit in call order and resulting events are queued behind the current callback.
- Stopping a script removes all callbacks, timers, logical MIDI ports, endpoint connections, and queued MIDI output owned by that script.

## Built-in modules and globals

The single sources under `src/lua/lib` provide `shoop_control`, `shoop_coords`, `shoop_helpers`, `shoop_format`, and `shoop_midi`. Only these preloaded Shoop modules are required through the compatibility `require`. The runtime provides `print`, `print_trace`, `print_debug`, `print_info`, `print_warning`, and `print_error`.

Each script gets its own Lua 5.4 state. Runtime or callback failure changes only that script's status and does not remove other scripts. The sandbox's exact standard-library profile, rooting/error rules, and reviewed omniLua embedding adaptations are specified in `omnilua_runtime.md`.

## Bundled-script capability map

| Script | Direct API families | Required application/runtime behavior |
|---|---|---|
| `keyboard.lua` | Selection/target queries and mutations; mode query/trigger; clear/grab/record-with-target; global cycle count; keyboard subscription. | Stable coordinates, synchronized and immediate transitions, selection movement, target recording, fixed cycles, and press/release sampler state from `shoop_helpers`. |
| `akai_apc_mini_mk1.lua` | Loop trigger/clear/grab/select/target/composition; track gain/balance; global controls; loop/global callbacks; timer; MIDI auto-open input/output. | Full grid and sync coordinate mapping, regular composition append/parallel execution, event-driven LEDs, delayed reset, input hotplug, output broadcast/throttling, and controller reconnect. |

The APC source indexes coordinate pairs as `coords[1]`/`coords[2]`; nested numeric indexing is invalid. This behavior is part of the shared script contract.

Transitive helper calls add `loop_count`, loop mode/length/next-mode queries, explicit transition, repeat-sync, and track mute/input-mute APIs to the required set.

## MIDI connection contract

- Matching is equivalent to `^(?:user_regex)$` over the full endpoint name. Empty patterns create no connection; invalid patterns are reported.
- A logical script input connects only to external outputs. A logical script output connects only to external inputs.
- All compatible matches connect in stable full-name order. Output sends are broadcast in that order; input messages retain per-endpoint arrival order.
- Discovery is repeated or subscribed so disappearance closes only that endpoint connection and reappearance reconnects without recreating the script.
- Queues are bounded. Overflow, refused oversized messages, connection failures, and send failures are counted and exposed.
- `msg_rate_limit_hz == 0` is unthrottled. A positive value is an actual maximum dispatch frequency. Delayed control pumps do not catch up by flushing multiple messages in one pump. Correct enforcement is an intentional defect fix to the old implementation, not an API-shape change.
- Application diagnostics retain aggregate rule/connection/drop/error counts and publish each rule's direction, pattern, matched endpoint names, connected endpoint names, and latest rule-specific failure on native and browser targets.

## Lifecycle, settings, sessions, and targets

The native application's settings document (identified by `shoop-egui-settings` for compatibility) stores typed bundled enablement toggles and an ordered user path/enabled list. Both bundled scripts are discoverable on first run and only `keyboard.lua` is enabled by default. The one Settings dialog exposes all script configuration, lifecycle, documentation, logs, and MIDI diagnostics in its **Scripts** tab. Persistent edits apply after Save; Stop, Restart, and Reload are runtime-only. Source-bearing session scripts use `.shoop` `ScriptDocument` entries and never persist machine paths.

Browser packages continue to use `wasm32-unknown-unknown` and exclude native MIDI dependencies. The cooperative browser application owner runs the same omniLua scripting manager as the native application: `scripting.supported` is true, embedded keyboard/APC sources are present, keyboard defaults enabled/APC disabled, and source-bearing session scripts use the shared transaction path. Browser settings register only bundled toggles; native user paths/Add-file remain absent. Before permission or on unsupported browsers, active logical registrations remain visible with no hosts. After explicit Web MIDI access, a bounded main-thread service supplies canonical physical endpoints, exact control input/output up to the existing 256-byte limit, hotplug reconnect, and owner-managed confirmed links independently of audio startup. Browser compatibility remains defined here; physical transport behavior is specified by `docs/web_midi_contract.md`.
