# Repeat the UDP/TCP comparison across a cable

The same runner works on localhost and with an explicitly selected SSH server:
[`scripts/benchmark_remote_dsp_transports.py`](../../scripts/benchmark_remote_dsp_transports.py).
It runs the requester on the machine invoking it and launches a fresh server on
the other machine for each case. **Audio and control connect directly to the
server's cable IP, not through an SSH tunnel.** SSH only launches the process and
collects its readiness/error output.

The [recorded TCP comparison](transport-results.md) is localhost evidence, not a
measurement of two NICs. A second physical machine was not available for that
capture. The remote command construction is unit tested; the physical cable/SSH
workflow still needs to be exercised on the intended pair.

## What stays matched

| Variant | Transport | Incomplete input block |
| --- | --- | --- |
| `udp-gap` | UDP | Existing `(k-1)*P/fs + 200 us` zero-substitution policy |
| `udp-complete` | UDP | Wait for all original input; no substitution |
| `tcp-complete` | Framed TCP, `TCP_NODELAY` at both ends | Wait for all original input; no substitution |

All three use yielding polling, identical packet/sample data, DSP configuration,
output deadlines, warm-up, and catch-up without deliberately skipping overdue
source packets. TCP has a separate authenticated audio connection, not audio
embedded in the control stream. There is no independently clocked server audio
engine. Missing **output** at the requester still produces the existing fixed-slot
fallback; TCP does not extend deadlines.

The default `all` profile contains:

- `P/k/N`: **8/1/4, 8/1/8, 16/4/4, 16/4/8**, stereo, 48 kHz, delay-copy `A=0`.
  Modeled audio latencies: **0.833, 1.500, 1.667, 3.000 ms** respectively.
- Ordinary `SCHED_OTHER`, unpinned within each endpoint's allowed CPU mask.
- Pinned `SCHED_FIFO` priority **1**, only if **both** endpoints actually permit it.
  The manifest preserves the probe result and each result records the actual
  requester and worker policy, priority, niceness and affinity.
- Three shuffled **2-second** repetitions per condition, then one fresh,
  independently shuffled **60-second** run of **every** condition. No selecting
  only successful or fastest cases for the long runs.
- Six separately labeled **1-second source-pause runs**, at `16/4/8` by default.
  Dispatch sleeps once for **2 ms**, immediately after submitting the first
  measured packet. Deadlines do not move; the requester catches up afterwards.
  This isolates the input-policy consequence of a scheduling-like source pause.
  It is **not network packet loss**, and is not mixed into the ordinary long-run
  results. Set `--stall-us 0` to omit it.

With FIFO permitted, this is **102 records**, approximately **28 minutes** plus
setup/reporting. Without FIFO, it is 51 records. `screen` runs just the short
matrix; `smoke` runs one 50 ms case per variant/scheduling combination. Latency
misses and retirement are retained as outcomes, not replaced with reruns or used
as brittle CI pass/fail timing thresholds. Infrastructure/verification failures
stop the campaign and leave an explicitly partial report.

## 1. Prepare both machines before measuring

Use Linux, the same repository source, and its Nix environment on both machines.
Build on each machine rather than assuming a copied Nix-linked executable will
run on the other installation:

```sh
nix develop --command cargo build --release -p shoop_dsp_host --no-default-features
nix develop --command cargo test \
  -p shoop_dsp_host -p shoop_remote_dsp -p shoop_dsp -p shoop_transport_socket
```

If reproducing an uncommitted capture, review its published `implementation.patch`
and manifest revision. The patch includes new Rust/Python files as well as tracked
changes; apply it only to the matching base, not over an already updated tree.
The transport capture also supplies a separate `runner-maintenance.patch` for
post-capture cleanup/validation improvements; apply it after the implementation
patch if reconstructing the current runner rather than the exact capture runner.
The manifest records executable and relevant source-file hashes. Different
machines/architectures can legitimately produce different executable hashes, but
both builds must implement the same protocol and experiment.

The controller needs Python **3.11+**, `ssh`, `taskset`, `chrt`, and `timeout`
(the repository Nix shell supplies the Python version). The server
needs the built executable, Python 3 (`--server-python` selects it), `taskset`,
`chrt`, and `timeout` in the noninteractive SSH PATH. `ip` provides optional network
inventory; a missing inventory utility is recorded rather than installed. SSH
must work without interactive prompts: the runner uses `BatchMode=yes`. Configure
and verify the server's host key and login beforehand. It does not use `sudo`.

