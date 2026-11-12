"""Construct explicit instruction/state protocol fixtures."""

from copy import deepcopy
import random

U32 = (1 << 32) - 1
U64 = (1 << 64) - 1


def word(value):
    return {"kind": "word", "value": value}


def ramp(duration=100, elapsed=0):
    return {"kind": "ramp", "value": {"start": -(1 << 31), "target": (1 << 31) - 1,
                                      "duration": duration, "elapsed": elapsed}}


def ins(kind, **fields):
    return {"kind": "process_" + kind, **fields}


def case(name, instructions, memory, capacity, *, quantum=32, budget=None, writes=None):
    return {"name": name,
            "program": {"chunk_frames": 1, "instructions": instructions, "quantum": quantum,
                        "jobs_per_quantum": len(instructions) if budget is None else budget},
            "initial_state": {"memory": memory}, "queue_capacity": capacity,
            "steps": [{"writes": items} for items in (writes if writes is not None else [[]])]}
