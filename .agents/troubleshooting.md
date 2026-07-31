# Troubleshooting

Do not read this if you are looking for the test instructions or build instructions.
Read it when having runtime issues running the tests or app, which seem unrelated to
the task at hand.

## QML self-tests: `shoopdaloop_dev.sh` vs running the binary directly

The `shoopdaloop_dev.sh` launcher is generated in `target/debug/` by
`src/rust/shoopdaloop/build.rs` at build time. It sets `SHOOP_CONFIG` to the dev
config TOML so the test runner can find the `src/qml/test/` directory (without it
the test runner reports 0 testcases because the default QML path is empty).

Always invoke the QML self-tests through the dev launcher:

```
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh \
  --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_TwoLoops.qml" \
  --junit-xml /tmp/qml_test_results/r1.xml
```

Running the binary directly (`target/debug/shoopdaloop`) will silently produce 0
testcases because neither `SHOOP_CONFIG` nor `SHOOP_QML_PATHS` are set.

### Filtering to individual testcases

`--list` is known to be broken (the filter regex is set to `^$` when listing, so
it matches nothing). Use `--filter` instead:

```
--filter 'CompositeLoop_running::test_sequential'
```

### Common pitfalls

- If the test binary hangs with `"Created invalid object"`, the `ShoopTestFile`
  QML failed to load. Usually this means the QML import path is incomplete —
  verify `SHOOP_CONFIG` points at the dev config TOML.
- `"Could not find top-level QQuickWindow to connect back-end refresh"` is a
  warning; with `QT_QPA_PLATFORM=offscreen` it is expected and does not prevent
  the tests from running.
