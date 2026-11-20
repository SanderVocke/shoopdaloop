#!/usr/bin/env python3
"""Extract validated clock counts from an RTL benchmark's stdout."""
import json
import re
import sys
from pathlib import Path

scenario, source, target = sys.argv[1:]
assert scenario in ("midi", "mixed")
text = Path(source).read_text()
matches = {
    int(tick): int(cycles)
    for tick, cycles in re.findall(r"quantum (\d+): (\d+) clocks", text)
}
assert all(tick in matches for tick in range(33)), "Incomplete validated window"
measurements = [
    dict(kind="playback_start" if tick == 0 else "steady", quantum=tick,
         position=tick * 32, cycles=matches[tick], us_at_100mhz=matches[tick] / 100)
    for tick in [0, *range(17, 33)]
]
result = dict(
    scenario="stereo_and_midi" if scenario == "mixed" else "midi_only",
    loops=64, sample_rate=48000, quantum_frames=32, chunk_frames=32,
    sequence_frames=1440000,
    memory="two independent 64-bit AXI ports, fixed 7-clock service per 16-byte native request, no random stalls; no board DDR arbitration",
    scope="full compiled SP, mixed queue and common JP; run acceptance through done; host loading/readback excluded",
    preparation="payloads and ready default-state comparison maps loaded outside measured quanta; hardware allocates MIDI descriptors/blocks and exported sample records are checked",
    capture="Startup and quanta 17 through 32 captured from validated stdout of a longer run, intentionally stopped after quantum 32; current example defaults reproduce this window",
    measurements=measurements,
)
Path(target).write_text(json.dumps(result, indent=2) + "\n")
