if {$argc != 1} {error "usage: xsdb fpga/zedboard/load.tcl BUILD_DIRECTORY"}
set deployment [file normalize [lindex $argv 0]]
foreach name {bench.bit ps7_init.tcl} {
    if {![file isfile [file join $deployment $name]]} {error "Missing deployment artifact: $name"}
}
connect -url TCP:127.0.0.1:3121
targets -set -nocase -filter {name =~ "APU*"}
rst -system
targets -set -nocase -filter {name =~ "*Cortex-A9 MPCore #1*"}
stop
targets -set -nocase -filter {name =~ "*Cortex-A9 MPCore #0*"}
stop
targets -set -nocase -filter {name =~ "xc7z020*"}
fpga -file [file join $deployment bench.bit]
targets -set -nocase -filter {name =~ "APU*"}
source [file join $deployment ps7_init.tcl]
ps7_init
ps7_post_config
targets -set -nocase -filter {name =~ "*Cortex-A9 MPCore #0*"}
mwr 0x00100000 0x12345678
set scratch [mrd -value 0x00100000]
mwr 0x00100000 0
if {$scratch != 0x12345678} {error [format "DDR scratch readback failed: 0x%08x" $scratch]}
puts "DDR and HP0 initialized; CPUs remain halted. Start the benchmark through VIO."
