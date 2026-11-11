"""Sample stimulus and programs; the Rust processors produce the expected samples."""
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


def directed(compilations, capacities):
    for frames, compiled in compilations.items():
        yield graph_case(compiled, capacities, frames, ramps=[ramp(0, ONE, 97), ramp(ONE, -ONE, 71)],
                         name=f"milestone-two-input-q{frames}", steps=8)
        settings = [ramp(0, target) for target in (0, 1, -1, ONE, -ONE, ONE // 2, MIN, MAX)]
        settings += [ramp(MIN, MAX, U32, U32 - 16), ramp(MAX, MIN, U32, U32 - 16),
                     ramp(MIN, MAX, U32, 1234567890), ramp(MAX, MIN, 7, 0),
                     ramp(-ONE, ONE, 1), ramp(ONE, -ONE, 17, 15),
                     ramp(MIN, MAX, 0, U32), ramp(MIN, MAX, 3, U32)]
        for index, setting in enumerate(settings):
            yield graph_case(compiled, capacities, frames, ramps=[setting, settings[-index - 1]],
                             name=f"arithmetic-q{frames}-{index}", steps=3)
        yield make_case(f"silence-q{frames}", [ins("loop_playback", view=65535, position=65535, length=65535,
                              enabled=0, destination=0), ins("output", source=0, port=0)],
                        [word(0)], capacities, frames=frames, buffers=2, inputs=[])
        yield make_case(f"fanout-q{frames}", [ins("input", port=0, destination=0),
                        ins("gain", ramp=0, source=0, destination=1), ins("gain", ramp=1, source=0, destination=2),
                        ins("mix", left=1, right=2, destination=3), ins("output", source=3, port=0),
                        ins("output", source=0, port=1)], [ramp(0, ONE // 2), ramp(0, -ONE // 2)],
                        capacities, frames=frames, buffers=4, steps=5)
        yield make_case(f"same-source-mix-q{frames}", [ins("input", port=0, destination=0),
                        ins("mix", left=0, right=0, destination=1), ins("output", source=1, port=0)],
                        [], capacities, frames=frames, buffers=2)
        yield make_case(f"output-overwrite-q{frames}", [ins("input", port=0, destination=0),
                        ins("input", port=1, destination=1), ins("output", source=0, port=0),
                        ins("output", source=1, port=0)], [], capacities, frames=frames, buffers=2)
        yield make_case(f"empty-program-q{frames}", [], [], capacities, frames=frames, buffers=0)
    frames = min(3, capacities[3])
    bad_jobs = [ins("gain", ramp=0, source=0, destination=0), ins("gain", ramp=0, source=65535, destination=1),
                ins("input", port=0, destination=0), ins("input", port=65535, destination=1),
                ins("mix", left=0, right=1, destination=2), ins("mix", left=0, right=0, destination=65535),
                ins("output", source=65535, port=0), ins("output", source=0, port=2),
                ins("loop_playback", view=65535, position=65535, length=65535, enabled=1, destination=0)]
    for index, bad in enumerate(bad_jobs):
        yield make_case(f"buffer-error-{index}", [ins("input", port=0, destination=0), bad,
                        ins("output", source=0, port=0)], [ramp(-ONE, ONE, 71), word(0)], capacities,
                        frames=frames, buffers=4, outputs=1)
    yield make_case("state-error-prefix", [ins("input", port=0, destination=0),
                    ins("gain", ramp=65535, source=0, destination=1)], [], capacities,
                    frames=frames, buffers=2)
    refusal = make_case("state-admission-error", [ins("input", port=0, destination=0)], [], capacities,
                        frames=frames, buffers=1)
    refusal["program"]["jobs_per_quantum"] = capacities[2] + 1
    yield refusal


def randomized(compilations, capacities, seed, count):
    rng = random.Random(seed)
    for index in range(count):
        frames = rng.choice(list(compilations))
        ramps = []
        for _ in range(2):
            duration = rng.choice([0, 1, 2, frames, frames + 1, 71, U32, rng.getrandbits(32)])
            elapsed = rng.choice([0, max(0, duration - 1), U32, rng.getrandbits(32)])
            ramps.append(ramp(rng.randint(MIN, MAX), rng.randint(MIN, MAX), duration, elapsed))
        value = graph_case(compilations[frames], capacities, frames, ramps=ramps,
                           name=f"random-{seed}-{index}", steps=3)
        for step in value["steps"]:
            step["inputs"] = [[rng.randint(MIN, MAX) for _ in range(frames)] for _ in range(2)]
            if rng.randrange(3) == 0:
                address = rng.randrange(2)
                step["writes"] = [{"address": address, "value": ramp(rng.randint(MIN, MAX),
                                    rng.randint(MIN, MAX), rng.getrandbits(32), rng.getrandbits(32))}]
        yield value
