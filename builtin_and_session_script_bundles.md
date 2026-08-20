# Filesystem built-ins and session script bundles

## Goals and scope

Replace compile-time application-script embedding with runtime discovery from a distributable built-ins tree, and make session scripts self-contained bundles whose Markdown and image resources work without machine paths.

In scope:

- A machine-wide built-in script location setting with platform-appropriate defaults.
- Startup discovery and user-triggered rescanning of built-in Lua scripts.
- Dynamic persistence of enabled built-ins without one setting key per filename.
- Packaging the complete built-ins directory, including companion Markdown and images.
- Per-session-script bundles containing the Lua entrypoint and relative Markdown/image resources.
- One path-safe resource-loading abstraction for filesystem and session-bundled scripts.
- Relative Markdown image loading through a custom script-resource URI loader.
- Backward-compatible settings and session migration.
- Native and browser behavior: native scans a directory; hosted browser builds use a generated external catalog rooted at the packaged built-ins location because WASM cannot enumerate directories. Neither target compiles application-script contents into the executable/WASM.

Out of scope:

- Moving host-provided `shoop_*` API libraries or the Lua sandbox out of the binary.
- Automatically watching built-in directories; discovery happens at startup, after a location-setting change, or on explicit rescan.
- Bundling arbitrary companion files when converting a session script. Only the Lua entrypoint, Markdown, and image formats supported by the application image loader are included.
- Treating bundled resources as writable or exposing their archive/storage paths to Lua.

## Immutable acceptance criteria

- No production application script (`keyboard.lua`, controller scripts, examples, or future equivalents) is named or embedded with `include_str!` in Rust.
- Native startup resolves the configured built-ins directory, recursively discovers regular `.lua` files in deterministic relative-path order, and lists every valid discovered script even when disabled.
- The built-ins location is a global setting. Its default is the packaged `builtins` directory beside the executable on Linux/Windows and under the application Resources directory on macOS; hosted browser packaging uses an external `builtins` root and generated catalog.
- Native and hosted-web artifacts contain the complete built-ins tree with directory structure, Markdown, and images preserved. A web artifact that cannot carry the external tree is not advertised as self-contained with built-ins.
- A **Rescan built-in scripts** action is available from the Scripts UI. A successful rescan adds new scripts, reloads changed scripts, removes deleted scripts, and preserves enabled choices by normalized relative identity without restarting the application.
- A missing/unreadable built-ins root or malformed individual script produces actionable diagnostics and does not crash startup or silently destroy the last usable runtime catalog during rescan.
- Built-in enablement is stored dynamically by normalized relative script identity; adding a built-in requires no Rust setting registration. Legacy keyboard/MK1 settings migrate once where present, but are not retained as the active model.
- Filesystem scripts resolve `shoop_file.load`, `dialog.markdown_file`, and relative Markdown images below the Lua script directory with one normalized, traversal-safe path policy.
- Converting a filesystem-backed script to session ownership recursively scans the Lua script’s parent directory and captures its current Lua source plus every regular Markdown/image file at that directory or deeper, retaining normalized paths relative to the Lua script directory.
- Conversion is atomic: an unreadable file, escaping symlink, invalid path, duplicate normalized path, or resource-limit violation leaves the script’s prior ownership and bundle unchanged and reports the offending path.
- A source-only script with no filesystem location can still become a session script with an empty companion-resource set. A session script converted to run-once and back reuses its in-memory bundle rather than requiring a filesystem location.
- Each session script round-trips as an independent bundle in `.shoop`: entrypoint, enabled state, relative resource paths, exact bytes, declared sizes, and hashes. Two scripts may use the same relative resource names without collision.
- Loaded session scripts can use `shoop_file.load`, `dialog.markdown_file`, and relative Markdown images on native and browser targets without extracting resources to disk.
- Session resource lookup rejects absolute paths, empty/`.`/`..` components, traversal, duplicates, undeclared resources, malformed URI encoding, and cross-script access.
- Existing source-only version-1 sessions remain loadable through an explicit migration to a one-entry script bundle; malformed or unsupported future bundles fail before session commit.
- Session decode, resource validation, Lua syntax checking, and capability checks remain transactional. A failure leaves the current session and running scripts usable.
- Built-in scanning, recursive conversion scanning, hashing, and archive work do not run in the audio callback or synchronously mutate the application actor from filesystem worker threads.

