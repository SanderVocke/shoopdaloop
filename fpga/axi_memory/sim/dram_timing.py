"""ctypes timing policy. Samples and AXI scheduling stay outside the native model."""
import configparser
from collections import Counter
import ctypes as C
import json
from pathlib import Path
from axi_memory import histogram_summary, summary

HERE = Path(__file__).resolve().parents[1]
DEFAULT_CONFIG = HERE/"configs/zynq7010-ddr3-1066-x16.ini"
DEFAULT_LIBRARY = HERE.parents[1]/"work/dramsim3/build/libshoop_dram.so"


class Completion(C.Structure):
    _fields_ = [(name, C.c_uint64) for name in ("id", "address", "submitted", "completed")]+[("write", C.c_uint32)]


class DramTiming:
    def __init__(self, library=DEFAULT_LIBRARY, config=DEFAULT_CONFIG, output=None, fabric_hz=100_000_000):
        self.library, self.config = Path(library).resolve(), Path(config).resolve()
        self.output = Path(output or HERE.parents[1]/"work/dramsim3/stats").resolve()
        self.output.mkdir(parents=True, exist_ok=True)
        if not self.library.is_file():
            raise RuntimeError(f"DRAMsim3 bridge missing: {self.library}; run fpga/axi_memory/build_dramsim3.py")
        if not self.config.is_file():
            raise ValueError(f"DDR3 configuration missing: {self.config}")
        cfg = configparser.ConfigParser()
        cfg.read(self.config)
        if cfg.get("dram_structure", "protocol") != "DDR3" or not cfg.getboolean("system", "unified_queue"):
            raise ValueError("use a DDR3 unified-queue profile for bounded small-write drain")
        if cfg.getint("timing", "tREFI") <= 0:
            raise ValueError("DDR3 refresh interval must be positive")
        self.capacity = cfg.getint("system", "channel_size")*1024*1024*cfg.getint("system", "channels")
        self.fabric_hz = int(fabric_hz)
        if not 1_000_000 <= self.fabric_hz <= 1_000_000_000:
            raise ValueError("fabric frequency must be 1 MHz to 1 GHz")
        self.lib = C.CDLL(str(self.library))
        signatures = {
            "create": (C.c_void_p, [C.c_char_p, C.c_char_p]), "destroy": (None, [C.c_void_p]),
            "tck_fs": (C.c_uint64, [C.c_void_p]), "native_bytes": (C.c_uint32, [C.c_void_p]),
            "ticks": (C.c_uint64, [C.c_void_p]), "submit": (C.c_uint64, [C.c_void_p, C.c_uint64, C.c_uint32]),
            "advance": (None, [C.c_void_p, C.c_uint64]), "poll": (C.c_uint32, [C.c_void_p, C.POINTER(Completion)]),
            "idle": (C.c_uint32, [C.c_void_p]), "drain": (C.c_uint32, [C.c_void_p, C.c_uint64]),
            "print_stats": (None, [C.c_void_p]), "error": (C.c_char_p, [])}
        for name, (restype, argtypes) in signatures.items():
            function = getattr(self.lib, "shoop_dram_"+name)
            function.restype, function.argtypes = restype, argtypes
        self.handle = None
        self.events = []
        self.done = set()
        self.generation = 0
        self._create()

    def _create(self):
        self.final_statistics = None
        self.handle = self.lib.shoop_dram_create(str(self.config).encode(), (str(self.output)+"/").encode())
        if not self.handle:
            raise RuntimeError(self.lib.shoop_dram_error().decode())
        self.native_bytes = self.lib.shoop_dram_native_bytes(self.handle)
        self.tck_fs = self.lib.shoop_dram_tck_fs(self.handle)

    def poll(self):
        event = Completion()
        while self.lib.shoop_dram_poll(self.handle, C.byref(event)):
            token = (self.generation, event.id)
            self.done.add(token)
            self.events.append(dict(id=event.id, generation=self.generation, address=event.address,
                                    write=bool(event.write), submitted=event.submitted, completed=event.completed,
                                    cycles=event.completed-event.submitted))

    def advance(self):
        if self.final_statistics is not None:
            raise RuntimeError("cannot advance a finalized DRAM report; reset first")
        self.lib.shoop_dram_advance(self.handle, self.fabric_hz)
        self.poll()

    def submit(self, address, write):
        if self.final_statistics is not None:
            raise RuntimeError("cannot submit after a finalized DRAM report; reset first")
        if address % self.native_bytes or not 0 <= address < self.capacity:
            raise ValueError("native DDR address out of range or unaligned")
        token = self.lib.shoop_dram_submit(self.handle, address, int(write))
        return (self.generation, token) if token else None

    def ready(self, token):
        return token in self.done

    @property
    def ticks(self):
        return self.lib.shoop_dram_ticks(self.handle)

    @property
    def idle(self):
        return bool(self.lib.shoop_dram_idle(self.handle))

    def drain(self, max_ticks=1_000_000):
        if not self.lib.shoop_dram_drain(self.handle, max_ticks):
            raise RuntimeError("DRAM model failed to drain within the tick limit")
        self.poll()

    def reset(self):
        self.lib.shoop_dram_destroy(self.handle)
        self.generation += 1
        self.done.clear()
        self._create()

    def report(self, *, final=False):
        if final and self.final_statistics is None:
            before = self.ticks
            self.drain()
            self.lib.shoop_dram_print_stats(self.handle)
            stats = json.loads((self.output/"dramsim3.json").read_text())
            reads = sum(s["num_read_cmds"] for s in stats.values())
            writes = sum(s["num_write_cmds"] for s in stats.values())
            read_hist, write_hist = Counter(), Counter()
            for channel in stats.values():
                read_hist.update(channel.get("read_latency") or {})
                write_hist.update(channel.get("write_latency") or {})
            self.final_statistics = dict(dram_read_commands=reads, dram_write_commands=writes,
                                         dram_command_bytes=(reads+writes)*self.native_bytes,
                                         refresh_commands=sum(s["num_ref_cmds"] for s in stats.values()),
                                         dram_read_service_dram_cycles=histogram_summary(read_hist),
                                         dram_write_service_dram_cycles=histogram_summary(write_hist),
                                         final_drain_dram_ticks=self.ticks-before)
        return dict(tck_fs=self.tck_fs, native_bytes=self.native_bytes, fabric_hz=self.fabric_hz,
                    ticks=self.ticks, idle=self.idle,
                    physical_counts_generation=self.generation,
                    physical_counts_finalized=self.final_statistics is not None,
                    read_service_dram_cycles=summary([e["cycles"] for e in self.events if not e["write"]]),
                    write_admission_dram_cycles=summary([e["cycles"] for e in self.events if e["write"]]),
                    **(self.final_statistics or {}))

    def close(self):
        if self.handle:
            self.report(final=True)
            self.lib.shoop_dram_destroy(self.handle)
            self.handle = None
