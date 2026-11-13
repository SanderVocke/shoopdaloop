# Python preparation ABI

These packed-field helpers serve the existing Python boot-ROM preparation tools.
They contain no HDL driver or simulation framework. Native testbenches use the
production Rust encoders through `rt_fpga_testbench::codec` and share Rust loader
construction through `prepared.rs`.

The default ROM generation check retains a byte-for-byte reference for the
prepared hardware image. Changes to the packed RTL ABI must update the production
Rust encoder, these preparation helpers, and the generated images together.