## Design rules and constraints

- Use normalized slash-separated relative paths as persistent identities. Keep display labels separate from identities and never identify scripts by basename alone.
- Centralize extension classification for `.lua`, Markdown, and every image format enabled in the renderer. Discovery and archive validation must use the same case-insensitive allowlist.
- Do not follow directory symlinks during recursive scans. Canonicalize regular-file targets and reject targets outside the selected root.
- Represent script resources as immutable byte maps behind a common provider with filesystem-root and in-memory-bundle implementations. Lua APIs must not know which provider is active.
- Use a generation-scoped custom URI such as `shoop-script-resource://<scope>/<relative-path>` for Markdown images. Include sufficient generation/content identity to prevent stale egui cache reuse after rescan or session replacement.
- Keep machine filesystem paths out of session documents, application persistence, and browser state. Publish only resource scope/base-URI metadata needed by presentation.
- Store script resource bytes as ZIP payloads, not JSON/base64. Manifest records must declare owner script, normalized relative path, kind, byte count, and SHA-256; archive entry order must be deterministic.
- Apply explicit per-file, per-script, and aggregate scan/decode limits before allocation. Reuse the session archive’s central-directory and total-uncompressed-byte protections.
- Keep all canvased/session resource bytes shared (`Arc` or equivalent) between scripting and presentation; do not copy complete bundles into every snapshot frame.
- Preserve current script lifecycle guarantees: replacement/restart tears down callbacks, MIDI connections, dialogs, and stale resource scopes before publishing the new generation.
- Native filesystem discovery and browser catalog fetching are platform adapters over one catalog/reconciliation model. Browser catalogs are generated from the same packaged directory and contain paths/hashes, not embedded source contents.

## Implementation stages

### 1. Define script identity, resource, and catalog contracts

- [x] Add normalized script identity and resource-path types plus one sanitizer/classifier shared by discovery, Lua file access, archive validation, and URI loading.
- [x] Define immutable `ScriptResourceBundle`, provider/origin metadata, built-in catalog entries, scan diagnostics, and resource-limit configuration.
- [x] Extend scripting/application API state with stable built-in identity and generation-scoped resource origin without exposing session bytes or machine paths unnecessarily.
- [x] Add unit tests for normalization, extension classification, duplicate/case handling, traversal, symlinks, and deterministic ordering.
- [x] Verify the focused API/scripting tests and warning-denying builds before changing startup behavior.

### 2. Replace hardcoded built-ins with discovery and dynamic settings

- [x] Add the global built-ins location setting and a dynamic ordered identity/toggle setting; remove active per-filename keyboard/MK1 setting logic.
- [x] Implement the native recursive scanner and the hosted-web catalog adapter/generator, including partial diagnostics and scan generations.
- [x] Build a reconciliation operation that atomically adds, reloads/restarts, removes, and preserves enablement for `ScriptKind::Bundled` records by relative identity.
- [x] Replace `KEYBOARD_SCRIPT`, APC, and dialog-example `include_str!`/manual `StartupScript` construction with discovered descriptors carrying real source paths or external browser resource origins.
- [x] Trigger discovery before initial script installation, after a committed location-setting change, and from a new **Rescan built-in scripts** Scripts action. Run native scanning on a worker and ignore stale scan completions.
- [x] Add a one-way legacy settings migration for stored keyboard/MK1 toggles, update settings documentation, and define newly discovered identities as disabled until explicitly enabled unless migrated.
- [x] Verify startup, enable/disable persistence, path changes, rescan add/change/delete behavior, invalid-file isolation, and stale-scan handling with temporary-directory tests.

### 3. Package and verify the external built-ins tree