Finish builds/tests before starting a capture. Avoid simultaneous builds, other
benchmark runs, power-profile changes, and heavy background load. Prefer AC power
and record any deliberate host tuning. Affinity is not CPU isolation: NIC IRQs,
SMT siblings and other processes can still interfere.

Check each endpoint's CPU topology and allowed mask; choose appropriate CPUs:

```sh
nix develop --command python3 -c \
  'import os; print(sorted(os.sched_getaffinity(0)))'
lscpu -e=CPU,CORE,SOCKET,ONLINE
```

For localhost, select different physical cores, not just different SMT siblings.
Across machines, CPU numbers are independent and may be identical. FIFO is
optional: the runner probes the actual policy/priority and omits it if denied on
either endpoint. Do not interpret a launch command alone as proof of RT scheduling.
No limits, capabilities, kernel parameters or host scheduling defaults are changed.

## 2. Prepare the dedicated link and credentials

Connect the two NICs and configure a private subnet using the machines' normal
network administration. For the examples below:

- requester cable address: `192.0.2.1/24`;
- server cable address: `192.0.2.2/24`;
- SSH destination: `user@SERVER` (management may use a different interface);
- remote checkout: `/home/user/dev/shoopdaloop`;
- control TCP port: **47000**;
- audio port: **47001**, TCP or UDP according to the case.

These are documentation addresses and placeholders, not addresses configured by
the runner. Substitute actual interface names, addresses and paths. Check that
routes to the peer's cable address use the intended NIC, the link is up at the
expected speed, and both MTUs agree:

```sh
ip route get 192.0.2.2                    # requester
ip route get 192.0.2.1                    # server
ip -s link show dev YOUR_CABLE_INTERFACE
ping -c 5 192.0.2.2                       # requester, if ICMP is allowed
```

Use a dedicated test link and permit the requester to initiate TCP to the two
server ports and UDP to the audio port, with corresponding return traffic. The
runner does **not** change addresses, routes, MTU, firewalls, NIC offloads, IRQ
placement, or queue disciplines. Authentication is not encryption; use a trusted
link. Before comparing NIC behavior, also record driver/firmware, link speed,
interrupt/coalescing and offload settings with the platform's NIC tools.

Create one private credential on the requester and transfer it securely to the
server. Do not put it in Git, the published evidence directory, command-line token
arguments or logs:

```sh
mkdir -p artifacts/cable-dsp
./target/release/shoop_dsp_host generate-credential artifacts/cable-dsp/credential.key
ssh user@SERVER \
  'umask 077; mkdir -p /home/user/dev/shoopdaloop/artifacts/cable-dsp'
scp artifacts/cable-dsp/credential.key \
  user@SERVER:/home/user/dev/shoopdaloop/artifacts/cable-dsp/credential.key
ssh user@SERVER \
  'chmod 600 /home/user/dev/shoopdaloop/artifacts/cable-dsp/credential.key'
```

Use fresh paths or an already prepared private key; generation does not overwrite
an existing file. Each file must be owned by its user, protected, regular and not
a symlink. The runner checks that both files contain the same credential, but does
not publish its content or hash. TCP audio credentials are generated separately
for each configured stream and are not published either.

## 3. Run a smoke pair, then the full comparison

On the requester, replacing **all** example host/path/CPU values:

```sh
nix develop --command python3 scripts/benchmark_remote_dsp_transports.py \
  --profile smoke \
  --credential-file artifacts/cable-dsp/credential.key \
  --server-ssh user@SERVER \
  --server-root /home/user/dev/shoopdaloop \
  --server-binary target/release/shoop_dsp_host \
  --server-credential artifacts/cable-dsp/credential.key \
  --server-bind 192.0.2.2:47000 --data-port 47001 \
  --client-cpu 2 --server-cpu 4 \
  --output artifacts/cable-dsp-smoke
```

The full command is identical except for the profile/output:

```sh
nix develop --command python3 scripts/benchmark_remote_dsp_transports.py \
  --profile all \
  --credential-file artifacts/cable-dsp/credential.key \
  --server-ssh user@SERVER \
  --server-root /home/user/dev/shoopdaloop \
  --server-binary target/release/shoop_dsp_host \
  --server-credential artifacts/cable-dsp/credential.key \
  --server-bind 192.0.2.2:47000 --data-port 47001 \
  --client-cpu 2 --server-cpu 4 \
  --output artifacts/cable-dsp-full \
  --publish docs/remote-dsp/cable-evidence
```

