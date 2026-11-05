"""Generate native profiles using the workspace's session protocol version."""
import json
from pathlib import Path
import re


def write_profile(path: Path, credential: Path, port_base: int):
    root = Path(__file__).resolve().parents[2]
    protocol = (root / "src/rust/shoop_app_protocol/src/lib.rs").read_text()
    version = re.search(r"^pub const VERSION: u16 = (\d+);$", protocol, re.MULTILINE)
    if version is None:
        raise RuntimeError("Cannot determine the workspace session protocol version")
    path.write_text(json.dumps({
        "version": int(version[1]), "name": "Zynq QEMU", "credential": str(credential),
        "endpoint": f"127.0.0.1:{port_base}", "management": f"127.0.0.1:{port_base+1}",
        "network": {"heartbeat_interval_ms": 1000, "heartbeat_timeout_ms": 5000, "snapshot_hz": 30},
    }, indent=2) + "\n")
