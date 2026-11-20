#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} == --catalina ]]; then
    bash scripts/build_macos_catalina_dependencies.sh .catalina-deps
    set -- --macos-dependency-dir .catalina-deps
else
    set --
fi

python3 scripts/carla_runtime.py fetch --asset macos-universal --output carla.dmg
mkdir .carla-volume
hdiutil attach carla.dmg -nobrowse -readonly -mountpoint .carla-volume
trap 'status=$?; hdiutil detach .carla-volume || exit 1; exit "$status"' EXIT
revision=$(python3 -c 'import json; print(json.load(open("third_party/carla/runtime-lock.json"))["git_revision"])')
curl --fail --location --retry 3 -o carla-GPL.txt "https://raw.githubusercontent.com/falkTX/Carla/$revision/doc/GPL.txt"
python3 scripts/carla_runtime.py normalize --platform macos \
    --search-root .carla-volume --license carla-GPL.txt --output .carla-component \
    "$@"
