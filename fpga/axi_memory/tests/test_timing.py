"""Native timing tests run without HDL or cocotb."""
import configparser
import json
from pathlib import Path
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE/"sim"))
from axi_memory import FixedTiming, histogram_summary, summary
from dram_timing import DramTiming, DEFAULT_CONFIG, DEFAULT_LIBRARY


class StatisticsTests(unittest.TestCase):
    def test_population_statistics(self):
        result = summary([1, 2, 3, 4, 5])
        self.assertEqual(result["mean"], 3)
        self.assertEqual(result["median"], 3)
        self.assertEqual(result["p99"], 5)
        self.assertAlmostEqual(result["stddev"], 2**0.5)
        self.assertEqual(summary([]), {"count": 0})
        self.assertEqual(histogram_summary({"1": 1, "2": 1, "3": 1, "4": 1, "5": 1}), result)
        self.assertEqual(histogram_summary({"3": 2})["median"], 3)

    def test_fixed_clock(self):
        timing = FixedTiming(7)
        token = timing.submit(0, False)
        for _ in range(7):
            self.assertFalse(timing.ready(token))
            timing.advance()
        self.assertTrue(timing.ready(token))

    def test_profile_geometry(self):
        cfg = configparser.ConfigParser()
        cfg.read(DEFAULT_CONFIG)
        native = cfg.getint("system", "bus_width")//8*cfg.getint("dram_structure", "BL")
        capacity = (cfg.getint("dram_structure", "rows")*cfg.getint("dram_structure", "columns")
                    *cfg.getint("dram_structure", "banks_per_group")*cfg.getint("system", "bus_width")//8)
        self.assertEqual(native, 16)
        self.assertEqual(capacity, 512*1024*1024)


@unittest.skipUnless(DEFAULT_LIBRARY.is_file(), "build the optional DRAMsim3 library to run native tests")
class NativeTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.model = DramTiming(output=self.directory.name, fabric_hz=100_000_000)

    def tearDown(self):
        self.model.close()
        self.directory.cleanup()

    def transact(self, address, write=False):
        token = self.model.submit(address, write)
        self.assertIsNotNone(token)
        for _ in range(10000):
            self.model.advance()
            if self.model.ready(token):
                return self.model.events[-1]["cycles"]
        self.fail("native transaction did not complete")

    def test_fractional_clock_and_idle_refresh(self):
        for _ in range(1000):
            self.model.advance()
        self.assertEqual(self.model.ticks, 1000*10_000_000//1_875_000)
        # More than one refresh interval; idle time must advance the controller.
        self.assertGreater(self.model.ticks, 4160)
        self.assertGreater(self.transact(0x1000000), 0)
        self.model.report(final=True)
        stats = json.loads(Path(self.directory.name, "dramsim3.json").read_text())
        self.assertGreater(stats["0"]["num_ref_cmds"], 0)

    def test_row_hits_conflicts_and_turnaround(self):
        cold = self.transact(0x1000000)
        hit = self.transact(0x1000010)
        conflict = self.transact(0x1020000)
        self.assertLess(hit, cold)
        self.assertGreater(conflict, hit)
        self.transact(0x1000040, True)
        self.transact(0x1000040)
        self.model.drain()
        self.assertTrue(self.model.idle)

    def test_queue_capacity_ids_and_posted_write_drain(self):
        accepted = [self.model.submit(0x1000000+(i % 4)*16, False) for i in range(64)]
        tokens = [token for token in accepted if token is not None]
        self.assertEqual(len(tokens), 32)
        self.assertEqual(len(set(tokens)), len(tokens))
        self.model.drain()
        self.assertTrue(all(self.model.ready(token) for token in tokens))
        self.transact(0x1000800, True)
        self.assertFalse(self.model.idle, "posted write callback was mistaken for physical completion")
        self.model.drain()
        self.assertTrue(self.model.idle)

    def test_different_banks_overlap(self):
        def pair(second):
            self.model.reset()
            start = self.model.ticks
            first = self.model.submit(0x1000000, False)
            other = self.model.submit(second, False)
            self.model.drain()
            self.assertTrue(self.model.ready(first) and self.model.ready(other))
            return self.model.ticks-start
        self.assertLess(pair(0x1000800), pair(0x1004000))

    def test_reset_generation_and_address_checks(self):
        old = self.model.submit(0x1000000, False)
        self.model.reset()
        new = self.model.submit(0x1000000, False)
        self.assertNotEqual(old, new)
        self.model.drain()
        self.assertFalse(self.model.ready(old))
        self.assertTrue(self.model.ready(new))
        for address in (-16, 1, self.model.capacity):
            with self.assertRaises(ValueError):
                self.model.submit(address, False)

    def test_final_statistics_are_not_double_counted(self):
        self.transact(0x1000000, True)
        first = self.model.report(final=True)
        second = self.model.report(final=True)
        self.assertEqual(first, second)
        self.assertEqual(first["dram_write_commands"], 1)
        self.assertEqual(first["dram_write_service_dram_cycles"]["count"], 1)
        with self.assertRaises(RuntimeError):
            self.model.advance()

    def test_bus_counters_include_physical_bursts_not_callbacks(self):
        start = self.model.bus_snapshot()
        tokens = [self.model.submit(0x1000000, False) for _ in range(4)]
        self.model.drain()
        self.assertTrue(all(self.model.ready(token) for token in tokens))
        read = self.model.bus_utilization(start)
        self.assertEqual(read["read_commands"], 1, "merged reads must not count as separate DDR bursts")
        self.assertEqual(read["read_cycles"], 4)
        self.assertEqual(read["transferred_bytes"], 16)
        self.assertGreater(read["command_cycles"], read["read_commands"])
        self.assertGreater(read["data_bus_utilization"], 0)
        self.assertLessEqual(read["data_bus_utilization"], 1)
        self.assertAlmostEqual(read["transfer_bytes_per_second"]/read["peak_bytes_per_second"], read["data_bus_utilization"])
        start = self.model.bus_snapshot()
        self.transact(0x1001000, True)
        self.model.drain()
        write = self.model.bus_utilization(start)
        self.assertEqual(write["write_commands"], 1)
        self.assertEqual(write["write_cycles"], 4)
        self.assertEqual(write["read_cycles"], 0)
        start = self.model.bus_snapshot()
        for _ in range(20):
            self.model.advance()
        idle = self.model.bus_utilization(start)
        self.assertEqual(idle["data_bus_utilization"], 0)
        self.model.reset()
        with self.assertRaises(ValueError):
            self.model.bus_utilization(start)

    def test_bus_count_windows_add_without_double_counting(self):
        start = self.model.bus_snapshot()
        token = self.model.submit(0x1000000, False)
        previous, accumulated = start, 0
        for _ in range(100):
            self.model.advance()
            current = self.model.bus_snapshot()
            accumulated += self.model.bus_utilization(previous, current)["data_busy_cycles"]
            previous = current
            if self.model.ready(token):
                break
        self.assertTrue(self.model.ready(token))
        self.assertEqual(accumulated, 4)
        self.assertEqual(accumulated, self.model.bus_utilization(start)["data_busy_cycles"])


if __name__ == "__main__":
    unittest.main()
