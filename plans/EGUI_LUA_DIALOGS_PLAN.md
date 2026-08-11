# egui Lua-controlled dialogs plan

## Status and document role

Status: **Planned**.

This is the implementation contract for script-owned dialogs in `shoopdaloop_egui`. It must remain synchronized with:

- `EGUI_FEATURE_PARITY_MATRIX.md` for capability status and evidence;
- `EGUI_REPLACEMENT_PROJECT.md` for architectural status;
- `docs/egui_lua_compatibility_contract.md` and the scripting user documentation for the public Lua contract.

## Investigation findings

- `shoop_scripting` owns one non-`Send` omniLua runtime per active script. `ScriptManager` already centralizes start, stop, restart, source replacement, callback dispatch, and teardown on the application owner for native and browser builds.
- Lua callbacks cannot cross the immutable application snapshot boundary. Dialog content therefore needs plain framework-independent descriptors and opaque callback identities; the actual omniLua `Function` values must remain inside the owning runtime.
- `shoop_app_api` is the plain state/intent boundary, `shoop_app` publishes script state and applies callback-produced operations, and `shoop_egui::AppWidget` renders snapshots and emits intents. This is the required path for dialog-button callbacks as well.
- Script-originated opening needs an edge/token rather than only an `open: bool`: a script must be able to reopen a dialog after the user closes it. User open/closed and current-page state are presentation state keyed by a generation-safe dialog identity.
- Dialog definitions themselves must keep a script listening. Otherwise a script that only defines informational dialogs would immediately finish and lose them.
- The top control bar is rendered by `GlobalControls` inside `AppWidget`; other egui dialogs use `egui::Window`, so no native-window or filesystem dependency is needed for either native or browser builds.
- No corresponding Lua-dialog feature exists in the retained QML application. This feature must not add a frontend/QML bridge or alter the legacy application's Lua surface.
- The egui Lua API is currently unversioned. A mandatory compatibility handshake must run before any versioned Shoop API is used, reject incompatible scripts synchronously, and remain independent of versioned modules so future API revisions do not need to change the handshake itself.

## Goals and scope

Deliver a semantically major/minor-versioned, cross-target egui Lua API with which each active script first announces its required API version, then can define any finite number of named simple or paged dialogs, optionally request that they open, and receive button callbacks. Render those definitions only in `shoopdaloop_egui` as persistent egui windows, with discovery and reopening from the top control bar.

In scope:

- a mandatory, stable egui Lua API version-announcement handshake and documented major/minor compatibility rules;
- migration of bundled egui scripts, examples, fixtures, and tests to announce the version for which they were designed;
- a new built-in `shoop_dialog` Lua module and documented constructor/opening contract;
- target-neutral dialog descriptors, stable identities, owner metadata, opening tokens, and button-callback intents;
- runtime-owned callback storage and lifecycle cleanup;
- simple and paged egui rendering, local open/page state, owner help, top-bar list, and count;
- native actor and browser cooperative runtime behavior;
- focused, workspace, browser, artifact, documentation, and retained-QML regression evidence.

Out of scope:

- support in the legacy QML application or its `frontend` crate; its legacy Lua surface remains outside this egui API version contract;
- native OS windows, browser popups, or target-specific dialog implementations;
- arbitrary Lua-built widget trees, text inputs, images, custom layout, or user-supplied egui code;
- script APIs to move/resize/close dialogs or mutate an existing definition;
- persisting open state, geometry, current page, or generated dialog descriptors in settings or `.shoop` files across application/script restarts. Source-bearing scripts remain persistent through the existing session mechanism and recreate their dialogs when run.

## Immutable acceptance criteria

