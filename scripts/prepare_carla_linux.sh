#!/usr/bin/env bash
set -euo pipefail

python3 scripts/carla_runtime.py fetch --asset source --output carla-source.tar.gz
mkdir .carla-source
tar -xzf carla-source.tar.gz -C .carla-source --strip-components=1
header_sha=$(python3 -c 'import json; print(json.load(open("third_party/carla/runtime-lock.json"))["source"]["native_header_sha256"])')
echo "$header_sha  .carla-source/source/includes/CarlaNative.h" | sha256sum --check
cc -std=c11 -I .carla-source/source/includes third_party/carla/abi_probe.c -o .carla-abi-probe
./.carla-abi-probe > .carla-abi.actual
diff -u third_party/carla/abi-x86_64.expected .carla-abi.actual

python3 -m venv .carla-venv
.carla-venv/bin/pip install --disable-pip-version-check 'cx-Freeze==7.2.7' 'PyQt5==5.15.11'
PATH="$PWD/.carla-venv/bin:$PATH" make -C .carla-source -j2 plugin frontend
.carla-venv/bin/python scripts/build_carla_linux_ui.py --source .carla-source --output .carla-frozen-ui
python3 scripts/carla_runtime.py normalize --platform linux \
    --search-root .carla-source/bin --search-root .carla-frozen-ui \
    --license .carla-source/doc/GPL.txt --output .carla-component
readelf -d .carla-component/lib/libcarla_native-plugin.so | grep 'Shared library: \[libsndfile'
