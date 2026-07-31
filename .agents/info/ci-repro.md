# Techniques for reproducing CI resource-contention failures locally

## Context

CI failures that look non-deterministic are often caused or exposed by
resource contention on runners with limited vCPUs, memory, or scheduler time.
Typical symptoms include timeouts, event-loop stalls, thread synchronization
failures, and tests that only fail under CI parallelism.

This note focuses on portable techniques and classes of tools. Install the
specific tools using whatever package manager or platform support is available
on the host.

## First decide: race/contention or deterministic logic bug?

Before adding resource limits, establish a baseline:

1. Run the failing test **10–20 times** with no artificial constraints.
2. If it fails every time, treat it as a deterministic logic bug first.
3. If it passes most runs but fails occasionally, suspect a race or resource
   contention issue.
4. If the failure only appears in CI, compare CI's CPU count, memory limit,
   containerization, test parallelism, and environment variables with local
   runs.

Resource limiting is useful for amplifying rare races; it is usually not needed
for fully deterministic failures.

## Useful tool categories

| Technique | Example tools | What it simulates |
|-----------|---------------|-------------------|
| CPU affinity / pinning | `taskset`, platform CPU-affinity tools | Threads competing on a small set of CPUs |
| Background CPU load | `stress-ng`, `yes`, custom busy loops | A busy runner with noisy neighbors |
| Cgroup or job limits | `systemd-run`, cgroups, container CPU/memory flags | CI-style quotas and memory ceilings |
| Containers | `podman`, `docker`, CI images | A closer match to the CI runtime environment |
| Scheduler priority changes | `nice`, `chrt`, `ionice` | Less favorable scheduling or I/O priority |
| Process limits | `prlimit`, `ulimit` | File descriptor, memory, process-count, or stack limits |
| VMs | QEMU, VirtualBox, cloud VMs | A host that genuinely has fewer vCPUs/RAM |

## Technique 1: limit visible or usable CPUs

Pin the test to a small CPU set so all relevant threads compete for the same
cores.

```bash
QT_QPA_PLATFORM=offscreen taskset -c 0-1 <test-cmd>
```

If the test framework itself launches multiple workers, also reduce its worker
count so the reproduction matches CI intentionally rather than accidentally.

## Technique 2: add background CPU pressure

Run CPU burners on the same CPUs used by the test. `stress-ng` is convenient,
but simple busy loops work when it is unavailable.

```bash
# Simple substitute for stress-ng
for i in 1 2 3; do (yes > /dev/null) & done

QT_QPA_PLATFORM=offscreen taskset -c 0-3 <test-cmd>

# Clean up background jobs from this shell
kill %1 %2 %3
```

This can make driver threads, GUI/update threads, timers, and event loops get
descheduled more aggressively, which helps expose timing assumptions.

## Technique 3: run under cgroup-style CPU and memory limits

Use the host's cgroup/job-control mechanism to apply quotas similar to CI.
For example, on systems with `systemd-run`:

```bash
systemd-run --user --scope \
  --property CPUQuota=200% \
  --property MemoryMax=4G \
  bash -c 'QT_QPA_PLATFORM=offscreen <test-cmd>'
```

Equivalent limits can be applied with raw cgroups, container flags, or OS-specific
job objects. The important part is to constrain both CPU budget and memory to
values comparable to the CI runner.

## Technique 4: use a constrained container

Containers are often the closest local approximation of CI, especially if CI
already runs tests in a container image.

```bash
podman run --rm -it \
  --cpus=2 --cpuset-cpus=0-1 --memory=4g \
  -v "$PWD:/work" -w /work \
  <test-image> bash -c '<test-cmd>'
```

The same idea applies to Docker or other container runtimes. Prefer an image
that matches CI dependencies, compiler/runtime versions, and environment
variables.

## Technique 5: use a small VM

When containers or cgroups do not reproduce the problem, run the test in a VM
configured with CI-like resources, for example 2 vCPUs and 4 GiB RAM. This can
reveal problems hidden by a powerful workstation, different kernels, or different
scheduler behavior. Never do this without asking the user first.

## Reproduction checklist

When documenting a CI-only failure, record:

- Exact test command and environment variables.
- CPU count, CPU affinity, memory limit, and test parallelism.
- Whether background load was used and how it was generated.
- Container image or VM details, if applicable.
- Number of runs and pass/fail count.
- Whether the failure also occurs without resource constraints.
