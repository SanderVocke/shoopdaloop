# In any new session

- If there are any clear-cut contradictions in your instructions, stop and inform the user to let them clarify.
- Tell the user which instruction md files you have read.

# Before committing

- If rust changes were made: `cargo fmt --all`, and build with `RUSTFLAGS="-D warnings"`

# Before pushing

- If changes were made which can affect program behavior: run the test suites.

# When encountering a situation where the instructions given were plainly wrong w.r.t the codebase content

- Stop and notify the user.

# Overriding

- These rules may be overruled by the user instructions. If you see a strong reason to overrule them yourself, ask explicit permission from the user.