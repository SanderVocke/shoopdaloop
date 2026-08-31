# Debugging CI failures

GitHub Actions provides several useful evidence sources through the `gh` CLI:

- `gh pr checks` and `gh run list` locate relevant workflow runs.
- `gh run view` shows run status, head SHA, attempt, matrix jobs, failed steps, and logs.
- `gh run watch` follows an active run.
- `gh run download` retrieves build and diagnostic artifacts.
- `gh api` exposes job and artifact metadata when the higher-level commands are insufficient.

Run attempts share a run ID, so the attempt and head SHA matter when comparing failures or reruns. Matrix peers can also help distinguish platform-specific behavior from a general failure.

## Diagnostic options

Workflow logs usually expose the failing command, environment, test name, compiler output, and surrounding warnings. Artifacts may contain stronger evidence than logs.

The build workflow can upload finalized failure-only Perfetto captures named `perfetto-nextest-<target>-<arch>-<profile>-<run-id>`; Wasm test reports include their Node/Chromium traces and testcase mappings. A trace may reveal the application, engine, realm/thread, timing, and event sequence leading to a failure. Abort, signal, timeout, OOM, and failures outside eligible tests can still prevent complete capture. Read `.agents/skills/perfetto/SKILL.md` before analyzing a downloaded `.pftrace` file.

Other useful options include comparing matrix jobs or attempts, rerunning a suspected flake, reproducing the CI command and environment locally, and applying the resource-contention techniques in `.agents/info/ci-repro.md` for timing-sensitive failures.

Do not expose GitHub tokens, secrets, signed artifact URLs, or sensitive environment values in reports or committed files.
