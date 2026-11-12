"""Construct media command fixtures; logical checks live beside the tests."""
import random

MIN, MAX, U32, U64 = -(1 << 31), (1 << 31) - 1, (1 << 32) - 1, (1 << 64) - 1


def command(kind, **fields):
    return {"kind": kind, **fields}


def case(name, frames, slots, steps):
    return {"name": name, "media": {"chunk_frames": frames, "chunk_limit": slots,
                                    "samples": [], "views": [], "free_chunks": []}, "steps": steps}
