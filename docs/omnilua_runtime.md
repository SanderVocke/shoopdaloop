# omniLua runtime contract and audit

ShoopDaLoop uses **omniLua 0.7.1** as its only embedded Lua runtime. The workspace pins the exact pre-1.0 release and uses Lua 5.4 semantics (`Lua::new()`). Runtime upgrades are deliberate compatibility changes: they require the complete `shoop_scripting`, native application, and browser Wasm evidence to be rerun before the pin changes.

## Configuration and sandbox profile

The workspace disables omniLua's default features and enables `coroutine`, `io`, and `os`. Shoop's sandbox exposes the base, string, table, and math libraries, the coroutine functions listed by `src/lua/system/sandbox.lua`, and only `os.clock`, `os.difftime`, and `os.time`. It does not expose `io`, dynamic package loading, the debug library, or host filesystem APIs. The `io` Cargo feature is linked only because omniLua 0.7.1's `os` implementation references its internal `io` result helper; no `io` table is copied into the Shoop sandbox.

Shoop supplies its own bounded `require` implementation for the single-sourced modules embedded from `src/lua/lib`. Built-in scripts remain embedded directly from `src/lua/builtins`.

## Dependency and license audit

omniLua 0.7.1 is MIT licensed and is a pure-Rust implementation. Its resolved runtime chain is the matching 0.7.1 set `lua-code`, `lua-gc`, `lua-lex`, `lua-parse`, `lua-stdlib`, `lua-types`, and `lua-vm`. It introduces no C Lua library, build script, Emscripten requirement, or native MIDI dependency on `wasm32-unknown-unknown`.

The lean configuration avoids omniLua's package, debug, UTF-8, bit32, derive, serde, and async features. Browser artifact size is checked at packaging time rather than inferred from native object files.

## Ownership, callbacks, and failure behavior

- Each application script owns one application-thread-confined `omnilua::Lua` state. Lua never enters an audio callback.
- omniLua values, tables, functions, and strings are owned GC-rooted handles. Cloning a callback clones its root; dropping the last handle queues the external root for removal.
- Rust callback panics are caught by omniLua and become Lua runtime errors. Shoop additionally prevents recursive script callback dispatch and records callback errors script-locally.
- omniLua is single-threaded and its error values are correspondingly not `Send + Sync`. Application boundaries convert them immediately to owned diagnostic strings before entering `anyhow::Error`; rooted Lua errors are never sent across threads.
- Script callback, timer, MIDI, and application-operation pumps retain their existing bounds. Garbage collection and Lua allocation occur only on the control/application side.

## Reviewed API adaptations

omniLua intentionally resembles but is not source-compatible with the former embedding API. Shoop uses omniLua directly and makes these explicit adaptations:

- dynamic argument lists use `omnilua::Variadic<Value>`;
- `u8` and `f32` conversions range-check through `i64` and `f64` respectively;
- sequence/table helpers are implemented with omniLua `Table::get`, `Table::set`, and `raw_pairs`;
- callbacks with more than three typed parameters parse an exact `Variadic<Value>` tail;
- implementation-specific conversion errors are emitted as equivalent runtime diagnostics because omniLua has no public conversion-error variants.

These are embedding adaptations, not changes to the Lua API frozen in `docs/lua_compatibility_contract.md`.
