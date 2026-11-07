"""Shared on-card geometry and generated C constants."""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import boards

ROOT = Path(__file__).resolve().parent
LAYOUT = json.loads((ROOT / "layout.json").read_text())
LAYOUT["profile"] = boards.load().profile


def header():
    values = {
        "VERSION": LAYOUT["version"], "CARD_SECTORS": LAYOUT["card_bytes"] // 512,
        "BUNDLE_MAX": LAYOUT["bundle_max_bytes"],
    }
    for name in ["boot", "metadata", "slot_a", "slot_b"]:
        for field in ["lba", "sectors"]:
            values[(name + "_" + field).upper()] = LAYOUT[name][field]
    for name in ["config", "state", "counter"]:
        for index, offset in enumerate(LAYOUT[name + "_offsets"]):
            values[f"{name.upper()}_{index}_LBA"] = LAYOUT["metadata"]["lba"] + offset
    lines = ["/* Generated from layout.json; regenerate with layout.py. */",
             "#ifndef SHOOP_DEPLOY_LAYOUT_H", "#define SHOOP_DEPLOY_LAYOUT_H"]
    lines += [f"#define SHOOP_{key} {value}u" for key, value in values.items()]
    lines += ['#include <shoop_board.h>', '#define SHOOP_PROFILE SHOOP_BOARD_PROFILE', "#endif", ""]
    return "\n".join(lines)


if __name__ == "__main__":
    (ROOT / "shoop_layout.h").write_text(header())
