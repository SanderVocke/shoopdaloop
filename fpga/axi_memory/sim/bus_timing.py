"""Measure physical DDR activity within the RTL quantum execution interval."""


class QuantumBusTiming:
    def __init__(self, dut, timing):
        self.dut, self.timing = dut, timing
        self.begin()

    def begin(self):
        self.admitted = False
        self.start = self.end = None

    def observe(self, cycle):
        if self.end is not None:
            return
        if not self.admitted:
            self.admitted = bool(int(self.dut.run_valid.value) and int(self.dut.run_ready.value))
            return
        if self.start is None:
            self.start = (cycle, self.timing.bus_snapshot())
        if int(self.dut.done.value):
            self.end = (cycle, self.timing.bus_snapshot())

    def finish(self, total_cycles):
        if self.start is None or self.end is None or self.end[0]-self.start[0] != total_cycles:
            raise AssertionError("bus observation interval differs from quantum timing")
        return self.timing.bus_utilization(self.start[1], self.end[1])
