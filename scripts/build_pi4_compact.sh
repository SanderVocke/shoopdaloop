#!/usr/bin/env bash
set -euo pipefail

if [[ "${SHOOP_CONSTRAINED_BUILD:-}" != "1" ]]; then
    export SHOOP_CONSTRAINED_BUILD=1
    exec nice -n 15 taskset --cpu-list "${SHOOP_BUILD_CPUS:-0,1}" "$0" "$@"
fi

export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/root/compact-perf-target}"
export CMAKE_BUILD_PARALLEL_LEVEL=2
export MAKEFLAGS=-j2
export NINJAFLAGS=-j2
export NUM_JOBS=2
export RAYON_NUM_THREADS=2
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export RUSTFLAGS="-D warnings -C target-cpu=cortex-a72 -C target-feature=-aes,-sha2 -C force-unwind-tables=yes"

cargo build --locked --release -p shoop_host --features testing \
    --bin shoop_host --example dense_chunk_benchmark \
    --target aarch64-unknown-linux-gnu "$@"

host="$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release/shoop_host"
launcher="$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release/examples/dense_chunk_benchmark"
file "$host" "$launcher"
sha256sum "$host" "$launcher"
