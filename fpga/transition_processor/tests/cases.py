"""Directed and seeded inputs; expected musical behavior is supplied by Rust."""
from copy import deepcopy


def lid(slot, generation=3):
    return {"slot": slot, "generation": generation}


def rid(slot, generation=1):
    return {"slot": slot, "generation": generation}


def spec(slot, length, source=None, repeat=False):
    return {"id": lid(slot), "kind": "primitive", "length": length,
            "sync_source": lid(source) if source is not None else None,
            "repeat_sync": repeat, "timeline": None}


def play(position=0, retrigger=True):
    return {"kind": "play", "position": position, "retrigger": retrigger}


def seek(position=0, retrigger=False):
    return {"kind": "seek", "position": position, "retrigger": retrigger}


def command(target, sequence, operation=None, at=0, source=None, skip=0):
    return {"target": lid(target), "sequence": sequence, "at": at,
            "timing": {"kind": "at"} if source is None else {"kind": "after_ticks", "source": lid(source), "skip": skip},
            "operation": operation or play()}


def step(commands=None, **updates):
    return dict({"collect_completed": True, "collect": [], "cancel": [],
                 "commands": commands or [], "previews": [[]]}, **updates)


def case(name, loops, steps, *, q=16, loops_limit=None, requests=32):
    loops_limit = loops_limit or max((s["id"]["slot"] + 1 for s in loops), default=1)
    return {"name": name, "plan": {"description": {"quantum": q, "loops": loops},
                                   "limits": {"loop_slots": loops_limit, "pending_requests": requests,
                                              "events": 2 * loops_limit + requests}}, "steps": steps}


def directed(depths):
    p = min(32, depths[1])
    # Single-slot cases also exercise generation reuse and one-quantum loops.
    for q in (1, 16, 32, 64, 128):
        yield case(f"single-loop-modes-q{q}", [spec(0, 2 * q)], [
            step([command(0, 1)]), step(),
            step([command(0, 2, seek(q, False))]),
            step([command(0, 3, play(99 * q, False))]),
            step([command(0, 4, {"kind": "stop"})]),
            step([command(0, 5, seek())]),
            step([command(0, 6, {"kind": "record"})]), step(),
            step([command(0, 7)]), step(), step(),
            step([command(0, 8, {"kind": "stop"})]),
        ], q=q, requests=p)
    yield case("empty-and-invalid-positions", [spec(0, 0)], [
        step([command(0, 1)]), step([command(0, 2, {"kind": "record"})],
            previews=[[{'kind': 'capture_unavailable', 'target': lid(0), 'reason': 'capacity'}]]),
        step([command(0, 3, seek(1))]), step([command(0, 4, {"kind": "stop"})]),
    ], requests=p)
    yield case("terminal-retention-stale-id-and-generation", [spec(0, 32)], [
        step([command(0, 1)], collect_completed=False),
        step([command(0, 1), command(0, 2)], collect_completed=False),
        step([command(0, 3, {"kind": "stop"})], collect_completed=False, collect=[rid(0)]),
        step(cancel=[rid(0), rid(0, 2)], collect_completed=False),
        step(collect=[rid(0, 2)], collect_completed=False),
    ], requests=1)
    yield case("failed-preview-freezes-controls-until-retry", [spec(0, 32)], [
        step([command(0, 1)], previews=[[], [{"kind": "capture_unavailable", "target": lid(0, 0), "reason": "capacity"}]]),
        step([command(0, 2, {"kind": "stop"})], collect=[rid(0)], cancel=[rid(0)]), step(),
    ], requests=p)
    yield case("off-grid-schedule-and-wide-integers", [spec(0, 2**64 - 32)], [
        step([command(0, 2**64-1, play(2**64 - 64), at=1)]), step(), step(),
    ], q=32, requests=p)
    yield case("self-sync-delays-consumed-once-per-boundary", [spec(0, 16, 0, True)], [
        step([command(0, 1)], previews=[[], [], []]),
        step([command(0, 2, seek(0, True), source=0, skip=2**64-1)], previews=[[], [], []]),
        step(), step(), step(),
    ], requests=p)
    duplicate = {"kind": "capture_unavailable", "target": lid(0), "reason": "capacity"}
    yield case("observation-validation-and-capacity", [spec(0, 16)], [
        step([command(0, 1)], previews=[[duplicate, duplicate], []]),
        step(previews=[[duplicate] * (p+2), []]),
        step(previews=[[{"kind": "refused", "request": rid(0), "reason": "capacity"}], []]),
    ], requests=p)
    yield case("empty-logical-topology", [], [step(), step(), step([command(0, 1)])], requests=p)
    if p < 2:
        return
    yield case("record-and-play-same-boundary-refused-retry", [spec(0, 32)], [
        step([command(0, 1, {"kind": "record"}), command(0, 2)], collect_completed=False,
             previews=[[{"kind": "refused", "request": rid(1), "reason": "capacity"}], []]),
        step([command(0, 3, {"kind": "record"})], previews=[[], [duplicate], []]),
        step([command(0, 4)]), step(), step(),
    ], requests=p)
    yield case("invalid-target-source-and-generation", [spec(0, 32)], [step([
        dict(command(0, 1), target=lid(0, 0)), command(2**32-1, 2),
        command(0, 3, play(1)), command(0, 4, at=2**64-1),
        command(0, 5, source=0),
    ])], requests=p)
    if depths[0] < 2:
        return
    yield case("repeat-wait-until-source-pulse", [spec(0, 64), spec(1, 16, 0, True)],
               [step([command(0, 1), command(1, 2)])] + [step() for _ in range(8)], requests=p)
    yield case("stop-cancels-older-future-request", [spec(0, 32), spec(1, 16, 0)], [
        step([command(0, 1), command(1, 2, source=0, skip=2)]),
        step([command(1, 3, {"kind": "stop"}), command(1, 4, at=160)]),
        step(), step(), step(), step(),
    ], requests=p)
    if p >= 4:
        yield case("same-boundary-direct-vs-delayed-priority", [spec(0, 32), spec(1, 32, 0)], [
            step([command(0, 1), command(1, 4), command(1, 2, {"kind": "record"}, source=0),
                  command(1, 5, seek(0, True), source=0)]), step(), step(),
        ], requests=p)
    if depths[0] < 3 or p < 4:
        return
    yield case("record-play-same-boundary-sync-chain", [spec(0, 64), spec(1, 0, 0, True), spec(2, 16, 1)],
               [step([command(0, 1), command(1, 2, {"kind": "record"}),
                      command(1, 3, source=0, skip=1), command(2, 4, source=1, skip=1)], previews=[[], [], []])]
               + [step() for _ in range(17)], requests=p)
    loops = [spec(0, 16, 1, True), spec(1, 32, 0, True), spec(2, 16, 2, True)]
    steps = [step([command(0, 1), command(1, 2), command(2, 3)])] + [step() for _ in range(12)]
    yield case("legal-sync-cycle-self-sync", loops, deepcopy(steps), requests=p)
    yield case("reversed-declarations", list(reversed(loops)), steps, requests=p)


