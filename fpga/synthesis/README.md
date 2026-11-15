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
