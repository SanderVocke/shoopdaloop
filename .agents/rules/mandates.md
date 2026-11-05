# In any new session

- If there are any clear-cut contradictions in your instructions, stop and inform the user to let them clarify.
- Tell the user which instruction md files you have read.

# Before committing

- If rust changes were made: `cargo fmt --all`, and build with `RUSTFLAGS="-D warnings"`
- If Rust tests changed, run `python3 scripts/check_shoop_test_usage.py`.

# Before pushing

- If changes were made which can affect program behavior: run the test suites. Exceptions can be made if you are confident - mention explicitly. Unless asked otherwise, only run tests for the platform you are on - generally rely on CI for cross-platform checks. During iterative development, use targeted limited testing strategies and keep the full suite for major milestones.

# PR management

- Unless explicitly asked or especially obvious, don't autonomously decide to merge a PR you deem finished.

# When encountering a situation where the instructions given were plainly wrong w.r.t the codebase content

- Stop and notify the user.

# Overriding

- These rules may be overruled by the user instructions. If you see a strong reason to overrule them yourself, ask explicit permission from the user.