class Random:
    def __init__(self, seed):
        self.value = seed

    def below(self, n):
        self.value = (self.value * 6364136223846793005 + 1442695040888963407) & (2**64-1)
        return (self.value ^ (self.value >> 29)) % n


def seeded(depths, count):
    # The default profile matches the 100 cases used by the Rust differential test.
    loop_slots, requests = min(12, depths[0]), min(32, depths[1])
    for seed in range(count):
        random = Random(seed + 1)
        q = (1, 16, 32, 64, 128)[seed % 5]
        n = 1 + random.below(loop_slots)
        loops = []
        for i in range(n):
            source = random.below(n) if random.below(4) != 0 else None
            loops.append(spec(i, random.below(5) * q, source, bool(random.below(2))))
        sequence = 0
        steps = []
        for quantum in range(80):
            commands = []
            for _ in range(random.below(6)):
                target = random.below(n)
                sequence += 1
                kind = random.below(7)
                if kind == 0:
                    operation = {"kind": "stop"}
                elif kind <= 2:
                    operation = {"kind": "record"}
                elif kind <= 4:
                    operation = play(random.below(6) * q, bool(random.below(2)))
                else:
                    operation = seek(random.below(6) * q, bool(random.below(2)))
                source, skip = None, 0
                if random.below(2) != 0 and loops[target]["sync_source"] is not None:
                    source, skip = loops[target]["sync_source"]["slot"], random.below(3)
                commands.append(command(target, sequence, operation, at=quantum * q + random.below(3*q), source=source, skip=skip))
            next_step = step(commands)
            if random.below(3) == 0:
                next_step["previews"].insert(0, [{"kind": "capture_unavailable", "target": lid(random.below(n)), "reason": "capacity"}])
            steps.append(next_step)
        yield case(f"seed-{seed}", loops, steps, q=q, loops_limit=loop_slots, requests=requests)


def workloads():
    loops = [spec(i, 128 if i % 8 == 0 else 32*(1+i%4), None if i%8 == 0 else i//8*8, i%8 != 0) for i in range(64)]
    first = [command(i, i+1, {"kind": "record"} if i%8 != 0 and i%3 == 0 else play()) for i in range(64)]
    next_commands = [command(i, i+65, play(), at=128 if i%8 == 0 else 32,
                             source=None if i%8 == 0 else i//8*8) for i in range(64)]
    yield case("8x8-tracks-record-play-and-repeat-sync", loops,
               [step(first), step(next_commands)] + [step() for _ in range(14)], q=32, requests=64)
    loops = [spec(i, 32, (i+1)%64) for i in range(64)]
    yield case("64-member-sync-cycle-all-active", loops, [step([command(i, i+1) for i in range(64)]),
               step([command(i, i+65, seek(0, True), source=(i+1)%64) for i in range(64)])]
               + [step() for _ in range(6)], q=32, requests=64)
