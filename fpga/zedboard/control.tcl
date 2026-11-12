if {$argc < 2} {error "usage: vivado -mode batch -source fpga/zedboard/control.tcl -tclargs BUILD_DIRECTORY ACTION ?QUANTA WARMUP GAP WATCHDOG?"}
source [file join [file normalize [lindex $argv 0]] manifest.tcl]
set action [lindex $argv 1]
open_hw_manager
connect_hw_server -url localhost:3121
open_hw_target
set device [lindex [get_hw_devices -filter {PART =~ "xc7z020*"}] 0]
if {$device eq ""} {error "Zynq-7020 not found"}
current_hw_device $device
set_property PROBES.FILE [file join $bench_output bench.ltx] $device
refresh_hw_device $device
set vio [lindex [get_hw_vios -of_objects $device -filter {CELL_NAME =~ "*vio*"}] 0]
if {$vio eq ""} {error "Benchmark VIO not found"}
proc probe {direction index} {
    global vio
    set type [expr {$direction eq "in" ? "vio_input" : "vio_output"}]
    set result [get_hw_probes -of_objects $vio -filter "TYPE == $type && PROBE_PORT == $index"]
    if {[llength $result] != 1} {error "Cannot identify VIO $direction probe $index"}
    return [lindex $result 0]
}
proc output {index value} {
    set p [probe out $index]
    set_property OUTPUT_VALUE_RADIX HEX $p
    set_property OUTPUT_VALUE [format %x $value] $p
    commit_hw_vio $p
}
proc pulse {index} {output $index 0; output $index 1; output $index 0}
proc input {index} {
    set p [probe in $index]
    set_property INPUT_VALUE_RADIX HEX $p
    set hex [get_property INPUT_VALUE $p]
    if {![regexp {^(?:0x)?([[:xdigit:]]+)$} $hex unused hex]} {error "Invalid VIO input: $hex"}
    return [expr "0x$hex"]
}
proc wait_stopped {} {
    global vio
    set deadline [expr {[clock milliseconds]+60000}]
    while {1} {
        after 100
        refresh_hw_vio $vio
        if {([input 0] & 7) in {0 5}} {return}
        if {[clock milliseconds] > $deadline} {error "Host polling timeout; inspect VIO/ILA or request stop"}
    }
}
if {$action in {run measure}} {
    set values [list 1000 $bench_warmup 0 10000000]
    for {set i 0} {$i < 4} {incr i} {
        if {$argc > $i+2} {lset values $i [lindex $argv [expr {$i+2}]]}
        set value [lindex $values $i]
        if {![string is entier -strict $value] || $value < 0 || $value > 0xffffffff} {error "Controls require unsigned 32-bit integers"}
        output [expr {$i+3}] $value
    }
    if {$action eq "measure" && [lindex $values 0] == 0} {error "Use run for an unlimited measurement"}
    if {[lindex $values 0] != 0 && [lindex $values 0] <= [lindex $values 1]} {error "Total quanta must exceed warmup quanta"}
    refresh_hw_vio $vio
    if {([input 0] & 7) ni {0 5}} {error "Runner is active; stop it before starting another measurement"}
    pulse 0
    if {$action eq "run"} {puts "Benchmark started"; close_hw_manager; return}
    wait_stopped
} elseif {$action eq "stop"} {
    pulse 1
    wait_stopped
} elseif {$action ne "snapshot"} {
    error "ACTION must be run, measure, stop or snapshot"
}
pulse 2
refresh_hw_vio $vio
set result [dict create]
set rows [list "  \"fabric_hz\": [expr {wide($bench_fabric_mhz*1000000)}]"]
for {set i 0} {$i < 16} {incr i} {
    set name [lindex $bench_statistics $i]
    set value [input [expr {$i+1}]]
    dict set result $name $value
    lappend rows "  \"$name\": $value"
}
set channel [open [file join $bench_output results.json] w]
puts $channel "\{\n[join $rows ,\n]\n\}"
close $channel
puts $result
if {[dict get $result measured_quanta] != 0} {
    puts "Average processing time (us): [expr {double([dict get $result total_cycles])/[dict get $result measured_quanta]/$bench_fabric_mhz}]"
}
puts "Snapshot saved to [file join $bench_output results.json]"
close_hw_manager
if {[dict get $result status] & 16} {error "Run failed; inspect the retained status/error fields"}
