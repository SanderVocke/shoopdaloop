#!/usr/bin/env python3
"""Generate the directly sequenced content store; no programmable MIDI engine."""
import argparse
from pathlib import Path

root = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
text = (root / 'program/midi_content_control.sv').read_text().replace('module midi_content_control #(', 'module midi_store #(', 1)
path = root / 'rtl/midi_store.sv'
if args.check:
    if path.read_text() != text:
        raise SystemExit('Stale generated midi_store')
else:
    path.write_text(text)
