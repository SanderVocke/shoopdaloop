The complete project build is managed by Cargo. To build, you just need to do a `cargo build`.

If it is the final build step to complete a task, first do a `cargo fmt --all`, and in that case run the build with `RUSTFLAGS="-D warnings"`.

If there is a failure in building dependencies from outside the project, you may just assume the development environment was not set up correctly for you. Just report this and stop your task.