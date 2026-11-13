"""Packed architectural ABI; no transition settlement is implemented here."""
ERRORS = [None, "invalid_config", "capacity", "invalid_id", "duplicate_id",
          "invalid_state", "invalid_frame", "missing_sync", "exhausted",
          "empty_content", "unsupported"]
OPS = ["stop", "play", "record", "seek"]
MODES = ["stopped", "playing", "recording"]
OUTCOMES = [None, "applied", "superseded", "cancelled", "failed"]
STATUS, WRITE_SPEC, WRITE_ORDER, PREPARE, SUBMIT, CANCEL, COLLECT = range(7)
INSTALL = 4
PROCESS, READ_STATE, READ_REQUEST, READ_EVENT = 9, 12, 13, 15


def bits(value, offset, width):
    return (value >> offset) & ((1 << width) - 1)


def identity(value):
    return value["slot"] | (value["generation"] << 32)


def decode_identity(value):
    return {"slot": bits(value, 0, 32), "generation": bits(value, 32, 32)}


def encode_spec(spec):
    if spec is None:
        return 0
    return (1 | (spec["id"]["generation"] << 1)
            | (int(spec["sync_source"] is not None) << 33)
            | ((identity(spec["sync_source"]) if spec["sync_source"] else 0) << 34)
            | (int(spec["repeat_sync"]) << 98)
            | (int(spec["kind"] == "composite") << 99)
            | (int(spec["timeline"] is not None) << 101)
            | (spec["length"] << 128))


def encode_command(command):
    timing, op = command["timing"], command["operation"]
    return ((int(timing["kind"] == "after_ticks") << 33)
            | (OPS.index(op["kind"]) << 34)
            | (int(op.get("retrigger", False)) << 36)
            | (identity(command["target"]) << 64)
            | (command["sequence"] << 128) | (command["at"] << 192)
            | ((identity(timing["source"]) if "source" in timing else 0) << 256)
            | (timing.get("skip", 0) << 320) | (op.get("position", 0) << 384))


def decode_operation(kind, position, retrigger):
    result = {"kind": OPS[kind]}
    if kind in (1, 3):
        result.update(position=position, retrigger=bool(retrigger))
    return result


def decode_state(value, slot):
    if not bits(value, 0, 1):
        return None
    result = {"id": {"slot": slot, "generation": bits(value, 1, 32)},
              "mode": MODES[bits(value, 33, 2)], "repeat_sync": bool(bits(value, 35, 1)),
              "last_pulse": bits(value, 512, 64) if bits(value, 36, 1) else None}
    for offset, field in enumerate(("content_length", "position", "capture_frames", "cycles",
                                   "content_revision", "capture_revision", "revision_watermark"), 1):
        result[field] = bits(value, 64 * offset, 64)
    return result


def decode_request(value, slot):
    if not bits(value, 0, 1):
        return None
    ticks = bool(bits(value, 33, 1))
    timing = {"kind": "after_ticks", "source": decode_identity(value >> 256), "skip": bits(value, 320, 64)} if ticks else {"kind": "at"}
    command = {"target": decode_identity(value >> 64), "sequence": bits(value, 128, 64),
               "at": bits(value, 192, 64), "timing": timing,
               "operation": decode_operation(bits(value, 34, 2), bits(value, 384, 64), bits(value, 36, 1))}
    kind = bits(value, 37, 3)
    outcome = {"kind": OUTCOMES[kind], "frame": bits(value, 576, 64)} if kind else None
    if kind == 4:
        outcome["reason"] = ERRORS[bits(value, 40, 4)]
    return {"id": {"slot": slot, "generation": bits(value, 1, 32)}, "command": command,
            "effective": bits(value, 448, 64), "remaining_ticks": bits(value, 512, 64) if ticks and not kind else None,
            "outcome": outcome}


def decode_completion(value, slot):
    if value == 0:
        return None
    request = decode_request(value, slot)
    assert request is not None and request["outcome"] is not None
    return {"id": request["id"], "sequence": request["command"]["sequence"],
            "requested": request["command"]["at"], "effective": request["effective"],
            "outcome": request["outcome"]}


def decode_event(value):
    kind = bits(value, 254, 2)
    content = {"kind": ["transition", "wrap", "pulse"][kind]}
    if kind == 0:
        content["value"] = decode_operation(bits(value, 192, 2), bits(value, 128, 64), bits(value, 194, 1))
    return {"frame": bits(value, 0, 64), "target": decode_identity(value >> 64), "kind": content}


def response(error, value):
    return {"kind": "rejected", "value": ERRORS[error]} if error else {"kind": "accepted", "value": value}