`--server-root` must be absolute. Paths for `--server-binary` and
`--server-credential` may be absolute or relative to that root. SSH commands are
shell-quoted. Use `--ssh-port` for a nondefault
SSH port. `--binary` selects the local executable. `--server-bind` must be a
concrete, reachable, non-loopback address in SSH mode. The server is automatically
launched with `--lan --once`, the requested data port, and a bounded lifetime.
There must be no independently running service on those ports.

Repeat `--config P,k,N` to replace the default configuration list, for example
`--config 16,4,8 --config 32,4,8`. Other controls include `--sample-rate`,
`--channels`, `--delay`, `--mtu`, `--warmup`, `--screen-seconds`,
`--followup-seconds`, `--repetitions`, `--seed` and `--stall-us`. Keep them fixed
when comparing hosts. TCP intentionally retains the same conservative logical
frame-size admission as UDP for these comparisons; `--mtu` does not reconfigure
an interface or measure path MTU.

For localhost, omit all `--server-*`/SSH flags and the shared credential options:

```sh
nix develop --command python3 scripts/benchmark_remote_dsp_transports.py \
  --client-cpu 2 --server-cpu 4 --output artifacts/local-dsp-transports
```

It generates a private credential inside that ignored output directory.

## 4. Resume, inspect, and publish

Repeat the **same command and arguments** to resume. Completed cases are cached,
including bad timing outcomes. Changed binaries, relevant sources, arguments or
host/scheduling availability require a new output directory. A completed capture's
end inventory/timestamp and archived payload are preserved on a cached resume.
Report regeneration needs no live server, credentials or executable:

```sh
nix develop --command python3 scripts/benchmark_remote_dsp_transports.py \
  --report-only --output artifacts/cable-dsp-full \
  --publish docs/remote-dsp/cable-evidence
```

The report-only command also works with the published evidence directory as its
input. Raw output contains per-case JSON and client/server logs, a credential,
manifest and completion inventory. Publication copies only an explicit whitelist:
manifest, completion inventory, implementation patch, compressed JSONL archive,
CSV, audited totals and Markdown tables. Review inventories and paths for privacy
before sharing them. Inspect `audit.json` for completeness and verification errors,
not just the best row in the table.

The JSONL preserves individual histograms, server retirement/concealment, actual
thread scheduling, socket buffers, TCP queue counters and endpoint addresses.
The CSV gives individual runs, modeled latency and p99 RTT/batch/sender timings.
Long-run outcomes are shown separately from short repetitions and intentional
source pauses. Child-process CPU cost includes both processes locally; over SSH
it covers requester plus SSH and **excludes remote-server CPU**, explicitly labeled
in each result. It must not be used as a two-host CPU-efficiency comparison.

## Interpretation and limits

- Modeled audio latency is `((N+1)*P+A)/fs`, assuming sample-paced playback at
  fixed consumption deadlines. It includes packetization and algorithmic delay;
  aggregation and turnaround must fit inside `N`, not be added again. Audio
  hardware, ADC/DAC, callback/driver buffering and clock-domain adaptation are not
  measured. No audio devices are opened.
- RTT and deadline availability use only the requester's monotonic clock; DSP
  processing duration is local to the server. No NTP/PTP synchronization is
  required. These are application observations, not hardware NIC timestamps or
  one-way cable propagation measurements.
- `udp-complete` is a diagnostic control, **not reliable UDP**. A genuinely missing
  datagram can hold the next block until bounded-window overflow/inactivity
  retires the stream. TCP can retransmit lost network bytes while the connection
  remains viable, but ordered delivery can hold later data behind a missing byte.
- Zero input concealment does not mean zero late/missing outputs. A late but
  sample-correct result cannot repair a slot that has already played fallback.
  Conversely, a timely UDP-gap output may contain substituted input rather than
  the original sound. Check both columns.
- The 50 ms diagnostic receive grace never extends playback deadlines. Results
  absent at that cutoff are called missing. Queued TCP bytes and application
  submission counts do not establish delivery to the peer before cutoff.
- The controlled source pause is not a test of NIC drops, congestion, real TCP
  retransmission or head-of-line blocking under injected wire loss. Those need
  separately labeled, matched link/fault experiments. Host-wide network counters
  are captured before/after, but include unrelated traffic and are not per-stream
  packet-loss counts. The runner does not inject network faults automatically.
- One healthy minute is evidence about that minute/configuration, not a universal
  hard-real-time guarantee or a conclusion that TCP is always better than UDP.
  Real Carla/plugin correctness is tested separately; these performance cases use
  the direct delay-copy processor, not a representative expensive plugin chain.
