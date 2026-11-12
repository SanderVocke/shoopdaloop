# P16 investigation: client-owned script persistence

Investigation baseline: `origin/feat/backend-replacement` at `9aee7496`;
production comparison: `origin/master` at `9134eb20`. Updated on 2026-11-12 after
the user approved client ownership of both script data and execution. This
supersedes the original service-owned data proposal and its implementation steps.
No feature implementation or Rust/browser acceptance was performed. P16 stays OPEN.

The agreed requirements are in the [P16 contract](p16-client-scripts.md); execution
is tracked in [PLAN_P16_CLIENT_SCRIPT_PERSISTENCE.md](../../PLAN_P16_CLIENT_SCRIPT_PERSISTENCE.md).

## Conclusion

A saved project file can combine the service's musical state with client-owned
script additions. P16 does not need a backend script catalog or script/resource
transfer protocol. Source, bundles, device bindings and execution belong to the
client. Existing musical capture/load remains the backend boundary.

This restores file save/load parity while deliberately limiting backend recovery
to music and accepted musical operations. A different client cannot recover the
previous client's script source/resources without its session file. Durable client
crash recovery and backend attachment storage are separate work.

## Code evidence

Paths below are relative to `src/rust/` on the replacement branch unless noted.

| Area | Existing code and required adaptation |
| --- | --- |
| Production oracle | `origin/master:src/rust/shoop_app/src/lib.rs` contains `scripts_export_exact_source_and_convert_session_ownership`, `filesystem_script_conversion_captures_resources_off_actor_and_survives_source_removal`, and `session_scripts_stage_before_commit_round_trip_and_preserve_machine_scripts`. Retained in `shoop_app_reference/src/lib.rs`. Port resource and preservation assertions, replacing automatic load execution with explicit starts. |
| Conversion | Reference `convert_script_kind` captures companions off the actor and checks source/resource generation before completion. Replacement `shoop_app_client/src/runtime.rs` calls `ScriptManager::convert_kind` directly, which can change local ownership and create a source-only bundle without the complete filesystem capture workflow. Restore bounded local capture, not a service command. |
| Script manager | `shoop_scripting/src/lib.rs`: `conversion_source`, `commit_session_bundle`, `session_scripts`, `validate_session_scripts`, `replace_session_scripts`. Reuse local source/bundle mechanics; introduce prepared stopped installation because the current replacement path can start enabled records. Preserve machine-local records. |
| Resources | `shoop_script_resources/src/lib.rs`: normalized paths, immutable bundles, SHA-256, URI providers and filesystem capture. Capture uses current in-memory entrypoint source and recursively collects supported Markdown/PNG companions; companion Lua files are skipped. Reuse bundles after source removal and manage provider lifetimes locally. |
| Archive | `shoop_session/src/archive.rs` supports script descriptors, owner/entrypoint/resource records and validation in reference paths. Production bounded encode rejects nonempty script bundles and bounded decode refuses script records. Extend bounded client accounting and remove refusals only when resources are retained and charged. |
| Portable codec | `shoop_persistence_codec/src/document.rs` rejects scripts in `requirements`/`from_bundle`; `codec.rs` prepares a musical image without a separate client additions result. Add explicit archive composition/extraction so no script data is silently stripped or uploaded. Keep bytes unchanged during musical rate conversion. |
| Browser worker | `shoop_persistence_codec/src/worker.rs` owns decoded input behind handles and currently transfers prepared musical images. `shoopdaloop/src/persistence_io/jobs/{browser.rs,codec_worker.js}` needs bounded client-worker additions handoff for save/load; this does not require changing service image/object schemas. |
| Runtime/provider boundary | `shoop_app_client/src/runtime.rs::PersistenceIo` passes only a `session_scripts: bool`. Replace it with immutable save input and prepared-load/confirmed-install handoffs; runtime retains sole VM ownership. |
| File tasks and commit | `shoopdaloop/src/persistence_io/{mod,pump}.rs` controls tasks, codec work and `reconcile_commit`. Integrate pending client additions with exact operation/host/session identity and confirmed outcomes. The current media-only completion signal is insufficient to report a complete client project load. |
| Workflow guards | Runtime blocks New Session with session scripts; the file provider blocks save/load when the local script boolean is set. Remove these guards only after coordinated local replacement works, including successful empty-session loads. |
| Export | Client `ExportScript` emits source bytes but unconditionally appends `.lua`; the production reference preserves `future.lua`. Verify exact bytes and filenames, including incompatible sources. |
| Service/engine | `shoop_app/src/service/persistence/{mod,mapping,capture}.rs` already prepares and atomically publishes musical replacements. Reuse these paths without a script catalog, resource uploads or new RT engine primitives. |

## Important boundaries to verify

- Freeze additions per save task and match them to the correct musical session;
  later source edits must not alter an already captured file.
- Validate full resources and supported Lua syntax without running code. A client
  unable to execute a supported-format bundle retains it inactive rather than
  deleting it. Archive compatibility and script API compatibility stay separate.
- Prepare local replacement records/providers before musical admission, keep old
  data through failure/cancellation, and block old-session commands during commit.
  Successful local installation preserves machine scripts and starts no VM.
- Musical commit and client memory cannot form a cross-process atomic transaction.
  A surviving client retains candidates and reconciles confirmed outcomes without
  replay; an uncertain result keeps affected scripts inactive. Client death loses
  this pending context even when the backend finishes the musical load.
- Client resource defaults are 16 MiB/file, 64 MiB/script, 256 MiB aggregate,
  10,000 resources/script and 100,000 scan entries. Extend archive/worker memory,
  expansion and count accounting explicitly; do not force these resources into
  musical transfer object limits or use unbounded reference codec paths.

The main work is now bounded client archive composition, local resource capture
and safe script installation around musical load completion. Backend script
storage and transfer are no longer part of the implementation scope.
