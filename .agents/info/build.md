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