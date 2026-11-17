#!/usr/bin/env python3
"""Estimate audio-chunk software traffic, reservations and reclamation work.

Counts are logical reads/writes, not DRAM transactions or timing predictions.
The legacy comparator excludes its index work; the candidate includes an explicit
conservative model of path copying and eventual reclamation for each update.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import json
import math


def ceil_div(n: int, d: int) -> int:
    return (n + d - 1) // d


def height(slots: int, fanout: int) -> int:
    levels, capacity = 1, fanout
    while capacity < slots:
        levels += 1
        capacity *= fanout
    return levels


def prefix_nodes(slots: int, fanout: int, levels: int) -> int:
    return sum(ceil_div(slots, fanout**level) for level in range(1, levels + 1))


@dataclass(frozen=True)
class Assumptions:
    sample_rate: int = 48000
    quantum: int = 32
    channels: int = 2
    sources: int = 1
    callback_frames: int = 256
    history_seconds: float = 30
    page_bytes: int = 4096
    index_entry_bytes: int = 4
    node_header_bytes: int = 16
    node_alignment: int = 64
    reference_access_bytes: int = 8
    node_bookkeeping_bytes: int = 16
    chunk_bookkeeping_bytes: int = 32
    chunk_metadata_bytes: int = 16
    descriptor_bytes: int = 32
    retained_views: int = 0
    view_seconds: float = 30
    writers: int = 0
    copy_edges: int = 0
    materializations: int = 0
    dsp_inputs: int = 1
    arena_scratch_chunks: int = 0
    transient_capture: bool = False
    extra_reclaim_nodes: int = 0


def estimate(a: Assumptions, fanout: int) -> dict:
    if fanout < 2:
        raise ValueError("fanout must be at least two")
    positive = (a.sample_rate, a.quantum, a.channels, a.sources, a.callback_frames,
                a.page_bytes, a.index_entry_bytes, a.node_alignment, a.reference_access_bytes)
    nonnegative = (a.node_header_bytes, a.node_bookkeeping_bytes,
                   a.chunk_bookkeeping_bytes, a.chunk_metadata_bytes,
                   a.descriptor_bytes, a.retained_views, a.view_seconds,
                   a.writers, a.copy_edges, a.materializations, a.dsp_inputs,
                   a.arena_scratch_chunks, a.extra_reclaim_nodes)
    if min(positive) <= 0 or min(nonnegative) < 0 or a.history_seconds <= 0:
        raise ValueError("sizes/rates must be positive and counts nonnegative")
    if a.writers > a.retained_views:
        raise ValueError("writers must be included in retained_views")
    if a.callback_frames % a.quantum:
        raise ValueError("this model requires callbacks aligned to the quantum")
    payload = a.quantum * a.channels * 4
    page_frames = a.page_bytes // (a.channels * 4)
    if page_frames == 0:
        raise ValueError("a frame must fit in a legacy page")
    ticks_per_second = a.sample_rate / a.quantum
    ticks_per_callback = a.callback_frames // a.quantum
    history_slots = math.ceil(a.history_seconds * a.sample_rate / a.quantum) + 1
    view_slots = math.ceil(a.view_seconds * a.sample_rate / a.quantum) + 2
    levels = height(max(history_slots, view_slots if a.retained_views else 0), fanout)
    node_bytes = ceil_div(a.node_header_bytes + fanout * a.index_entry_bytes,
                          a.node_alignment) * a.node_alignment
    updates = a.sources + a.writers

    # A span can straddle pages. This is the maximum over every frame-aligned
    # starting position; the aligned case is also reported independently.
    worst_pages = ceil_div(a.quantum + page_frames - 1, page_frames)
    aligned_pages = ceil_div(a.quantum, page_frames)
    legacy_aligned = a.sources * (4 * aligned_pages * a.page_bytes + 4 * payload)
    legacy_worst = a.sources * (4 * worst_pages * a.page_bytes + 4 * payload)

    # Each copied node retains at most B-1 unchanged handles. Eventually dropping
    # its predecessor visits at most B handles. Include one root decrement.
    rc_updates = levels * (2 * fanout - 1) + 1
    index_traffic_per_update = (3 * levels * node_bytes
                                + rc_updates * a.reference_access_bytes
                                + levels * a.node_bookkeeping_bytes)
    payload_traffic = 2 * payload * a.sources * (2 if a.transient_capture else 1)
    candidate_capture = (payload_traffic + updates * index_traffic_per_update
                         + a.sources * a.chunk_bookkeeping_bytes)

    # Only copy-only eligible edges are counted. Fan-in and DSP are explicit
    # materializations, not magically eliminated by forwarding descriptors.
    legacy_edges = a.copy_edges * 2 * payload
    candidate_edges = a.copy_edges * 2 * a.descriptor_bytes
    common_dsp = a.materializations * payload * (a.dsp_inputs + 1)
    extra_reclaim_bytes = a.extra_reclaim_nodes * (
        node_bytes + fanout * a.reference_access_bytes + a.node_bookkeeping_bytes)
    candidate_total = candidate_capture + candidate_edges + common_dsp + extra_reclaim_bytes
    legacy_total = legacy_aligned + legacy_edges + common_dsp

    # One edge release or one node finalization is a work unit. Steady-state
    # maintenance includes path retirement for every admitted index writer.
    steady_reclaim_units = updates * levels * (fanout + 1)
    total_reclaim_units = steady_reclaim_units + a.extra_reclaim_nodes * (fanout + 1)
    history_nodes = prefix_nodes(history_slots, fanout, levels)
    # An arbitrary interval has two boundary paths; a circular range has at most
    # four. Add a boundary allowance to the compact full-range node count.
    view_nodes = prefix_nodes(view_slots, fanout, levels) + 4 * levels
    owned_chunks = a.sources * history_slots + a.retained_views * view_slots
    owned_nodes = a.sources * history_nodes + a.retained_views * view_nodes
    # One callback of allocation headroom only. A real admission policy must add
    # the maximum permitted backlog and simultaneous command staging separately.
    staging_nodes = updates * levels * ticks_per_callback
    staging_chunks = a.sources * ticks_per_callback
    arena_chunks = a.materializations + a.arena_scratch_chunks
    if a.transient_capture:
        arena_chunks += a.sources

    return {
        "fanout": fanout, "height": levels, "node_bytes": node_bytes,
        "payload_bytes": payload, "ticks_per_second": ticks_per_second,
        "history_slots_per_source": history_slots,
        "index_updates_per_quantum": updates,
        "nodes_created_per_quantum": updates * levels,
        "reference_updates_per_index_update": rc_updates,
        "index_logical_bytes_per_update": index_traffic_per_update,
        "legacy_aligned_capture_bytes_per_quantum": legacy_aligned,
        "legacy_worst_alignment_capture_bytes_per_quantum": legacy_worst,
        "candidate_capture_bytes_per_quantum": candidate_capture,
        "candidate_total_bytes_per_quantum": candidate_total,
        "legacy_comparator_bytes_per_quantum": legacy_total,
        "candidate_MB_per_second": candidate_total * ticks_per_second / 1e6,
        "legacy_comparator_MB_per_second": legacy_total * ticks_per_second / 1e6,
        "logical_traffic_ratio": legacy_total / candidate_total,
        "steady_reclaim_units_per_quantum": steady_reclaim_units,
        "reclaim_units_per_callback": total_reclaim_units * ticks_per_callback,
        "deletion_nodes_drained_per_quantum": a.extra_reclaim_nodes,
        "sample_reservation_bytes": (owned_chunks + staging_chunks) * payload,
        "chunk_metadata_reservation_bytes": (owned_chunks + staging_chunks) * a.chunk_metadata_bytes,
        "index_reservation_bytes": (owned_nodes + staging_nodes) * node_bytes,
        "arena_sample_bytes": arena_chunks * payload,
        "reservation_scope": "owner capacity plus one callback staging; excludes command staging, backlog headroom, root tables, processor state and MIDI",
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    defaults = Assumptions()
    help_text = {
        "sources": "distinct captured signal groups, each with an input history",
        "writers": "additional index writers sharing those captured payloads; counted within retained-views",
        "retained_views": "maximum retained owners, conservatively reserved at view-seconds each",
        "reference_access_bytes": "assumed traffic per reference update: 8 logical bytes by default; 128 explores cold 64-byte lines",
        "copy_edges": "copy-only graph edges eligible for reference forwarding; excludes mixing",
        "materializations": "additional uncaptured DSP outputs per quantum; their sample work remains in both comparators",
        "dsp_inputs": "input payloads read per additional materialization",
        "extra_reclaim_nodes": "bulk deletion node equivalents serviced per quantum, beyond streaming retirement",
    }
    for name, default in asdict(defaults).items():
        option = "--" + name.replace("_", "-")
        if isinstance(default, bool):
            parser.add_argument(option, action="store_true", help=help_text.get(name))
        else:
            parser.add_argument(option, type=type(default), default=default, help=help_text.get(name))
    parser.add_argument("--fanouts", type=int, nargs="+", default=[2, 4, 8, 16, 32])
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    a = Assumptions(**{name: getattr(args, name) for name in asdict(defaults)})
    try:
        rows = [estimate(a, fanout) for fanout in args.fanouts]
    except ValueError as error:
        parser.error(str(error))
    if args.json:
        print(json.dumps({"assumptions": asdict(a), "estimates": rows}, indent=2))
        return
    print("Logical software traffic; not measured DRAM traffic or callback time.")
    print("Legacy column excludes index work; candidate includes path retirement.")
    print(" B  height node_B  old_MB/s  new_MB/s  ratio  reclaim_units/callback  index_MiB")
    for row in rows:
        print(f"{row['fanout']:2} {row['height']:7} {row['node_bytes']:6} "
              f"{row['legacy_comparator_MB_per_second']:9.3f} "
              f"{row['candidate_MB_per_second']:9.3f} "
              f"{row['logical_traffic_ratio']:6.2f} "
              f"{row['reclaim_units_per_callback']:23} "
              f"{row['index_reservation_bytes'] / 2**20:10.3f}")
    print("Reservations exclude command staging and reclamation backlog headroom.")


if __name__ == "__main__":
    main()
