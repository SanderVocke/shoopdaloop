if {$argc != 2} {
    error "usage: vivado -mode batch -source fpga/host_bridge/synthesis.tcl -tclargs OUTPUT {ooc|route}"
}
set root [file normalize [file join [file dirname [info script]] ../..]]
set output [file normalize [lindex $argv 0]]
set stage [lindex $argv 1]
if {$stage ni {ooc route}} {error "Unknown synthesis stage $stage"}
file mkdir $output
cd $output

set host [file join $root fpga host_bridge rtl]
set files [list [file join $host host_registers.sv]]
foreach directory {state_processor job_processor chunk_memory media_store axi_memory} {
    lappend files {*}[lsort [glob [file join $root fpga $directory rtl *.sv]]]
}
foreach source {host_mailbox.sv host_dispatch.sv host_pipeline_axi.sv} {
    lappend files [file join $host $source]
}
set_property include_dirs [list [file join $root fpga state_processor rtl]] [current_fileset]
read_verilog -sv $files

set constraints [file join $output clock.xdc]
set channel [open $constraints w]
puts $channel {create_clock -name fabric -period 10.000 [get_ports clk]}
close $channel
read_xdc $constraints
synth_design -top host_pipeline_axi -part xc7z020clg400-2 -mode out_of_context
write_checkpoint -force [file join $output host_synth.dcp]
report_utilization -hierarchical -file [file join $output utilization_synth.rpt]
if {$stage eq "route"} {
    opt_design
    place_design
    phys_opt_design
    route_design
    write_checkpoint -force [file join $output host_route.dcp]
    report_timing_summary -file [file join $output timing_route.rpt]
    report_route_status -file [file join $output route_status.rpt]
    report_utilization -hierarchical -file [file join $output utilization_route.rpt]
    report_drc -file [file join $output drc_route.rpt]
}
