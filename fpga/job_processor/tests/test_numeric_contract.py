"""Check real RTL arithmetic against the production numeric policy."""
import json
import os
from pathlib import Path
import subprocess

import cocotb

from audio_cases import make_case, ramp
from audio_driver import AudioDriver
from cases import ins


@cocotb.test()
async def production_numeric_vectors(dut):
    capacities = tuple(map(int, os.environ["AUDIO_CAPACITIES"].split(",")))
    executable = Path(os.environ["NUMERIC_VECTORS"])
    comparisons = 0
    for frames in sorted({1, min(3, capacities[3]), capacities[3]}):
        result = subprocess.run([str(executable), str(frames)], check=True, text=True, capture_output=True)
        for index, line in enumerate(result.stdout.splitlines()):
            vector = json.loads(line)
            instructions = [ins("input", port=0, destination=0),
                            ins("gain", ramp=0, source=0, destination=1),
                            ins("input", port=1, destination=2),
                            ins("mix", left=1, right=2, destination=3),
                            ins("output", source=3, port=0)]
            memory = [ramp(vector["start"], vector["target"], vector["duration"], vector["elapsed"])]
            case = make_case(f"numeric-q{frames}-{index}", instructions, memory, capacities,
                             frames=frames, buffers=4, outputs=1, steps=1,
                             inputs=[vector["first"], vector["second"]])
            driver = AudioDriver(dut, capacities, index)
            await driver.load(case)
            stage, code = await driver.execute_audio(case["steps"][0]["inputs"])
            assert (stage, code) == (2, 0)
            _, outputs = await driver.read_samples()
            assert outputs[0] == vector["expected"], case["name"]
            state = await driver.snapshot()
            assert state["memory"][0]["value"]["elapsed"] == min(
                vector["duration"], vector["elapsed"] + frames)
            comparisons += frames
    dut._log.info("production numeric contract: %d exact output samples", comparisons)