1. The egui Lua host exposes a semantic API version consisting of non-negative integer `major` and `minor` components. This feature establishes and documents the initial host version; patch versions and string parsing are not part of the compatibility decision.
2. Every egui Lua script must make `shoop_announce_api_version(major, minor)` its initial Shoop API call, using the version for which it was designed. The global function name and two-integer signature are the permanent, unversioned handshake: it is installed before `require` or any versioned Shoop module, depends on no module/table shape, and is reserved to remain available across future major versions.
3. A host at `Hmajor.Hminor` accepts a script at `Smajor.Sminor` exactly when `Smajor == Hmajor` and `Sminor <= Hminor`. Any different major or higher script minor is incompatible; equal and lower script minors run. Missing, repeated, non-integer, negative, or otherwise malformed announcements are rejected with an actionable script-local error.
4. An incompatible script is cancelled synchronously by its initial announcement call before any versioned Shoop API, dialog registration, callback, timer, MIDI rule, or control mutation can take effect. It enters the existing error lifecycle with requested and supported versions in diagnostics; failure does not affect another script. A script that never announces is also rejected rather than treated as an implicit legacy version.
5. `shoopdaloop_egui` Lua scripts accepted by the version handshake can `require('shoop_dialog')` and define any finite number of dialogs. Names are unique within one script runtime; the same visible name may be used by different scripts without identity or callback collisions.
6. The public module provides constructors for rich-text and labeled-button elements, simple-dialog registration, paged-dialog registration from an ordered sequence of simple contents, and `open(name)`. A button callback is optional; malformed elements, pages, callbacks, empty names, duplicate names, and opening an unknown name produce actionable script-local Lua errors.
7. Rich text is represented by a documented target-neutral text/style value rather than an egui type, HTML, or executable markup. Text, style, element order, page order, button labels, and callback association survive publication unchanged on native and Wasm targets.
8. A simple dialog renders its elements in one vertical sequence. Rich text wraps as needed; labeled buttons occupy their corresponding sequence position and invoke only their own current-runtime callback.
9. A paged dialog renders exactly one simple content page at a time and has a page control at the bottom that can browse every page and communicates the current position. The selected page is clamped safely and remains selected when the window is closed and reopened during the same definition lifetime.
10. Every dialog is an `egui::Window`, never a native child/OS window. Its title is the Lua-defined dialog name. A small question-mark control in the window decoration/content has hover text containing the owning script's current name.
11. Users can close an open dialog and reopen any existing dialog from a drop-down in the top control bar. The control visibly shows the total number of existing dialog definitions, including closed dialogs; duplicate visible names are disambiguated by owner in the list while the window title remains the dialog name.
12. User-controlled open/closed state, current page, and egui window identity survive ordinary application snapshot revisions and unrelated UI activity. They reset when that exact dialog definition disappears; recreating the same script/name after restart cannot inherit stale page state or target stale callbacks.
13. `open(name)` increments a generation-safe opening request so it opens a closed dialog at startup, from a timer/event/MIDI callback, or from a dialog-button callback. Repeated requests remain observable even after intervening user closes, and opening one dialog does not change another dialog's state.
14. Clicking a callback-bearing button emits a typed intent to the application owner. The owner rejects stale script/dialog/button identities, invokes the omniLua function non-reentrantly on the owning runtime, applies any resulting ordinary control operations in order, publishes dialog/opening changes, and reports callback errors using the existing script-local diagnostics without affecting other scripts.
15. Defining a dialog keeps its script in the listening lifecycle even if it has no buttons or other subscriptions. Stopping, disabling, restarting, replacing, forgetting, session-replacing, finishing, or failing a script removes all definitions, callback functions, and pending opening requests owned by the discarded runtime before the next authoritative snapshot; no other script's dialogs are removed.
16. Dialog snapshots contain only bounded/plain immutable data and opaque IDs—never omniLua functions, runtime handles, egui types, backend handles, or mutable cross-thread state. Dialog callbacks and registries remain application-owner-local and do not run on the audio callback.
17. `shoop_egui` remains presentation-only and browser-compatible, depending on plain API/settings crates rather than `shoop_scripting`, the backend, filesystem APIs, native window APIs, Qt, or QML. Native and browser builds use the same version check, Lua parsing, descriptors, application intent path, and renderer.
18. Existing version-announcing Lua control, keyboard, timer, MIDI, settings, session, and error-isolation behavior remains compatible. Bundled egui scripts are migrated to announce the established version. The legacy QML executable neither adopts this egui handshake nor exposes/renders `shoop_dialog`, and retained QML tests continue to pass without a new frontend bridge.
19. The Lua/API docs define the host version, permanent announcement signature, compatibility matrix, cancellation/migration behavior, dialog signatures, rich-text fields, validation/error behavior, naming scope, callback semantics, lifecycle, and egui-only availability. The feature-parity matrix and project status are updated with implementation and verification evidence.

