# Techniques for reproducing CI resource-contention failures locally

## Context

CI failures that present as seemingly non-deterministic failures (e.g.
composite loop engine stalls) are most likely caused by resource contention
on CI runners with limited vCPUs. The following techniques were attempted
to reproduce a failure locally on a 16-core NixOS workstation.

## Outcome

The failure was found to be **fully deterministic** and reproduced without
any resource limiting — the root cause was a logic bug (graph mutations
that bump `graph_request_id` without calling `apply_graph_changes()`), not
a timing race. The techniques below remain documented for future use when
genuine race conditions need to be provoked.

## Available tools on NixOS

| Tool | Available | Notes |
|------|-----------|-------|
| `systemd-run --scope` | ✓ | CPUQuota/MemoryMax via cgroups |
| `taskset` | ✓ | CPU affinity pinning |
| `nice` / `chrt` / `ionice` | ✓ | Scheduler priorities |
| `prlimit` | ✓ | Process resource limits |
| `podman` | ✓ | Rootless container with cgroup limits |
| `qemu-system-x86_64` | ✓ | Full VM with limited vCPUs |
| `stress-ng` | ✗ | Not installed; `yes > /dev/null` as substitute |
| `cpulimit` | ✗ | Not installed |
| `Docker` | ✗ | Not installed |

## Approach 1: `systemd-run --scope` with `CPUQuota`

Target: simulate a CI runner with limited CPU budget.

```bash
systemd-run --user --scope \
  --property CPUQuota=200% \
  --property MemoryMax=4G \
  bash -c 'QT_QPA_PLATFORM=offscreen <test-cmd>'
```

**Failed on this box**: `systemd-run --user --scope` returned
`Failed to connect to user scope bus via local transport: No data available`
(systemd 258 on NixOS with userdb cgroup controller in a non-standard
configuration).

## Approach 2: `taskset` + CPU-burning background load

Target: make the test's threads contend for the same physical cores.

```bash
# Saturate cores with `yes`
for i in 1 2 3; do (yes > /dev/null) & done

# Pin test to those same cores
QT_QPA_PLATFORM=offscreen taskset -c 0-3 <test-cmd>

kill %%  # clean up
```

Works reliably and requires no root. The `yes` processes compete for
CPU on the pinned cores, causing the dummy driver thread and the
update/GUI threads to be descheduled more aggressively.

**Verdict**: Not needed for this particular bug (it was deterministic),
but confirmed as a usable technique for genuinely racy failures.

## Approach 3: `podman` rootless container

Target: run the test binary inside a container with explicit cgroup limits.

```bash
podman run --rm -it \
  --cpus=2 --cpuset-cpus=0 --memory=4g \
  -v "$PWD:/work:z" -w /work \
  <test-image> bash -c '<test-cmd>'
```

Requires a container image with the test binary and its dependencies
pre-installed. Would match CI closest because the container's own init
process sees only 2 vCPUs, so the kernel scheduler naturally treats
all threads as competing within that budget.

**Verdict**: Not attempted (deterministic bug found without it).
Still the recommended approach for hard-to-reproduce CI races because
it eliminates any "but my machine has more cores" discrepancy.

## How to determine whether a failure is a race vs a logic bug

1. Run the failing test **10–20 times** without any resource constraints.
2. If it fails every time → logic bug, skip resource limiting.
3. If it passes most runs but fails occasionally → likely a race, proceed
   with one of the approaches above.
4. To amplify a race: pin to 1–2 CPUs with `taskset`, or run `podman`
   with `--cpus=2`.
