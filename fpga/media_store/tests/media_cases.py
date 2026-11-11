"""Directed ownership cases and seeded command sequences; samples come from Rust."""
import random

MIN, MAX, U32, U64 = -(1 << 31), (1 << 31) - 1, (1 << 32) - 1, (1 << 64) - 1


def command(kind, **fields):
    return {"kind": kind, **fields}


def case(name, frames, slots, steps):
    return {"name": name, "media": {"chunk_frames": frames, "chunk_limit": slots,
                                    "samples": [], "views": [], "free_chunks": []}, "steps": steps}


def directed(capacities):
    _, slots, cf = capacities
    values = list(range(1, 3*cf + 1))
    yield case("unaligned-source-clear-and-reuse", cf, 3, [
        command("create"), command("append", view=0, samples=values),
        command("slice", source=0, start=cf-1, frames=cf+1), command("clear", view=0),
        command("append", view=0, samples=[MIN, MAX, -3, 3] * (cf//4)),
        command("read", view=1, position=cf, length=cf+1, frames=2*cf+3),
        command("clear", view=1), command("append", view=0, samples=list(range(-2*cf, 0))),
        command("read", view=0, position=3*cf-1, length=3*cf, frames=cf+3),
        command("remove", view=0), command("reclaim", view=1), command("remove", view=1),
        command("read", view=0, position=0, length=1, frames=4),
    ])
    yield case("nested-partial-tail-and-empty", cf, slots, [
        command("create"), command("append", view=0, samples=[MAX-i for i in range(cf-1)]),
        command("slice", source=0, start=1, frames=cf-2),
        command("slice", source=1, start=1, frames=cf-3),
        command("slice", source=1, start=cf-2, frames=0),
        command("append", view=0, samples=list(range(2*cf+1))),
        command("read", view=1, position=cf-3, length=cf-2, frames=cf+5),
        command("remove", view=0), command("remove", view=1),
        command("read", view=2, position=0, length=cf-3, frames=cf),
        command("append", view=2, samples=[]), command("clear", view=2),
        command("append", view=2, samples=[MIN]), command("remove", view=2),
        command("slice", source=3, start=0, frames=0),
    ])
    yield case("zero-slot-admission", cf, 0, [
        command("create"), command("append", view=0, samples=[]),
        command("append", view=0, samples=[1]), command("slice", source=0, start=0, frames=0),
        command("append", view=1, samples=[]), command("reclaim", view=0),
        command("read", view=0, position=0, length=0, frames=0), command("remove", view=0),
    ])
    yield case("pinned-full-arena-and-atomic-refusal", cf, 1, [
        command("create"), command("append", view=0, samples=list(range(cf-1))),
        command("slice", source=0, start=1, frames=cf-2),
        command("append", view=0, samples=[MAX]), command("append", view=0, samples=[MIN]),
        command("clear", view=0), command("append", view=0, samples=[MIN]),
        command("read", view=1, position=0, length=cf-2, frames=cf),
        command("remove", view=1), command("append", view=0, samples=[MIN]),
    ])
    yield case("incremental-offset-reclamation", cf, slots, [
        command("create"), command("append", view=0, samples=values),
        command("slice", source=0, start=cf-1, frames=cf+2),
        *[command("reclaim", view=1) for _ in range(4)],
        *[command("reclaim", view=0) for _ in range(4)],
        command("append", view=1, samples=[]), command("append", view=0, samples=values),
    ])
    yield case("invalid-and-wide-inputs", cf, slots, [
        command("create"), command("append", view=0, samples=[MIN, MAX, -1, 1]),
        command("read", view=U32, position=0, length=4, frames=4),
        command("read", view=65536, position=0, length=4, frames=4),
        command("slice", source=0, start=U64, frames=U64),
        command("slice", source=0, start=4, frames=1),
        command("read", view=0, position=U64, length=U64, frames=0),
        command("read", view=0, position=0, length=0, frames=0),
        command("read", view=0, position=0, length=4, frames=0),
        command("remove", view=0), command("clear", view=0), command("reclaim", view=0),
    ])
    unbounded = case("elastic-snapshot-within-hardware-capacity", cf, slots, [
        command("create"), command("append", view=0, samples=list(range(cf+3))),
        command("slice", source=0, start=cf-1, frames=4), command("remove", view=0),
        command("read", view=1, position=3, length=4, frames=7), command("remove", view=1),
        command("create"), command("append", view=2, samples=[MIN, MAX]),
    ])
    del unbounded["media"]["chunk_limit"]
    yield unbounded


def randomized(capacities, seed, count):
    views, slots, cf = capacities
    rng = random.Random(seed)
    for index in range(count):
        steps = [command("create"), command("append", view=0, samples=[rng.randint(MIN, MAX) for _ in range(2*cf)]),
                 command("slice", source=0, start=cf-1, frames=cf+1)]
        possible_views = 2
        for _ in range(30):
            kind = rng.choice(["create", "slice", "clear", "remove", "reclaim", "append", "read"])
            view = rng.randrange(min(possible_views + 1, views))
            if kind in ("create", "slice") and possible_views == views:
                kind = "read"
            if kind == "create":
                step = command(kind)
                possible_views += 1
            elif kind == "slice":
                step = command(kind, source=view, start=rng.randrange(2*cf+1), frames=rng.randrange(2*cf+1))
                possible_views += 1
            elif kind == "append":
                step = command(kind, view=view, samples=[rng.choice([MIN, MAX, -3, 0, 3, rng.randint(MIN, MAX)])
                                                         for _ in range(rng.randrange(3*cf+1))])
            elif kind == "read":
                step = command(kind, view=view, position=rng.randrange(2*cf+1), length=rng.randrange(2*cf+1),
                               frames=rng.choice([0, 1, cf, cf+3]))
            else:
                step = command(kind, view=view)
            steps.append(step)
        yield case(f"seed-{seed}-{index}", cf, slots, steps)
