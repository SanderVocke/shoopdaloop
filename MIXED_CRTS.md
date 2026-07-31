# Open issue: the Windows debug portable package bundles two C++ runtimes

## Symptom

The **debug** Windows portable package crashes at startup once the UI is loaded.
Packaging itself succeeds — the dependency scan reports 0 unresolved and 0
unclassified — and the process gets far enough to install the crash handler and
write a minidump, so it is not a missing-DLL loader failure. It then dies with
exit code 139 before printing any of its own log output.

The **release** Windows package is unaffected and verified working end to end
(loads the real UI from the package with the build environment stripped).

## What is in the package

The debug package bundles both flavours of the Visual C++ runtime:

| release | debug |
|---|---|
| `MSVCP140.dll` | `MSVCP140D.dll` |
| `MSVCP140_2.dll` | `MSVCP140_1D.dll`, `MSVCP140_2D.dll` |
| `VCRUNTIME140.dll` | `VCRUNTIME140D.dll` |
| `VCRUNTIME140_1.dll` | `VCRUNTIME140_1D.dll` |

Two CRTs in one process do not share a heap. An allocation or a `FILE*` crossing
a module boundary between them corrupts state, which is the most likely
explanation for the crash — though note this has **not** been proven; the
minidump was never examined.

## Where they come from

Established by reading real import tables, not inferred:

1. **The CRT DLLs import each other.** `MSVCP140_2.dll` imports `MSVCP140.dll`,
   `VCRUNTIME140.dll` and `VCRUNTIME140_1.dll`. `MSVCP140_1.dll` imports
   `MSVCP140.dll`.

2. **`*/MSVCP*.dll` and `*/VCRUNTIME*.dll` are in
   `distribution/windows/includelist`**, so once one is discovered it is bundled
   *and walked* — its own imports come along.

3. **The release-flavour set observed in the debug package is exactly
   `MSVCP140_2.dll` plus its own closure.** That is not a coincidence: a single
   edge to the release `MSVCP140_2.dll` accounts for all four release DLLs.

Two plausible culprits were ruled out:

- The debug CRT chain is self-consistent — `MSVCP140D.dll` imports only
  `VCRUNTIME140D.dll` and `VCRUNTIME140_1D.dll`; nothing in it references a
  release CRT.
- `dbghelp.dll`, although bundled from `System32`, imports no Visual C++ runtime
  at all.

So: **some binary in vcpkg's `debug/bin` is linked `/MD` (release) rather than
`/MDd`, and imports the release `MSVCP140_2.dll`** — the auxiliary runtime
holding the special-math functions. In the release tree that library is imported
by `Qt6Gui.dll` and `Qt6Quick.dll`.

**This is a property of the build inputs, not of the dependency scan.** Nothing
about the build changed. Previously the scan started only at the executable and
never reached the mismatched binary; now that it seeds from every binary in the
package (which is the point — see `src/rust/packaging/src/deps_walker.rs`), that
binary is reachable and its CRT comes with it.

## What is still unknown

**Which** binary in the debug tree carries the mismatched runtime flag. Finding
that needs a vcpkg debug tree, which was not available while investigating.

## Approach: read the warnings the build now emits

Packaging emits the diagnostics needed to close this out. Both appear in the
build log (CI sets `SHOOP_LOG=packaging=debug` in
`.github/actions/build_package/action.yml`; set it locally too).

1. **The mixed-runtime warning** — fires when both flavours end up bundled:

   ```
   --> Bundling both debug and release Visual C++ runtimes (msvcp140.dll + msvcp140d.dll).
       Two CRTs in one process do not share a heap; this package may crash at
       startup. Some dependency is linked against the other CRT.
   ```

   Implemented in `warn_on_mixed_vc_runtimes`,
   `src/rust/packaging/src/portable_folder_common.rs`.

2. **Per-runtime importer attribution** — names what asked for each one:

   ```
   Runtime msvcp140_2.dll <- Qt6Gui.dll, Qt6Quick.dll (from .../bin/MSVCP140_2.dll)
   Runtime msvcp140.dll   <- MSVCP140_1.dll, MSVCP140_2.dll, Qt6Core.dll (+9 more)
   ```

   Implemented in `log_report_summary`, `src/rust/packaging/src/scan.rs`.

   Because the CRTs import each other, **only the first link identifies the
   culprit.** Look for the runtime whose importers are *not* themselves CRT DLLs —
   in the debug build, whatever imports the release `msvcp140_2.dll`.

The same attribution is available without a full build, against any existing
package folder:

```
package scan-dependencies --folder <dir> --use-cmake-prefix-path --report-only
```

## Candidate fixes, once the binary is identified

- **Rebuild that port with matching runtime flags** in vcpkg. Correct fix if it is
  a project-controlled port or overlay.
- **Exclude it from the debug package** if the component is not needed, the way
  `UNWANTED_QT_PLUGINS` drops the PostgreSQL driver
  (`src/rust/packaging/src/portable_folder_common.rs`).
- **Accept it and stop shipping the debug CRT**, noting the debug CRT DLLs are not
  redistributable in any case. This makes the debug portable folder a
  build-tree-only artifact rather than a self-contained one.

Note that the debug CRT currently resolves from `System32` on the CI runner
because Visual Studio is installed there; it would not resolve on a clean
machine, which is worth keeping in mind when judging how self-contained this
package really is.

## Related: the CI check was narrowed to match

`.github/actions/build_toplevel/action.yml` runs the full UI-loading verification
(`--backend dummy --test-grab-screens`, asserting screenshots exist and the log is
free of loader errors) only for **release** packages. Debug and coverage variants
fall back to the weaker `--help` check:

- **debug** — this issue.
- **coverage** — Qoverage rewrites the QML to import `QoverageSingleton`, which
  belongs to the coverage tooling and is not in the package, so no window is ever
  created and the run hits the timeout.

Restoring the full check for debug is the acceptance criterion for closing this
issue.
