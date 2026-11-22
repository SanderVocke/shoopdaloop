# Building

Cargo manages the project build. The default build command is:

```sh
cargo build
```

Choose how to run that command according to the execution environment. Commands in other agent documentation are payloads to run in the same selected environment.

## CI

When running inside CI (GitHub Actions), use the environment provisioned by the current job. Do not enter the Nix development shell or install replacement dependencies.

## Nix and NixOS

Outside CI, when executing in a Nix or NixOS environment, use the repository flake instead of host-installed build tools:

```sh
nix develop
cargo build
```

For a non-interactive command, use:

```sh
nix develop --command cargo build
```

Keep subsequent Cargo, Python, Trunk, and test commands inside that shell. The shell supplies the Rust toolchain, native and WebAssembly build dependencies, development tools, runtime library paths, Carla runtime selection, and JACK provider handling.

## Other environments

For other environments, including Windows, macOS, and browser or web development, determine the appropriate setup from the checked-out project, available toolchain, and intended target. No package manager, environment bootstrap, or target-specific command is prescribed here. Prefer the basic Cargo command unless the target's own build entry point requires otherwise.

## Vivado 2021.1 (explicit FPGA checks only)

Download/install Vivado **only when the user specifically requests Vivado FPGA
work and checks**, such as synthesis, implementation, utilization or timing.
Ordinary RTL changes, Rust reference work and Verilator/Marlin simulation do not
require downloading it. Reuse an existing installation when available.

Previous FPGA measurements used the public Docker Hub image
[`siliconbootcamp/xilinx-vivado:stable`](https://hub.docker.com/r/siliconbootcamp/xilinx-vivado),
pinned to this digest rather than the mutable tag:

```sh
docker pull docker.io/siliconbootcamp/xilinx-vivado@sha256:d24586405894b07f6af006d5cb15ba01c7e8585de97b28b6121fcf6282db7a5f
```

Verify Vivado **2021.1, SW build 3247384**. Prior builds targeted Zynq-7020
`xc7z020clg484-1`. See the
[measurement reproduction notes](../../docs/fpga/measurements/midi-expression-state/README.md)
and [container runner](../../fpga/synthesis/run_midi_container.sh).
The runner expects extracted tools at
`TOOLS_DIRECTORY/Xilinx/Vivado/2021.1/bin/vivado` and a compatible Linux runtime
image. The runtime-image SHA recorded in the measurements is local provenance,
not a public image to pull.
