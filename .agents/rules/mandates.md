# In any new session

- If there are any clear-cut contradictions in your instructions, stop and inform the user to let them clarify.
- Tell the user which instruction md files you have read.

# Before committing

- If rust changes were made: `cargo fmt --all`, and build with `RUSTFLAGS="-D warnings"`

# Before pushing

- If changes were made which can affect program behavior: run the test suites.
