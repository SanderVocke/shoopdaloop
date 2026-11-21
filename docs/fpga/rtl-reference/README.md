# FPGA RTL technical reference

Read [the manual](manual.md) and the [opcode appendices](opcodes.md) in the
repository, including the [musical-state chapter](manual.md#midi-expression-state-and-restoration)
and [state ABI appendix](opcodes.md#midi-expression-state-abi), or build the standalone HTML and paginated PDF. Markdown is the master text. The SVGs in [figures](figures/) are
the editable Inkscape sources and are used directly by both builds. Labels,
blocks and connectors remain separate SVG objects.

## Build

Install Pandoc with `--embed-resources` support, WeasyPrint and its native
dependencies, GNU Make, a shell with `base64`, and the DejaVu fonts. The build was checked with
Pandoc 3.1.11.1 and WeasyPrint 70.0. Inkscape is needed for editing figures.
Build from the repository root.

```sh
make -C docs/fpga/rtl-reference
```

The outputs are `work/fpga-rtl-reference/fpga-rtl-reference.html` and
`work/fpga-rtl-reference/fpga-rtl-reference.pdf`. The HTML embeds the
stylesheet and figures. Use the `html` or `pdf` target to build one format.
`OUT` overrides the destination. `PANDOC` and `WEASYPRINT` override the
executable paths.