## Design rules and constraints

- Define the host's supported egui Lua API version in one shared source of truth. Compare numeric `(major, minor)` pairs only: equality of majors is mandatory, and the script minor may not exceed the host minor.
- Keep `shoop_announce_api_version(major, minor)` deliberately primitive and outside all versioned modules. Do not replace it with `require`, a table argument, a version string, feature negotiation, or a callback whose shape would itself need version negotiation.
- Install only the handshake before compatibility is established and gate all versioned Shoop entry points until it succeeds. The first successful announcement locks the runtime's requested version; missing or additional announcements are errors, and no incompatible pre-announcement side effects may leak into application state.
- Treat rejection of unannounced egui scripts as the intentional migration boundary. Update bundled scripts and test/example fixtures, document how user/session scripts add the first-line call, and preserve the unversioned legacy QML runtime rather than weakening egui enforcement.
- Use an opaque generation-safe `ScriptDialogId` (and button identity) in `shoop_app_api`. Never use a display name alone for UI state or callback routing.
- Keep omniLua `Function` values in a per-runtime registry in `shoop_scripting`. Publish copied descriptors through `ScriptManager`; do not place functions in `ScriptingState` or make the runtime `Send`.
- Add a dedicated dialog bridge/module rather than mixing presentation descriptors into `ControlBridge`. Callback execution may still feed the existing ordered `ControlOperation` path.
- Treat a dialog definition as immutable for its runtime lifetime. Duplicate registration is an error rather than an implicit replacement; restart/source replacement tears down the old generation before creating a new one.
- Scope names per script, preserve registration order for deterministic top-bar listing, and use owner metadata to disambiguate cross-script duplicates.
- Represent script-driven opening with a monotonic request token in authoritative state. Keep user close/reopen and current page local to `AppWidget`; do not send frame-by-frame visibility state back to Lua.
- Define a small portable rich-text schema in Stage 0 and map it to `egui::RichText` only in `shoop_egui`. Do not add a Markdown/HTML parser unless separately approved.
- Reuse the existing application actor/cooperative command paths and callback error handling. A dialog callback error is observable and script-local; it does not become a GUI panic or execute on the render/audio thread.
- Make teardown explicit on every runtime-discard path, including partial startup failure after definitions were registered. Stale queued clicks must fail safely and cannot call a new generation's callback.
- Keep this feature out of settings/session documents. Existing session script source persistence is sufficient; runtime dialog state is ephemeral by design.

## Staged implementation plan

Dependencies are sequential unless explicitly noted. Complete, verify, document, and commit each stage before beginning its dependent stage.

### Stage 0 — Freeze the version handshake, Lua API, and plain-data contract

- [ ] Establish the initial egui Lua host major/minor version and specify the permanent global `shoop_announce_api_version(major, minor)` call, first-call requirement, exact compatibility matrix, malformed/repeated/missing behavior, and migration example.
- [ ] Specify the exact `shoop_dialog` signatures and examples for rich-text/button constructors, simple and paged registration, and `open(name)`; define the portable rich-text style fields and validation rules.
- [ ] Add framework-independent API-version and dialog/content/element/owner/ID/button/open-request types to `shoop_app_api`, plus the typed button-click intent and stable `kind()` value where these values cross the application boundary.
- [ ] Define version comparison, identity/generation, registration ordering, duplicate-name, empty-page/content, optional-callback, and stale-action semantics in tests before wiring presentation.
- [ ] Add planned rows to `EGUI_FEATURE_PARITY_MATRIX.md`, mark this feature in progress in `EGUI_REPLACEMENT_PROJECT.md`, and remove/update stale browser-scripting documentation discovered during the audit without claiming implementation evidence early.

