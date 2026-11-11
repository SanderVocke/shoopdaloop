"""Independent metadata slave, delayed responses and prefetch backpressure."""
import json
import os
from pathlib import Path
import random
import cocotb
from cocotb.triggers import Timer
import fpga_testbench as wire


@cocotb.test()
async def resolver_conformance(dut):
    views, slots, cf = map(int, os.environ["RESOLVER_CAPACITIES"].split(","))
    output = Path(os.environ["RESOLVER_OUTPUT"])
    rng = random.Random(20261111)
    metadata = [{"chunks": list(range(slots)), "frames": slots*cf}]
    for _ in range(min(views, 6)-2):
        ids = rng.sample(range(slots), rng.randint(1, slots))
        metadata.append({"chunks": ids, "frames": len(ids)*cf, "range": {"offset": 0}})
    metadata.append(None)
    requests = [{"view": 0, "position_chunks": slots-1, "length_chunks": slots, "chunks": 3}]
    for _ in range(160):
        view = rng.randrange(len(metadata)+2)
        n = len(metadata[view]["chunks"]) if view < len(metadata) and metadata[view] else 0
        requests.append({"view": view, "position_chunks": rng.randrange(n+2),
                         "length_chunks": rng.randrange(n+2), "chunks": rng.randrange(2*slots+1)})
    case = {"name": "isolated-resolver-wrap-missing-and-stalls", "requests": requests,
            "media": {"chunk_frames": cf, "samples": [0]*(slots*cf), "free_chunks": [], "views": metadata}}
    document = {"format_version": 2, "payload": {"kind": "resolver_case", "value": case}}
    wire.dump(document, output / "resolver.case.json")
    trace = wire.run_reference("trace", document, executable=Path(os.environ["RESOLVER_REFERENCE"]))
    pending = {"descriptor": None, "table": None}
    observed, held = [], {}

    def set_values(**values):
        for name, data in values.items():
            getattr(dut, name).value = data

    def read(name):
        return int(getattr(dut, name).value)

    async def tick():
        set_values(clk=0, prefetch_ready=int(rng.randrange(4) != 0))
        for port in pending:
            item = pending[port]
            set_values(**{port+"_ready": int(item is None and rng.randrange(3) != 0),
                          port+"_response": int(item is not None and item[0] == 0),
                          port+"_data": item[1] if item else 0})
        await Timer(5, units="ns")
        accepted = read("hint_valid") and read("hint_ready")
        if not read("reset"):
            for port in pending:
                if read(port+"_response"):
                    pending[port] = None
                elif pending[port] is not None:
                    pending[port][0] -= 1
                packet = read(port+"_address")
                if held.get(port) is not None:
                    assert read(port+"_valid") and packet == held[port]
                held[port] = packet if read(port+"_valid") and not read(port+"_ready") else None
                if read(port+"_valid") and read(port+"_ready"):
                    if port == "descriptor":
                        view = metadata[packet]
                        data = 0 if view is None else (1 | (int("range" in view) << 1)
                            | view["frames"] << 64 | len(view["chunks"]) << 128)
                    else:
                        view, ordinal = divmod(packet, slots)
                        data = metadata[view]["chunks"][ordinal]
                    pending[port] = [rng.randrange(4), data]
            if held.get("prefetch") is not None:
                assert read("prefetch_valid") and read("prefetch_chunk") == held["prefetch"]
            held["prefetch"] = read("prefetch_chunk") if read("prefetch_valid") and not read("prefetch_ready") else None
            if read("prefetch_valid") and read("prefetch_ready"):
                observed.append(read("prefetch_chunk"))
        set_values(clk=1)
        await Timer(5, units="ns")
        return accepted

    set_values(clk=0, reset=1, hint_valid=0, hint_view=0, hint_position=0, hint_length=0,
               hint_count=0, view_count=len(metadata), slot_count=slots)
    await tick()
    set_values(reset=0)
    for request, expected in zip(requests, trace["payload"]["value"]["results"]):
        observed.clear()
        set_values(hint_valid=1, hint_view=request["view"], hint_position=request["position_chunks"]*cf,
                   hint_length=request["length_chunks"]*cf, hint_count=request["chunks"]*cf)
        for _ in range(10000):
            if await tick():
                break
        else:
            raise AssertionError("resolver hint admission timed out")
        set_values(hint_valid=0)
        for _ in range(10000):
            if read("hint_ready"):
                break
            await tick()
        else:
            raise AssertionError("resolver did not finish")
        assert observed == expected, (request, observed, expected)
    (output / "summary.json").write_text(json.dumps({"requests": len(requests)})+"\n")
