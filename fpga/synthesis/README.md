# Standalone subsystem synthesis

Retained open-source checks:

```sh
bash fpga/state_processor/check.sh
bash fpga/job_processor/check.sh
bash fpga/media_store/check.sh
bash fpga/axi_memory/check_pipeline.sh
```

The last command lints AXI32/64 component tops and synthesizes the TP-free
`media_audio_pipeline_axi`. These commands require Yosys/Verilator as documented
by their scripts.

`subsystems.py state|job|memory --output DIRECTORY` retains Vivado 2021.1
standalone measurements for the ZedBoard part. It collects only execution and
memory sources and uses the registered context generator in `run.py`.
No complete loop core or ARM board demo synthesis target remains.

`measurements/` is historical evidence with original source manifests, revisions,
constraints and reports. Earlier system and 150 MHz results do not certify the
current component-only architecture. Preserve those measured artifacts unchanged.

`midi_session.py before|after --mhz 100|150 --output DIRECTORY` prepares the
frozen 64-stereo-loop comparison, including registered component boundaries,
historical RTL snapshots, Vivado 2021.1 compatibility patches, source hashes,
constraints and the implementation script. Run `vivado -mode batch -source
run.tcl` in the output directory. This composes the audio and optional MIDI
components for resource/timing measurement; it does not add a board frontend.
See the [Zynq-7020 comparison](../../docs/fpga/midi-vivado-7020.md) for the
configuration, storage requirements and results.
