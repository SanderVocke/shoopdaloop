"""Shared on-card geometry and generated C constants."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
LAYOUT = json.loads((ROOT / "layout.json").read_text())


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
    lines += ['#define SHOOP_PROFILE "' + LAYOUT["profile"] + '"', "#endif", ""]
    return "\n".join(lines)


if __name__ == "__main__":
    (ROOT / "layout.h").write_text(header())
