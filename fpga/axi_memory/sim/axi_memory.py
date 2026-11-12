"""Clock-independent AXI slave state; timing and functional bytes are separate."""
import math
import random
import statistics


def summary(values):
    if not values:
        return {"count": 0}
    ordered = sorted(values)
    return {"count": len(values), "mean": statistics.mean(values),
            "median": statistics.median(values), "stddev": statistics.pstdev(values),
            "min": ordered[0], "max": ordered[-1],
            **{f"p{p}": ordered[max(0, math.ceil(len(values)*p/100)-1)] for p in (95, 99)}}


def histogram_summary(histogram):
    values = sorted((int(value), count) for value, count in histogram.items() if count)
    count = sum(n for _, n in values)
    if not count:
        return {"count": 0}
    def at(rank):
        cumulative = 0
        for value, n in values:
            cumulative += n
            if cumulative >= rank:
                return value
        raise AssertionError("histogram rank exceeds population")
    mean = sum(value*n for value, n in values)/count
    return dict(count=count, mean=mean, median=(at((count+1)//2)+at(count//2+1))/2,
                stddev=math.sqrt(sum((value-mean)**2*n for value, n in values)/count),
                min=values[0][0], max=values[-1][0],
                **{f"p{p}": at(math.ceil(count*p/100)) for p in (95, 99)})


class FixedTiming:
    native_bytes = 16

    def __init__(self, cycles=0):
        self.delay = cycles
        self.cycle = 0
        self.next_id = 0
        self.due = {}

    def advance(self):
        self.cycle += 1

    def submit(self, address, write):
        token = self.next_id
        self.next_id += 1
        self.due[token] = self.cycle+self.delay
        return token

    def ready(self, token):
        return self.cycle >= self.due[token]

    def reset(self):
        self.due.clear()

    def close(self):
        pass


class AxiMemory:
    is_native = False
    SLAVE = ("awready", "wready", "bid", "bresp", "bvalid", "arready",
             "rid", "rdata", "rresp", "rlast", "rvalid")
    MASTER = ("awid", "awaddr", "awlen", "awsize", "awburst", "awvalid", "wdata", "wstrb", "wlast", "wvalid",
              "bready", "arid", "araddr", "arlen", "arsize", "arburst", "arvalid", "rready")

    def __init__(self, dut, words, *, base=0x01000000, timing=None, seed=0, word_only=False):
        self.dut, self.words, self.base = dut, words, base
        self.width = len(dut.m_axi_wdata)//8
        self.timing = timing or FixedTiming()
        self.rng = random.Random(seed)
        self.word_only = word_only
        self.stalls = False
        self.pause = self.pause_responses = self.fail_next = False
        self.hold_aw = self.hold_w = self.hold_ar = False
        self.active = None
        self.write_beats = []
        self.offered_r = self.offered_b = None
        self.held = {}
        self.cycle = self.reads = self.writes = 0
        self.native_reads = self.native_writes = 0
        self.records = []
        self.address_stalls = self.write_stalls = self.response_stalls = self.submission_stalls = 0
        self.resetting = False

    @property
    def pending(self):
        if self.active is None:
            return None
        return [0, self.active.get("data", [0])[0], self.active["error"]]

    def read_bus(self, address):
        start = (address//self.width)*self.width-self.base
        result = 0
        for lane in range(self.width):
            offset = start+lane
            if 0 <= offset < len(self.words)*4:
                result |= ((self.words[offset//4] >> ((offset % 4)*8)) & 255) << (lane*8)
        return result

    def apply_write(self, address, data, strobe, size):
        lane_start = address % self.width
        assert strobe & ~(((1 << (1 << size))-1) << lane_start) == 0, "WSTRB outside transfer lanes"
        aligned = address-address % self.width
        for lane in range(self.width):
            if strobe & (1 << lane):
                offset = aligned+lane-self.base
                assert 0 <= offset < len(self.words)*4
                shift = (offset % 4)*8
                self.words[offset//4] = ((self.words[offset//4] & ~(255 << shift))
                                         | (((data >> (lane*8)) & 255) << shift))

    def drive(self):
        permitted = not self.pause and not self.resetting
        def admit(hold):
            return int(permitted and not hold and (not self.stalls or self.rng.randrange(4) != 0))
        active = self.active
        values = dict(awready=admit(self.hold_aw) if active is None else 0,
                      arready=admit(self.hold_ar) if active is None and not self.write_beats else 0,
                      wready=admit(self.hold_w) if (active is None or active["write"])
                      and len(self.write_beats) < 16 else 0,
                      bid=0, bresp=0, bvalid=0, rid=0, rdata=0, rresp=0, rlast=0, rvalid=0)
        respond = permitted and not self.pause_responses
        if active is not None and respond:
            if active["write"]:
                if (active["submitted"] and all(self.timing.ready(token) for token in active["tokens"].values())
                        and self.offered_b is None):
                    self.offered_b = int(active["error"])*2
                    active.setdefault("available", self.cycle+1)
            else:
                index = active["index"]
                address = active["address"]+(index << active["size"])
                block = address-address % self.timing.native_bytes
                token = active["tokens"].get(block)
                if token is not None and self.timing.ready(token) and self.offered_r is None:
                    self.offered_r = (active["data"][index], int(active["error"])*2, int(index == active["length"]))
                    active.setdefault("available", self.cycle+1)
        if self.offered_b is not None:
            values.update(bvalid=1, bresp=self.offered_b)
        if self.offered_r is not None:
            data, resp, last = self.offered_r
            values.update(rvalid=1, rdata=data, rresp=resp, rlast=last)
        for key, value in values.items():
            getattr(self.dut, "m_axi_"+key).value = value
        self.driven = values

    def sample(self):
        return {name: int(getattr(self.dut, "m_axi_"+name).value) for name in self.MASTER}

    def accept(self, packet, write):
        prefix = "aw" if write else "ar"
        address, length, size = (packet[prefix+key] for key in ("addr", "len", "size"))
        assert packet[prefix+"id"] == 0 and packet[prefix+"burst"] == 1
        assert 0 <= length < 16 and (1 << size) <= self.width and address % (1 << size) == 0
        end = address+((length+1) << size)
        assert address//4096 == (end-1)//4096
        assert self.base <= address < end <= self.base+len(self.words)*4, "AXI address outside arena"
        if self.word_only:
            assert length == 0 and size == 2, "pipeline emitted a non-word transaction"
        error, self.fail_next = self.fail_next, False
        first = address-address % self.timing.native_bytes
        blocks = list(range(first, end, self.timing.native_bytes))
        self.active = dict(address=address, length=length, size=size, write=write,
                           error=error, start=self.cycle, index=0, blocks=blocks,
                           tokens={}, submitted=False, beats=[],
                           data=[self.read_bus(address+(i << size)) for i in range(length+1)])
        self.writes += int(write)
        self.reads += int(not write)

    def finish(self):
        active = self.active
        self.records.append({key: active[key] for key in ("address", "length", "size", "write", "start", "first")})
        self.records[-1].update(end=self.cycle, latency=self.cycle-active["start"],
                                first_latency=active["first"]-active["start"],
                                available_latency=active["available"]-active["start"],
                                submission_latency=active["submission"]-active["start"],
                                beat_cycles=active["beats"])
        self.active = None
        self.write_beats = []

    def step(self, packet, reset=False):
        self.cycle += 1
        self.timing.advance()
        if reset:
            if not self.resetting:
                self.timing.reset()
            self.resetting = True
            self.active = None
            self.write_beats = []
            self.offered_r = self.offered_b = None
            self.held = {}
            return
        self.resetting = False
        self.address_stalls += sum(bool(packet[c+"valid"] and not self.driven[c+"ready"]) for c in ("aw", "ar"))
        self.write_stalls += int(bool(packet["wvalid"] and not self.driven["wready"]))
        self.response_stalls += sum(bool(self.driven[c+"valid"] and not packet[c+"ready"]) for c in ("r", "b"))
        for channel, fields in (("aw", ("awid", "awaddr", "awlen", "awsize", "awburst")),
                                ("ar", ("arid", "araddr", "arlen", "arsize", "arburst")),
                                ("w", ("wdata", "wstrb", "wlast"))):
            payload = tuple(packet[field] for field in fields)
            if channel in self.held:
                assert packet[channel+"valid"] and payload == self.held[channel], f"stalled {channel} changed"
            if packet[channel+"valid"] and not self.driven[channel+"ready"]:
                self.held[channel] = payload
            else:
                self.held.pop(channel, None)
        if self.driven["bvalid"] and packet["bready"]:
            self.active.setdefault("first", self.cycle)
            self.active["beats"].append(self.cycle)
            self.offered_b = None
            self.finish()
        if self.driven["rvalid"] and packet["rready"]:
            self.active.setdefault("first", self.cycle)
            self.active["beats"].append(self.cycle)
            last = self.offered_r[2]
            self.offered_r = None
            self.active["index"] += 1
            if last:
                self.finish()
        if packet["awvalid"] and self.driven["awready"]:
            assert self.active is None
            self.accept(packet, True)
        if packet["arvalid"] and self.driven["arready"]:
            assert self.active is None
            self.accept(packet, False)
        if packet["wvalid"] and self.driven["wready"]:
            self.write_beats.append((packet["wdata"], packet["wstrb"], packet["wlast"]))
        active = self.active
        if active is None:
            return
        if active["write"] and not active.get("written"):
            if len(self.write_beats) < active["length"]+1:
                return
            assert len(self.write_beats) == active["length"]+1
            for index, (data, strobe, last) in enumerate(self.write_beats):
                assert last == (index == active["length"])
                if not active["error"]:
                    self.apply_write(active["address"]+(index << active["size"]), data, strobe, active["size"])
            active["written"] = True
        for address in active["blocks"]:
            if address not in active["tokens"]:
                token = self.timing.submit(address, active["write"])
                if token is None:
                    self.submission_stalls += 1
                    break
                active.setdefault("submission", self.cycle)
                active["tokens"][address] = token
                self.native_writes += int(active["write"])
                self.native_reads += int(not active["write"])
        active["submitted"] = len(active["tokens"]) == len(active["blocks"])

    def report(self, records=None):
        records = self.records if records is None else records
        groups = {}
        for write in (False, True):
            for burst in (False, True):
                selected = [r for r in records if r["write"] == write and (r["length"] > 0) == burst]
                name = ("write" if write else "read")+("_burst" if burst else "_word")
                groups[name] = {"latency_cycles": summary([r["latency"] for r in selected]),
                                "first_response_cycles": summary([r["first_latency"] for r in selected]),
                                "response_available_cycles": summary([r["available_latency"] for r in selected]),
                                "native_submission_cycles": summary([r["submission_latency"] for r in selected]),
                                "interbeat_cycles": summary([b-a for r in selected for a,b in zip(r["beat_cycles"], r["beat_cycles"][1:])])}
        return dict(cycles=self.cycle, reads=self.reads, writes=self.writes,
                    logical_bytes=sum((r["length"]+1) << r["size"] for r in records),
                    native_reads=self.native_reads, native_writes=self.native_writes,
                    native_requested_bytes=(self.native_reads+self.native_writes)*self.timing.native_bytes,
                    address_stall_cycles=self.address_stalls, write_stall_cycles=self.write_stalls,
                    response_stall_cycles=self.response_stalls, native_queue_stall_cycles=self.submission_stalls,
                    groups=groups)

    def close(self):
        self.timing.close()

    async def load_words(self, words, tick):
        self.words[:] = words

    async def read_words(self, tick):
        return list(self.words)
