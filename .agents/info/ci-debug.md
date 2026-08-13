# Debugging CI failures

Inspect the remote evidence before changing code or trying to reproduce a failure. Record the run URL, head commit, attempt, failing matrix job, failed step, and exact command. A workflow failure can come from source code, test behavior, runner resources, packaging, browser infrastructure, or an external service.

## Find the relevant run

Authenticate and verify the repository first:

```sh
gh auth status
gh repo view --json nameWithOwner,url
```

For a pull request, start with its checks:

```sh
gh pr checks <pr-number>
```

For branch or manually dispatched runs:

```sh
gh run list \
  --workflow build_and_test.yml \
  --branch "$(git branch --show-current)" \
  --limit 10
```

Always confirm that the run tested the expected commit. Do not diagnose an older run as if it represented the current checkout.

```sh
RUN_ID=<run-id>
gh run view "$RUN_ID" \
  --json status,conclusion,event,attempt,headBranch,headSha,url,jobs
```

While a run is active, follow it with:

```sh
gh run watch "$RUN_ID" --interval 30
```

Use `--exit-status` when a script should fail if the workflow fails.

## Read logs

Start with all failed steps:

```sh
gh run view "$RUN_ID" --log-failed
```

For a noisy matrix, get job IDs from the JSON view and inspect one job:

```sh
gh run view "$RUN_ID" --json jobs \
  --jq '.jobs[] | [.databaseId, .name, .conclusion] | @tsv'
gh run view "$RUN_ID" --job <job-id> --log > /tmp/ci-job.log
```

Search from the first meaningful error, not only the final nonzero-exit summary. Capture enough preceding output to identify the command, environment, compiler or test name, and earlier warnings. Compare equivalent jobs in the same matrix when platform- or profile-specific behavior is suspected.

If logs are incomplete or `gh run view` cannot retrieve them, inspect the Actions API directly:

```sh
gh api "repos/{owner}/{repo}/actions/runs/$RUN_ID/jobs?filter=all"
```

A rerun keeps the run ID but creates another attempt. Record which attempt supplied the evidence. Avoid rerunning before preserving transient logs and artifacts; a green rerun is evidence of nondeterminism, not proof that the first failure was irrelevant.

## Inspect and download artifacts

List artifacts before assuming the logs are the only evidence:

```sh
gh api "repos/{owner}/{repo}/actions/runs/$RUN_ID/artifacts" \
  --jq '.artifacts[] | [.name, .size_in_bytes, .expired] | @tsv'
```

Download one artifact or a name pattern:

```sh
gh run download "$RUN_ID" --name <artifact-name> --dir /tmp/ci-artifact
gh run download "$RUN_ID" --pattern '<artifact-pattern>' --dir /tmp/ci-artifacts
```

The build workflow may upload finalized failure-only Tracy captures named `tracy-nextest-<target>-<arch>-<profile>-<run-id>`. These artifacts exist only when an eligible captured test publishes a trace; many CI failures therefore have no trace. Read `.agents/skills/tracy/SKILL.md` before validating and investigating any downloaded `.tracy` file.

## Decide the next action

1. Confirm the exact head SHA and workflow attempt.
2. Identify the first failing job and step, plus failures that are downstream or cancelled.
3. Save relevant logs and artifacts.
4. Classify the failure as deterministic code/test behavior, platform-specific behavior, likely resource contention, browser/service infrastructure, or unknown.
5. Reproduce the exact command and environment locally when practical.
6. If the failure appears timing- or resource-sensitive, use the techniques in `.agents/info/ci-repro.md` and report the number of repeated runs.
7. After a fix, verify the targeted failure and the required broader suite. If relying on a rerun without a code change, state both attempts and why the evidence supports a flake classification.

Do not expose GitHub tokens, secrets, signed artifact URLs, or sensitive environment values in reports or committed files.
