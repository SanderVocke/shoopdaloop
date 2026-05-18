The project has multiple levels of testing:

- C++ unit/integration tests (Catch2): compiled into a separate `test_runner` executable, which results as a side effect from building the `backend` crate (nested somewhere in the `target` folder).
- Rust unit/integration tests: simply `cargo test`.
- QML-based self-tests: these are application-level integration tests. Run using `./target/debug/shoopdaloop_dev.sh --self-test` (or slightly different script names depending on the OS you run).

Note that the `shoopdaloop_dev` script sets up the environment to use all built libraries and dependencies, but also QML code straight from the source tree. Such files are not copied anywhere. Therefore you can make QML changes and run without building first.