# Mixed Visual C++ runtimes on Windows

Two causes, both now fixed. This records what went wrong, why, and how to tell if
it comes back — the diagnostics for that are in the build log.

## Background: rustc forces the release CRT

`rustc` on MSVC links the **release** C runtime (`vcruntime140.dll`) and has no
`/MDd` equivalent — the CRT flavour is not selectable per profile the way it is
for MSVC C++. So in *any* build of this application, debug included, the Rust side
is release-CRT.

Everything below follows from that: a debug-built C++ dependency can never match
the Rust side, so C++ dependencies must be release-built even in a debug build.

Two CRTs in one process do not share a heap. An allocation or a `FILE*` crossing
a module boundary between them corrupts state.

## Cause 1: cxx-qt was pointed at vcpkg's debug Qt

`scripts/vcpkg_prebuild.py:find_qmake` used to compile a wrapper around
`qmake.debug.bat` for Windows debug builds, so cxx-qt linked debug Qt and the
packaging step copied the debug Qt plugin/QML tree. Debug Qt is `/MDd`; the Rust
executable is `/MD`. The debug portable folder crashed at startup as a result —
packaging reported 0 unresolved and 0 unclassified, and the process got far enough
to write a minidump, so it was never a missing-DLL failure.

**Fixed** by always using the release `qmake`, which is what non-Windows platforms
were already doing (the debug branch was guarded on `win32`). The wrapper
machinery existed only to run that `.bat` and has been removed.

Trade-off: no Qt asserts or debug symbols in Windows debug builds. Deliberate —
they were never usable with a release-CRT Rust binary anyway.

## Cause 2: the dependency search order was inverted

`common::env::add_lib_search_path` **prepends** to `PATH`. The old code iterated
`["debug/bin", "bin", "debug/lib", "lib"]` and prepended each, so the effective
search order came out reversed — `lib`, `debug/lib`, `bin`, `debug/bin`. Since
vcpkg keeps DLLs in `bin`/`debug/bin` (the `lib` dirs hold import libraries),
**release `bin` effectively won**.

`windows_search_dirs` in `src/rust/packaging/src/scan.rs` reads its list as
*priority* order, and was given that same array — silently inverting the
behaviour. This mattered because **non-Qt vcpkg libraries have identical names in
both trees** (`zlib1.dll`, `harfbuzz.dll`, `double-conversion.dll`, ...), so order
alone decides which flavour is bundled. Qt is unaffected: its debug binaries are
`d`-suffixed, so a reference to `Qt6Core.dll` cannot resolve to a debug build.

This hit the **release** package, not just debug: 28 of 100 bundled DLLs came from
`debug/bin`, dragging in the debug CRT. Measured on a release build:

```
Runtime msvcp140d.dll       <- double-conversion.dll
Runtime vcruntime140d.dll   <- FLAC.dll, brotlicommon.dll, brotlidec.dll, bz2d.dll
Runtime vcruntime140_1d.dll <- double-conversion.dll, harfbuzz.dll, meshoptimizer.dll
```

That is worse than the debug crash: `VCRUNTIME140D.dll` is **not
redistributable**, and it only resolved at all because CI runners have Visual
Studio installed.

**Fixed** by putting `bin` before `debug/bin`.

## Why fixing only one would not have been enough

| | fixes | why the other does not |
|---|---|---|
| release `qmake` | the debug package's **Qt** | search order cannot pick debug Qt — it is `d`-suffixed |
| release-first search order | the release package's **non-Qt** libraries | `qmake` does not govern non-Qt libraries at all |

Note also what does **not** work: excludelisting the debug CRT. That changes only
which runtime *files* are bundled, not which binaries *import* them. A bundled DLL
importing `MSVCP140D.dll` will either find it on the target machine (both CRTs
load, same crash, now with an undeclared dependency on a non-redistributable DLL)
or fail to load outright. An ABI mismatch cannot be fixed by relocating files —
only by changing which binaries are in the package. Both fixes above do that;
excludelisting would merely have silenced the warning.

## How to tell if it comes back

Two diagnostics, both in the build log (CI sets `SHOOP_LOG=packaging=debug` in
`.github/actions/build_package/action.yml`).

1. **Mixed-runtime warning** — `warn_on_mixed_vc_runtimes`,
   `src/rust/packaging/src/portable_folder_common.rs`:

   ```
   --> Bundling both debug and release Visual C++ runtimes (msvcp140.dll + msvcp140d.dll).
   ```

   A clean build log means one CRT. This is the acceptance signal for both fixes.

2. **Per-runtime importer attribution** — `log_report_summary`,
   `src/rust/packaging/src/scan.rs`:

   ```
   Runtime msvcp140_2.dll <- Qt6Gui.dll, Qt6Quick.dll (from .../bin/MSVCP140_2.dll)
   ```

   The CRTs import each other (`MSVCP140_2.dll` pulls in `MSVCP140.dll`,
   `VCRUNTIME140.dll` and `VCRUNTIME140_1.dll`), so a single mismatched binary
   drags in a whole chain. **Only the first link identifies the culprit**: look for
   the runtime whose importers are not themselves CRT DLLs.

Available without a full build, against any package folder:

```
package scan-dependencies --folder <dir> --use-cmake-prefix-path --report-only
```

## Still outstanding

- **The crash was never proven to be the CRT mismatch.** The minidump was not
  examined. The mismatch is real and had to be fixed regardless, but if the debug
  package still crashes with a clean CRT log, look elsewhere.
- **The CI check is still narrowed.** `.github/actions/build_toplevel/action.yml`
  runs the full UI-loading verification (`--backend dummy --test-grab-screens`,
  asserting screenshots exist and the log is free of loader errors) only for
  **release** packages; debug and coverage fall back to `--help`. Restoring it for
  debug is the acceptance criterion for closing this out. Coverage cannot be
  restored the same way: Qoverage rewrites the QML to import `QoverageSingleton`,
  which belongs to the coverage tooling and is not in the package, so no window is
  ever created.
- **`VCPKG_BUILD_TYPE` is deliberately not set.** Making vcpkg release-only would
  prevent this by construction, but debug dependency trees are wanted for other
  work. Both trees are still built; the fixes ensure only the release one is
  linked and bundled.
