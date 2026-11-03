# Graph-to-DSP-service demonstration

This executable runs an actual `rt_runtime` graph through the portable link node.
`serve` reuses the existing native DSP server. `run` uses either the in-memory
reference service or a background UDP adapter. Neither path opens an audio device.

```sh
cargo build -p shoop_dsp_link_demo
python3 scripts/smoke_dsp_link.py --binary target/debug/shoop_dsp_link_demo \
  --output work/dsp-link-smoke
```

On Windows, append `.exe` to the executable path.

The smoke script checks Q=3/B=5 without networking and real UDP with graph
callbacks smaller/larger than P=16. It saves commands, source/binary identity,
sample checks, fallback/concealment counters and process outcomes. It also creates
a private credential; do not publish that file or raw logs with evidence.

For native Carla, prepare the pinned runtime using repository guidance, then:

```sh
cargo build -p shoop_dsp_link_demo --features carla
python3 scripts/smoke_dsp_link.py --binary target/debug/shoop_dsp_link_demo \
  --carla --paced --output work/dsp-link-full
```

Carla validation loads Audio Gain at 0.5 and saves/restores its state through the
same graph node. That fixture smooths gain from zero, so verification explicitly
reports 4096 warm-up source frames and uses the existing 0.0001 gain tolerance.
Delay-copy checks are bit-exact. Unknown plugin latency remains unknown even when
a known gain fixture is used as an oracle.

`run --mode stepped` is simulated-time correctness: the host waits for completed
DSP batches **between** graph calls. The render callback never waits. This mode
does not establish a real-time latency. `--mode paced` instead follows a software
clock, retains every fallback/concealment outcome and makes no timing guarantee.
The optional paced run is excluded from CI's correctness smoke subset.

To connect manually, generate a credential, start the server, and use the printed
control endpoint in another terminal:

```sh
mkdir -p work
target/debug/shoop_dsp_link_demo generate-credential work/link-key
target/debug/shoop_dsp_link_demo serve --credential-file work/link-key
target/debug/shoop_dsp_link_demo run --credential-file work/link-key \
  --endpoint 127.0.0.1:PORT --callback-frames 64 --packet-frames 16 \
  --multiplier 4 --allowance 64 --frames 4096 --delay 35
```

`--quantum`/`--dsp-frames` select reference-service Q/B; UDP derives Q=P and B=kP.
`--latency-frames` selects L, or the descriptor's conservative minimum is used.
The JSON report separates link L, known/unknown DSP delay, processed, concealed,
fallback, warm-up and unverified samples. No adaptive delay or dry bypass is used.

## Platform bindings

The portable crates depend only on DSP/runtime contracts and prepared memory
queues. A Zynq service can pump endpoint work between render interrupts or in a
separate execution context, using its own Ethernet/IP implementation. It receives
the same prepared descriptor; no Linux sockets, threads or OS clock enter the
node. DMA, cache maintenance, coherency and notifications belong to that adapter.

A future shared-memory IPC binding can supply slot leases with validated IDs,
offsets, layout version and incarnation. Setup/peer-death cleanup and mappings
stay with its platform owner until node retirement. A socket bridge instead
frames logical blocks and manages partial reads/backpressure outside rendering.
Control and audio carriers may differ. Production IPC and Zynq board drivers are
not implemented by this demo; the reference binding proves the network-free core.
