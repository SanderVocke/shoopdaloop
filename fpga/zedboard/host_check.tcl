# Load the host flavor and exercise its register map from xsdb, with both CPUs halted.
# usage: xsdb fpga/zedboard/host_check.tcl BUILD_DIRECTORY ?ARENA_BASE?
if {$argc < 1 || $argc > 2} {error "usage: xsdb fpga/zedboard/host_check.tcl BUILD_DIRECTORY ?ARENA_BASE?"}
set deployment [file normalize [lindex $argv 0]]
set arena [expr {$argc == 2 ? [lindex $argv 1] : 0x17800000}]
foreach name {bench.bit ps7_init.tcl} {
    if {![file isfile [file join $deployment $name]]} {error "Missing deployment artifact: $name"}
}
set base 0x40000000
array set reg {
    ID 0x000 VERSION 0x004 CONTROL 0x008 STATUS 0x00c QUANTA 0x010 LAST_CYCLES 0x018 CAPACITY 0x040
    RUN 0x080 RUN_GO 0x0a0 TP_OPCODE 0x0c0 TP_INDEX 0x0c4 TP_GO 0x0c8 TP_ERROR 0x0cc TP_DATA 0x100
    TP_RESPONSE 0x180
}
set capacities {PROGRAM_DEPTH STATE_DEPTH BUFFERS MAX_FRAMES INPUT_PORTS OUTPUT_PORTS LOOP_DEPTH
    REQUEST_DEPTH EVENT_DEPTH VIEWS SLOTS BLOCK_CHUNKS CHUNK_FRAMES EXTERNAL_IO AXI_DATA_WIDTH PHYSICAL_BASE}
proc rd {name {offset 0}} {global base reg; return [mrd -force -value [expr {$base + $reg($name) + $offset}]]}
proc wr {name value {offset 0}} {global base reg; mwr -force [expr {$base + $reg($name) + $offset}] $value}
proc wait_clear {bit what} {
    for {set i 0} {$i < 100000} {incr i} {
        if {([rd STATUS] >> $bit & 1) == 0} {return}
    }
    error "$what did not complete; status [format 0x%08x [rd STATUS]]"
}
proc transition {opcode words} {
    wr TP_OPCODE $opcode
    wr TP_INDEX 0
    for {set i 0} {$i < 20} {incr i} {wr TP_DATA [expr {$i < [llength $words] ? [lindex $words $i] : 0}] [expr {4*$i}]}
    wr TP_GO 1
    wait_clear 17 "TP opcode $opcode"
    set error [rd TP_ERROR]
    if {$error != 0} {error "TP opcode $opcode error $error"}
    return [list [rd TP_RESPONSE] [rd TP_RESPONSE 4]]
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
if {[mrd -value 0x00100000] != 0x12345678} {error "DDR scratch readback failed"}
mwr 0x00100000 0

set identity [rd ID]
if {$identity != 0x53484f50} {error [format "no host bridge at %#x: identity %#010x" $base $identity]}
puts "bridge version [rd VERSION]"
for {set i 0} {$i < [llength $capacities]} {incr i} {
    set value [rd CAPACITY [expr {4*$i}]]
    puts [format "  %-15s %u" [lindex $capacities $i] $value]
    if {[lindex $capacities $i] eq "PHYSICAL_BASE" && $value != $arena} {
        error [format "bitstream arena %#010x differs from expected %#010x" $value $arena]
    }
}
wr CONTROL 1
wait_clear 22 "core reset"
if {([rd STATUS] >> 23 & 1) == 0} {error "AXI not quiescent after reset"}
puts "frame after reset: [lindex [transition 0 {}] 0]"
transition 1 {}
# One loop and one request need at least 2*loops+requests event slots.
transition 3 [list 1 1 3 0 32]
foreach {offset value} {0 0 4 0 8 32 12 32 16 0 20 0 24 2 28 2} {wr RUN $value $offset}
wr RUN_GO 1
wait_clear 16 "quantum"
set status [rd STATUS]
if {$status & 0x7} {error [format "core fault after quantum: status %#010x" $status]}
set frame [lindex [transition 0 {}] 0]
puts "quanta [rd QUANTA], last quantum [rd LAST_CYCLES] cycles, frame $frame"
if {[rd QUANTA] != 1 || $frame != 32} {error "quantum did not advance the core"}
puts "Host bridge check passed"
