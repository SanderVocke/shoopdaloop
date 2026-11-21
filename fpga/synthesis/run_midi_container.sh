#!/bin/bash
set -euo pipefail
if test "$#" -ne 3; then
    echo 'Usage: run_midi_container.sh BUILD_DIRECTORY TOOLS_DIRECTORY RUNTIME_IMAGE' >&2
    exit 2
fi
build_dir=$(realpath "$1")
tools_dir=$(realpath "$2")
mkdir -p "$build_dir/tool-home"
test -f "$build_dir/run.tcl"
test -x "$tools_dir/Xilinx/Vivado/2021.1/bin/vivado"
docker run --rm \
    --mount "type=bind,source=$build_dir,target=/build" \
    --mount "type=bind,source=$tools_dir,target=/tools,readonly" \
    --mount "type=bind,source=$build_dir/tool-home,target=/home/agent" \
    -w /tmp -e LD_PRELOAD=/lib/x86_64-linux-gnu/libudev.so.1 \
    -e "MIDI_SAVE_CHECKPOINTS=${MIDI_SAVE_CHECKPOINTS:-0}" \
    "$3" /bin/bash -c '
        set -eu
        mkdir -p /tmp/midi-design
        cp -a /build/source /tmp/midi-design/
        cp /build/context.sv /build/context.xdc /build/run.tcl /tmp/midi-design/
        if test -f /build/input.dcp; then cp /build/input.dcp /tmp/midi-design/; fi
        cd /tmp/midi-design
        set +e
        /tools/Xilinx/Vivado/2021.1/bin/vivado -mode batch -source run.tcl > /build/console.log 2>&1
        status=$?
        set -e
        for artifact in *.rpt *.json *.dcp setup.txt outcome.txt vivado.log vivado.jou; do
            if test -f "$artifact"; then cp "$artifact" /build/; fi
        done
        exit "$status"
    '
