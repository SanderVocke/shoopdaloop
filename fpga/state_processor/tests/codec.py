"""Convert architectural JSON values to the version-1 packed RTL ABI."""

INSTRUCTIONS = {
    "process_input": (0, ("port", "destination")),
    "process_loop_playback": (1, ("view", "position", "length", "enabled", "destination")),
    "process_gain": (2, ("ramp", "source", "destination")),
    "process_mix": (3, ("left", "right", "destination")),
    "process_loop_capture": (4, ("view", "enabled", "source")),
    "process_output": (5, ("source", "port")),
}
MASK32 = (1 << 32) - 1
MASK64 = (1 << 64) - 1


def unsigned(value, bits):
    if type(value) is not int or not 0 <= value < (1 << bits):
        raise ValueError(f"{value!r} does not fit an unsigned {bits}-bit RTL field")
    return value


def encode_instruction(instruction):
    if instruction["kind"] in ("process_logical_playback", "process_logical_capture"):
        loop = instruction["loop_id"]
        generation = unsigned(loop["generation"], 32)
        fields = [unsigned(loop["slot"], 16), generation & 65535, generation >> 16,
                  unsigned(instruction["view"], 16), unsigned(instruction.get("destination", instruction.get("source")), 16)]
        return (6 if instruction["kind"] == "process_logical_playback" else 7) | sum(v << (8+16*i) for i,v in enumerate(fields))
    opcode, fields = INSTRUCTIONS[instruction["kind"]]
    return opcode | sum(unsigned(instruction[field], 16) << (8 + 16 * index)
                        for index, field in enumerate(fields))


def encode_value(value):
    if value["kind"] == "word":
        return unsigned(value["value"], 64)
    if value["kind"] != "ramp":
        raise ValueError(f"unknown state value: {value!r}")
    ramp = value["value"]
    for field in ("start", "target"):
        if type(ramp[field]) is not int or not -(1 << 31) <= ramp[field] < (1 << 31):
            raise ValueError(f"invalid signed ramp endpoint: {ramp[field]!r}")
    return ((1 << 128) | (ramp["start"] & MASK32) | ((ramp["target"] & MASK32) << 32)
            | (unsigned(ramp["duration"], 32) << 64)
            | (unsigned(ramp["elapsed"], 32) << 96))


def signed32(value):
    return value - (1 << 32) if value & (1 << 31) else value


def decode_ramp(value):
    return {"start": signed32(value & MASK32),
            "target": signed32((value >> 32) & MASK32),
            "duration": (value >> 64) & MASK32,
            "elapsed": (value >> 96) & MASK32}


def decode_value(value):
    unsigned(value, 129)
    if value >> 128:
        return {"kind": "ramp", "value": decode_ramp(value)}
    return {"kind": "word", "value": value & MASK64}


def decode_job(value):
    unsigned(value, 256)
    opcode = value & 255
    id0, id1 = (value >> 8) & 65535, (value >> 24) & 65535
    p0, p1, p2 = ((value >> offset) & MASK64 for offset in (64, 128, 192))
    assert (value >> 40) & ((1 << 24) - 1) == 0, "nonzero reserved job bits"
    if opcode == 0:
        assert (p0, p1, p2) == (0, 0, 0)
        return {"kind": "input", "port": id0, "destination": id1}
    if opcode == 1:
        assert (id1, p0, p1, p2) == (0, 0, 0, 0)
        return {"kind": "silence", "destination": id0}
    if opcode == 2:
        assert id1 == 0
        return {"kind": "read_view", "destination": id0, "view": p0,
                "position": p1, "length": p2}
    if opcode == 3:
        assert p2 == 0
        return {"kind": "gain", "source": id0, "destination": id1,
                "ramp": decode_ramp(p0 | (p1 << 64))}
    if opcode == 4:
        assert p0 <= 65535 and (p1, p2) == (0, 0)
        return {"kind": "mix", "left": id0, "right": id1, "destination": p0}
    if opcode == 5:
        assert (id1, p1, p2) == (0, 0, 0)
        return {"kind": "capture", "source": id0, "view": p0}
    if opcode == 6:
        assert (p0, p1, p2) == (0, 0, 0)
        return {"kind": "output", "source": id0, "port": id1}
    raise AssertionError(f"unknown RTL job opcode: {opcode:#x}")


def decode_status(code):
    if code == 0:
        return {"kind": "completed"}
    if code not in (1, 2):
        raise AssertionError(f"hardware-only or unknown error in a Rust case: {code}")
    return {"kind": "failed", "value": {"error": {1: "queue_full", 2: "invalid_state"}[code]}}
