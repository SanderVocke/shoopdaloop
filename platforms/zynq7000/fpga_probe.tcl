if {$argc != 6} {
    error "usage: xsdb fpga_probe.tcl SERVER PS_INIT BITSTREAM ELF NONCE_LOW NONCE_HIGH"
}
lassign $argv server ps_init bitstream elf nonce_low nonce_high
connect -url $server
targets -set -nocase -filter {name =~ "APU*"}
rst -system
targets -set -nocase -filter {name =~ "*Cortex-A9 MPCore #1*"}
stop
targets -set -nocase -filter {name =~ "*Cortex-A9 MPCore #0*"}
stop
targets -set -nocase -filter {name =~ "APU*"}
source $ps_init
ps7_init
fpga -file $bitstream
ps7_post_config
targets -set -nocase -filter {name =~ "*Cortex-A9 MPCore #0*"}
mwr 0x00100000 0x12345678
if {[mrd -value 0x00100000] != 0x12345678} {error "DDR readback failed"}
mwr 0x00100000 0
dow $elf
mwr 0x8000 $nonce_low
mwr 0x8004 $nonce_high
con
