# Vivado 2021.1 IP catalog

A matching public catalog is available in
[`pwang7/vivado_ubuntu:standard-2021.1`](https://hub.docker.com/r/pwang7/vivado_ubuntu/tags?name=standard-2021.1).
The verified image digest is
`sha256:32542c4687bcce84dbaafc8135508dca01c6261f22b195bc10558b7a69beffd7`.
Its second filesystem layer contains the catalog at
`/tools/Xilinx/Vivado/2021.1/data/ip`; the layer digest is
`sha256:60cd7af3afde6e3e8a55efc2babc577c0220e4be92ce75cd6ed900315996fa13`.
That layer is 21,158,655,945 compressed bytes.

The catalog contains the required versions and revisions:

| IP | Version | Revision |
| --- | --- | ---: |
| processing_system7 | 5.5 | 6 |
| proc_sys_reset | 5.0 | 13 |
| xlconstant | 1.1 | 7 |
| vio | 3.0 | 20 |
| ila | 6.2 | 11 |

For a machine with enough Docker storage, extract the directory from a stopped
container. This does not execute the image's entrypoint or scripts:

```sh
docker pull pwang7/vivado_ubuntu@sha256:32542c4687bcce84dbaafc8135508dca01c6261f22b195bc10558b7a69beffd7
catalog_container=$(docker create pwang7/vivado_ubuntu@sha256:32542c4687bcce84dbaafc8135508dca01c6261f22b195bc10558b7a69beffd7)
mkdir -p work/vivado-catalog
docker cp "$catalog_container:/tools/Xilinx/Vivado/2021.1/data/ip" work/vivado-catalog/
docker rm "$catalog_container"
```

The validation workspace instead streamed and SHA256-verified the entire source
layer, then extracted all 26,198 catalog entries. OCI hardlinks referring outside
the catalog were resolved using their original targets from the same verified
layer. Extracting only pathname matches from a raw tar without recovering these
targets leaves an incomplete catalog.

The catalog was mounted read-only at the canonical `data/ip` location alongside
the unmodified `siliconbootcamp/xilinx-vivado` vendor installation. No compiler,
configuration overlay or license files from the catalog image were used. Vivado
reports **2021.1, SW Build 3247384, IP Build 3246043**. This combination passed
block-design validation, all IP output generation, full synthesis, placement,
routing and bitstream generation, including automatic JTAG debug-hub synthesis.

The build also accepts `SHOOP_IP_REPO` for a matching external repository. The
validation above used the canonical installation path, including dependencies
needed by automatic debug-hub generation.

The container runtime also required preloading its ordinary system `libudev`:

```sh
LD_PRELOAD=/lib/x86_64-linux-gnu/libudev.so.1 vivado -mode batch \
  -source fpga/zedboard/build.tcl -tclargs work/zedboard/build
```

Without it, a project launch crashed in `malloc_usable_size` during `libudev`
device enumeration, called by Vivado host registration/Webtalk. The preload
also reaches the automatically launched debug-hub synthesis process. This uses
the original OS library and normal `/sys` view; no stubs, vendor-file patches
or `/sys` masking are involved. A similar container workaround is documented by
[OpenCPI](https://gitlab.com/opencpi/opencpi/-/work_items/3733).

See [validation results](VALIDATION.md) for the full board build outcome and
[catalog provenance](validation/2021.1/catalog-provenance.json) for source
digests and required-IP descriptor hashes. Vendor catalog files are not checked
into this repository.