Verification:

- [ ] `cargo test -p shoop_app_api` covers the full lower/equal/higher minor and different-major matrix, invalid numeric components, ID distinction, plain descriptor equality/order, intent identity, and opening-token behavior.
- [ ] API/docs review confirms the announcement function is independent of every versioned module and that no acceptance behavior depends on an egui, Qt, backend, or Lua runtime type crossing the plain boundary.
- [ ] Commit the contract/types/documentation milestone.

### Stage 1 — Enforce API compatibility and implement runtime-owned dialogs

- [ ] Install `shoop_announce_api_version` as the only stable pre-version API entry point in each egui script runtime; track exactly one announcement and synchronously abort incompatible or malformed calls with requested/supported versions in the error.
- [ ] Reject scripts that complete initial execution without announcing, and gate `require` plus all other Shoop host APIs so an unannounced/incompatible script cannot register or mutate application-owned state before cancellation.
- [ ] Migrate bundled egui scripts and all egui Lua fixtures/examples to announce the initial version while keeping their shared source usable by the explicitly unversioned retained QML runtime.
- [ ] Add a focused dialog module/registry in `shoop_scripting` and install it as the new embedded `shoop_dialog` library only after a compatible announcement.
- [ ] Parse constructor/registration tables into plain descriptors with deterministic order and script-local validation errors; retain button `Function` values only in the current runtime registry.
- [ ] Make existing-dialog presence count as listening activity, generate new definition/open tokens safely, and expose current descriptors through `ScriptManager` with `ScriptId` and script name ownership attached.
- [ ] Add a `ScriptManager` entry point that validates script/dialog/button generation and invokes one button callback non-reentrantly, capturing errors and any dialog/open operations generated by the callback.
- [ ] Clear registries and requests on every stop/restart/error/replacement/drop path, including execution that registers a dialog and then fails.

Verification:

- [ ] `cargo test -p shoop_scripting` covers equal/lower accepted minors, higher rejected minor, both directions of major mismatch, malformed/negative/missing/repeated announcement, exact cancellation diagnostics, no pre-announcement side effects, cross-script isolation, and the complete simple/paged dialog matrix.
- [ ] Dialog coverage includes ordered elements/pages, all documented rich-text fields, optional callbacks, duplicate/unknown/malformed input, startup and callback `open`, callback isolation/order/error reporting, informational-dialog listening, stale identities, restart generations, and complete teardown.
- [ ] Production Lua source embedding/syntax tests include the new library and version announcement; existing control-function inventory and announced bundled keyboard/APC workflows remain green, while retained QML tests prove the shared sources still run there.
- [ ] Native and `wasm32-unknown-unknown` checks confirm one shared implementation and no target-native dependency in the new module.
- [ ] Commit the scripting-runtime milestone and update plan evidence.

### Stage 2 — Publish dialogs and route callbacks through the application owner

- [ ] Extend the immutable scripting/application snapshot with structurally shareable active dialog descriptors while keeping callback functions and large/mutable runtime state out of it.
- [ ] Refresh dialog state after startup, every script lifecycle intent, timers/events/MIDI, button callbacks, session-script replacement, and normal application ticks so script-driven opens and teardown become authoritative promptly.
- [ ] Handle the typed dialog-button intent in `shoop_app`: prepare the current control snapshot, invoke the exact runtime callback, apply resulting `ControlOperation`s through the existing reducers/backend path, refresh script/dialog diagnostics, and notify safely on stale/error cases.
- [ ] Ensure native threaded and browser cooperative runtimes preserve command ordering and that one script's callback/lifecycle cannot mutate another script's dialog registry.

Verification:

- [ ] `cargo test -p shoop_app` covers startup-open, timer/event/MIDI/button-triggered open, repeated reopen tokens, callback-produced loop/global changes, callback-produced dialog opens, stale click after restart/stop, startup failure cleanup, stop/disable/forget/source/session replacement, cross-script same-name isolation, and actor/cooperative parity.
- [ ] Snapshot tests prove plain structural sharing, deterministic ordering/ownership, no callback leakage, and removal on all lifecycle paths.
- [ ] Existing script settings/session/driver-switch/browser-Lua application tests remain green.
- [ ] Commit the application-integration milestone and synchronized status updates.

