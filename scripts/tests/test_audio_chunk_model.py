"""Formula checks and an independent immutable-index ownership simulation."""

from collections import deque
from dataclasses import dataclass, replace
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from model_audio_chunks import Assumptions, estimate, height, prefix_nodes


@dataclass
class Node:
    level: int
    entries: tuple
    references: int = 1


class ReferenceIndex:
    """Small ownership oracle; Python allocation is not a runtime design."""

    def __init__(self, fanout, slots):
        self.fanout = fanout
        self.height = height(slots, fanout)
        self.nodes = {}
        self.chunk_refs = {}
        self.next_node = 0
        self.retired = deque()

    def retain(self, handle, level):
        if handle is not None:
            if level == 0:
                self.chunk_refs[handle] += 1
            else:
                self.nodes[handle].references += 1

    def release(self, handle, level):
        if handle is None:
            return
        if level == 0:
            self.chunk_refs[handle] -= 1
            if self.chunk_refs[handle] == 0:
                del self.chunk_refs[handle]
        else:
            node = self.nodes[handle]
            node.references -= 1
            if node.references == 0:
                self.retired.append((handle, 0))

    def create(self, level, owned_entries):
        result = self.next_node
        self.next_node += 1
        self.nodes[result] = Node(level, tuple(owned_entries))
        return result

    def append(self, root, key, chunk):
        assert chunk not in self.chunk_refs
        self.chunk_refs[chunk] = 1

        def visit(old, level):
            entries = list(self.nodes[old].entries) if old is not None else [None] * self.fanout
            selected = key // self.fanout**(level - 1) % self.fanout
            for index, entry in enumerate(entries):
                if index != selected:
                    self.retain(entry, level - 1)
            entries[selected] = chunk if level == 1 else visit(entries[selected], level - 1)
            return self.create(level, entries)

        result = visit(root, self.height)
        self.release(root, self.height)
        return result

    def clip(self, root, ranges):
        def visit(handle, level, low):
            if handle is None:
                return None
            high = low + self.fanout**level
            if all(end <= low or start >= high for start, end in ranges):
                return None
            if any(start <= low and high <= end for start, end in ranges):
                self.retain(handle, level)
                return handle
            if level == 0:
                raise AssertionError("integer chunk ranges cannot partially overlap a chunk")
            node = self.nodes[handle]
            stride = self.fanout**(level - 1)
            entries = []
            for index, child in enumerate(node.entries):
                start = low + index * stride
                if level == 1:
                    keep = any(a <= start < b for a, b in ranges)
                    if keep:
                        self.retain(child, 0)
                    entries.append(child if keep else None)
                else:
                    entries.append(visit(child, level - 1, start))
            return self.create(level, entries)

        return visit(root, self.height, 0)

    def read(self, root, key):
        handle = root
        for level in range(self.height, 0, -1):
            if handle is None:
                return None
            node = self.nodes[handle]
            assert node.references > 0
            handle = node.entries[key // self.fanout**(level - 1) % self.fanout]
        return handle

    def maintain(self, budget):
        work = 0
        while self.retired and work < budget:
            handle, cursor = self.retired[0]
            node = self.nodes[handle]
            if cursor == self.fanout:
                self.retired.popleft()
                del self.nodes[handle]
            else:
                self.release(node.entries[cursor], node.level - 1)
                self.retired[0] = (handle, cursor + 1)
            work += 1
        return work


class CostTests(unittest.TestCase):
    def test_default_hand_calculation(self):
        result = estimate(Assumptions(), 4)
        self.assertEqual(result["height"], 8)
        self.assertEqual(result["node_bytes"], 64)
        self.assertEqual(result["payload_bytes"], 256)
        self.assertEqual(result["legacy_aligned_capture_bytes_per_quantum"], 17408)
        self.assertEqual(result["reference_updates_per_index_update"], 57)
        self.assertEqual(result["candidate_capture_bytes_per_quantum"], 2664)
        self.assertEqual(result["reclaim_units_per_callback"], 320)

    def test_passive_sharing_changes_reservation_not_capture_work(self):
        base = Assumptions()
        shared = replace(base, retained_views=128)
        plain, retained = estimate(base, 4), estimate(shared, 4)
        self.assertEqual(plain["candidate_capture_bytes_per_quantum"], retained["candidate_capture_bytes_per_quantum"])
        self.assertGreater(retained["sample_reservation_bytes"], plain["sample_reservation_bytes"])
        self.assertGreater(retained["index_reservation_bytes"], plain["index_reservation_bytes"])

    def test_active_writers_cost_metadata_not_duplicate_capture_payload(self):
        base = estimate(Assumptions(retained_views=8), 4)
        writing = estimate(Assumptions(retained_views=8, writers=8), 4)
        self.assertEqual(writing["candidate_capture_bytes_per_quantum"] - base["candidate_capture_bytes_per_quantum"],
                         8 * base["index_logical_bytes_per_update"])

    def test_transient_promotion_adds_one_payload_copy(self):
        base = estimate(Assumptions(), 4)
        promoted = estimate(Assumptions(transient_capture=True), 4)
        self.assertEqual(promoted["candidate_capture_bytes_per_quantum"] - base["candidate_capture_bytes_per_quantum"], 512)

    def test_source_scaling_and_quantum_partition(self):
        base = estimate(Assumptions(), 4)
        many = estimate(Assumptions(sources=16), 4)
        partition = estimate(Assumptions(callback_frames=512), 4)
        self.assertEqual(many["candidate_capture_bytes_per_quantum"], 16 * base["candidate_capture_bytes_per_quantum"])
        self.assertEqual(partition["candidate_MB_per_second"], base["candidate_MB_per_second"])
        self.assertEqual(partition["reclaim_units_per_callback"], 2 * base["reclaim_units_per_callback"])

    def test_counter_locality_can_change_preferred_fanout(self):
        compact = Assumptions()
        pessimistic = replace(compact, reference_access_bytes=128)
        self.assertLess(estimate(compact, 8)["candidate_capture_bytes_per_quantum"],
                        estimate(compact, 4)["candidate_capture_bytes_per_quantum"])
        self.assertLess(estimate(pessimistic, 4)["candidate_capture_bytes_per_quantum"],
                        estimate(pessimistic, 8)["candidate_capture_bytes_per_quantum"])

    def test_page_crossing_is_not_hidden_by_aligned_comparison(self):
        row = estimate(Assumptions(), 4)
        self.assertEqual(row["legacy_worst_alignment_capture_bytes_per_quantum"], 33792)

    def test_height_boundaries(self):
        for fanout in (2, 4, 8, 16):
            for level in range(1, 7):
                self.assertEqual(height(fanout**level, fanout), level)
                self.assertEqual(height(fanout**level + 1, fanout), level + 1)

    def test_invalid_admission_assumptions(self):
        for assumptions in (Assumptions(quantum=0), Assumptions(writers=1), Assumptions(callback_frames=255)):
            with self.assertRaises(ValueError):
                estimate(assumptions, 4)


class IndexTests(unittest.TestCase):
    def test_streaming_retirement_bound_with_bulk_deletion_stalled(self):
        for fanout in (2, 4, 8):
            index = ReferenceIndex(fanout, 19)
            root, views = None, []
            for tick in range(60):
                root = index.append(root, tick % 19, tick)
                self.assertLessEqual(index.maintain(index.height * (fanout + 1)), index.height * (fanout + 1))
                self.assertFalse(index.retired)
                if tick >= 19 and tick % 2 == 0:
                    views.append(index.clip(root, [(0, 19)]))
            for view in views:
                index.release(view, index.height)
            bulk = index.retired
            self.assertTrue(bulk)
            index.retired = deque()
            node_limit = (len(views) + 1) * (prefix_nodes(19, fanout, index.height) + 4 * index.height)
            for tick in range(60, 260):
                root = index.append(root, tick % 19, tick)
                index.maintain(index.height * (fanout + 1))
                self.assertFalse(index.retired)
                self.assertLessEqual(len(index.nodes), node_limit)
                self.assertLessEqual(len(index.chunk_refs), (len(views) + 1) * 19)
            index.retired = bulk
            index.release(root, index.height)
            while index.retired:
                self.assertLessEqual(index.maintain(3), 3)
            self.assertFalse(index.nodes)
            self.assertFalse(index.chunk_refs)

    def test_wrapping_history_with_pinned_ranges_and_budgeted_release(self):
        for fanout in (2, 4, 8):
            index = ReferenceIndex(fanout, 11)
            root, views, expected = None, [], {}
            for tick in range(220):
                key = tick % 11
                before = index.next_node
                root = index.append(root, key, tick)
                self.assertEqual(index.next_node - before, index.height)
                expected[key] = tick
                if tick >= 11 and tick % 3 == 0:
                    first = (key + 8) % 11
                    count = 1 + tick % 10
                    keys = [(first + offset) % 11 for offset in range(count)]
                    ranges = [(first, min(11, first + count))]
                    if first + count > 11:
                        ranges.append((0, first + count - 11))
                    before = index.next_node
                    snapshot = index.clip(root, ranges)
                    self.assertLessEqual(index.next_node - before, 4 * index.height)
                    views.append((snapshot, {slot: expected[slot] for slot in keys}))
                if len(views) > 20:
                    snapshot, _ = views.pop(0)
                    index.release(snapshot, index.height)
                budget = index.height * (fanout + 1) + 13
                self.assertLessEqual(index.maintain(budget), budget)
                for slot, value in expected.items():
                    self.assertEqual(index.read(root, slot), value)
                for snapshot, values in views:
                    for slot, value in values.items():
                        self.assertEqual(index.read(snapshot, slot), value)
                    for slot in set(range(11)) - values.keys():
                        self.assertIsNone(index.read(snapshot, slot))
            index.release(root, index.height)
            for snapshot, _ in views:
                index.release(snapshot, index.height)
            while index.retired:
                self.assertEqual(index.maintain(1), 1)
            self.assertEqual(index.nodes, {})
            self.assertEqual(index.chunk_refs, {})

    def test_range_node_reservation_covers_all_small_circular_ranges(self):
        for fanout in (2, 4, 8):
            slots = 19
            index = ReferenceIndex(fanout, slots)
            root = None
            for key in range(slots):
                root = index.append(root, key, key)
                index.maintain(1000)
            for start in range(slots):
                for length in range(1, slots + 1):
                    ranges = [(start, min(slots, start + length))]
                    if start + length > slots:
                        ranges.append((0, start + length - slots))
                    before = index.next_node
                    view = index.clip(root, ranges)
                    self.assertLessEqual(index.next_node - before, 4 * index.height)
                    visited = set()
                    pending = [view]
                    while pending:
                        node_id = pending.pop()
                        if node_id is None or node_id in visited:
                            continue
                        visited.add(node_id)
                        node = index.nodes[node_id]
                        if node.level > 1:
                            pending.extend(node.entries)
                    bound = prefix_nodes(length, fanout, index.height) + 4 * index.height
                    self.assertLessEqual(len(visited), bound)
                    index.release(view, index.height)
                    index.maintain(10000)
            index.release(root, index.height)
            while index.retired:
                index.maintain(10000)
            self.assertFalse(index.nodes)
            self.assertFalse(index.chunk_refs)


if __name__ == "__main__":
    unittest.main()
