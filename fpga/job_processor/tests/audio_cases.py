"""Raw arithmetic stimulus and handwritten program fixtures."""
from copy import deepcopy
import random

from cases import ins, word

MIN, MAX, ONE, U32 = -(1 << 31), (1 << 31) - 1, 1 << 24, (1 << 32) - 1


def ramp(start, target, duration=0, elapsed=0):
    return {"kind": "ramp", "value": dict(start=start, target=target, duration=duration, elapsed=elapsed)}


def samples(frames, phase=0):
    edges = [-1, -3, 3, MIN, MAX, ONE, -ONE, 0, 1, MAX - 1, MIN + 1, 123456789]
    return [edges[(index + phase) % len(edges)] for index in range(frames)]


def make_case(name, instructions, memory, capacities, *, frames=3, buffers=6, outputs=2, steps=3, inputs=None):
    return {"name": name, "program": {"chunk_frames": 1, "instructions": instructions, "quantum": frames,
                                      "jobs_per_quantum": len(instructions)},
            "initial_state": {"memory": memory}, "queue_capacity": capacities[2],
            "buffers": buffers, "output_ports": outputs,
            "steps": [{"writes": [], "inputs": [samples(frames, step), samples(frames, step + 3)]
                       if inputs is None else inputs} for step in range(steps)]}


def graph_case(compiled, capacities, frames, *, ramps=None, name="two-input-ramp-mix", steps=8):
    value = make_case(name, compiled["program"]["instructions"], deepcopy(compiled["initial_state"]),
                      capacities, frames=frames, steps=steps)
    if ramps is not None:
        addresses = [binding["value"]["ramp"] for binding in compiled["bindings"] if binding["kind"] == "gain"]
        for address, setting in zip(addresses, ramps):
            value["initial_state"]["memory"][address] = setting
    return value