- [x] Establish one distributable built-ins tree containing all intended scripts and companion resources; retain source-relative structure rather than flattening files.
- [x] Update Linux, Windows, and macOS artifact staging and default-path resolution, including macOS `Contents/Resources/builtins`.
- [x] Generate the hosted-web catalog from that tree and package the external files; adjust or retire the single-file web claim where an external built-ins tree cannot be present.
- [x] Extend artifact verification to assert expected scripts/resources, catalog hashes, absence of path flattening, and absence of compiled application-script source markers in binaries/WASM.
- [x] Verify packaged applications discover scripts when launched outside the source checkout.

### 4. Introduce provider-backed Lua and Markdown resource loading

- [x] Refactor `ScriptFileReader` into a provider-backed reader supporting canonical filesystem roots and immutable in-memory bundles with identical path validation/error semantics.
- [x] Attach the correct provider when starting built-in, user, run-once, and session scripts; retain useful virtual chunk names for Lua diagnostics.
- [x] Route `shoop_file.load` and `dialog.markdown_file` through the provider and preserve binary versus UTF-8 behavior.
- [x] Add a generation-scoped script-resource registry and egui custom bytes/image loader; make Markdown viewers use provider base URIs instead of deriving `file://` directly from a script name.
- [x] Ensure Markdown paths remain relative to the Lua script directory as specified, including Markdown loaded from nested paths.
- [x] Verify equivalent filesystem/bundle reads, missing resources, UTF-8 errors, image decoding, stale generations, cache invalidation, and cross-script isolation.

### 5. Redesign `.shoop` session script persistence

- [x] Add a new session document version/DTO in which each `ScriptDocument` references an entrypoint and per-script resource records rather than carrying only an inline source string.
- [x] Extend `SessionBundle` and archive encoding/decoding with deterministic `scripts/<script-id>/...` payload entries, ownership metadata, exact byte counts, hashes, and resource kinds.
- [x] Validate script IDs, entrypoint presence/type, normalized unique paths, owner/path agreement, hashes, supported types, and resource budgets before constructing runtime state.
- [x] Implement and test migration from the current source-only session document into a one-entry in-memory bundle; continue rejecting unsupported future versions transactionally.
- [x] Carry bundles through `SessionScriptSource`, `ScriptManager`, save snapshots, load staging, replacement, conversion away from/back to session ownership, and sample-rate conversion without loss or machine paths.
- [x] Update session-format documentation and golden/round-trip/adversarial archive tests.

### 6. Capture filesystem resources during session conversion

- [x] Split “include in session” into a request/completion flow so the composition layer can scan without blocking the application actor.
- [x] For filesystem-backed scripts, recursively collect supported Markdown/images below the script parent, canonicalize and read them on a worker, enforce limits, and return an immutable bundle tagged with request/script generation.
- [x] Combine scanned companions with the actor’s current Lua source as the bundle entrypoint, so conversion never silently substitutes changed on-disk Lua for the running source.
- [x] For source-only or already bundle-backed scripts, create/reuse the in-memory bundle without filesystem work.
- [x] Commit ownership and resources together only if the completion still matches the requested script generation; report stale/failing scans without partial conversion.
- [x] Verify nested resources, duplicate basenames in different directories, symlink escape, unreadable/oversized files, source changes during scan, repeated requests, and conversion away/back.

### 7. End-to-end validation and documentation

- [ ] Native: package and launch outside the checkout, verify all MK1/MK2/example scripts are listed, enable one, render adjacent Markdown/images, modify the tree, and rescan without restarting.
- [ ] Native: convert a filesystem script with nested Markdown/images to session ownership, rename/remove the source directory, save/reload, and verify file API, Markdown, and images still work.
- [ ] Browser: load the external built-ins catalog and verify rescan/reconciliation; load the same `.shoop` session and render bundled Markdown/images without filesystem access.
- [ ] Verify old source-only sessions and legacy script settings migrate, while malicious archives/paths and failed conversions leave existing runtime/session state intact.
- [ ] Run formatting, test-attribute policy, warning-denying workspace builds, complete native tests, WASM builds/tests, browser smoke tests, artifact verification, and tracing-coverage checks required by the touched code.
- [x] Update scripting, dialog/file API, settings-format, session-format, packaging, and user documentation to describe discovery, defaults, rescan, bundle capture rules, supported resource types/limits, and platform behavior.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
