#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
destination=${1:-"$root/work/verilator"}
jobs=${SHOOP_VERILATOR_JOBS:-2}
revision=8ff77e9d47351b0a59114929880687839a51840b

if [[ ! "$jobs" =~ ^[1-9][0-9]*$ ]]; then
    echo "SHOOP_VERILATOR_JOBS must be a positive integer" >&2
    exit 1
fi
if [[ ! -d "$destination/.git" ]]; then
    git clone --depth 1 --branch v5.032 https://github.com/verilator/verilator.git "$destination"
fi
if [[ $(git -C "$destination" rev-parse HEAD) != "$revision" ]]; then
    echo "Expected Verilator 5.032 ($revision) in $destination" >&2
    exit 1
fi
cd "$destination"
autoconf
./configure
make -C src -j"$jobs" opt
bin/verilator --version