### Stage 3 — Render and control dialogs in egui

- [ ] Add a dedicated script-dialog presentation component to `shoop_egui`, keyed by `ScriptDialogId`, with local open state, last-seen opening token, current page, and stale-state pruning.
- [ ] Integrate the visible dialog count and drop-down into the top control bar. List all current definitions in deterministic order, disambiguate duplicate names with owner names, and open the selected definition without an application intent.
- [ ] Render simple contents vertically and paged contents one page at a time with a bottom page control; map the portable rich-text schema to `egui::RichText` and emit exact button-click intents only for callback-bearing buttons.
- [ ] Decorate each `egui::Window` with its dialog-name title and hoverable question-mark owner help. Preserve close/reopen/current-page state across snapshots, while resetting state for removed/recreated generations.
- [ ] Keep windows usable at constrained browser sizes with wrapping and bounded scrolling/resizing, without introducing native window APIs.

Verification:

- [ ] Backend-free `shoop_egui` interaction tests cover zero/nonzero count, list opening, user close/reopen, startup/repeated script open, multiple scripts and duplicate names, title/owner help, ordered vertical text/buttons, optional callback behavior, exact intent IDs, every-page navigation, page persistence, unrelated snapshot revisions, removal, and same-name new generation reset.
- [ ] Paint tests cover simple and paged dialogs at minimum/common viewports with long rich text and labels.
- [ ] `cargo test -p shoop_egui -p shoop_app_api` and `cargo check -p shoop_egui --target wasm32-unknown-unknown` pass; dependency inspection confirms no scripting/backend/native/Qt additions.
- [ ] Commit the egui presentation milestone and update user-facing documentation/status.

### Stage 4 — Final cross-target and end-to-end validation

- [ ] Add one small example/session script used by tests that begins with the stable version announcement, defines both dialog flavors, opens on startup and from a callback, and mutates observable application state from a dialog button.
- [ ] Extend native product-level and production browser automation to prove accepted equal/lower-minor announcements, visible cancellation of higher-minor/different-major/unannounced scripts with no side effects, and the top-bar count/list, egui-window rendering, owner help, page browsing, close/reopen, script-driven reopen, button callback, and immediate teardown when the script stops.
- [ ] Verify hosted and self-contained browser artifacts use egui windows and the shared omniLua module with no browser popup/native-window dependency; verify native packaging embeds the module without source-tree access.
- [ ] Update `docs/source/developers.scripting.rst`, `docs/egui_lua_compatibility_contract.md`, `src/rust/shoopdaloop_egui/README.md`, the feature matrix, and project status with exact supported behavior and evidence.
- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run focused warning-denying tests/builds for `shoop_app_api`, `shoop_scripting`, `shoop_app`, `shoop_egui`, and `shoopdaloop_egui`, including native and `wasm32-unknown-unknown` targets.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace --features shoop_engine/app_backend` and `cargo test --workspace --features shoop_engine/app_backend`.
- [ ] Build first, then run `target/debug/shoopdaloop_dev.sh --self-test` to prove the legacy QML app remains unchanged and compatible.
- [ ] Build/verify debug and release hosted and self-contained WebAssembly artifacts, run the relevant Chrome/Firefox scripting workflows, and audit package/dependency/source boundaries.
- [ ] Record exact test counts, platforms, browser modes, skips, and residual limitations in this plan; reconcile every new matrix row and commit the validation/documentation milestone.

Final acceptance evidence must include one native and one production-browser workflow proving the complete major/minor compatibility matrix and side-effect-free cancellation, followed by a compatible script that creates multiple simple/paged dialogs, reopens them at startup and from a callback after user close, retains page state across close/reopen, changes authoritative state through the correct Lua button callback, and loses all owned windows/list entries after stop while another script remains intact. Source and dependency audits must prove the handshake is stable and module-independent, with no QML/frontend dialog implementation and no native-window path.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
