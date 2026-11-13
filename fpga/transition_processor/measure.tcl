set stage [lindex $argv 0]
set_param general.maxThreads [lindex $argv 1]
set part xc7z020clg484-1
set generics "LOOP_DEPTH=[lindex $argv 2] REQUEST_DEPTH=[lindex $argv 3] EVENT_DEPTH=[lindex $argv 4] INTEGRATED=[lindex $argv 5]"
if {[llength $argv] > 6} { append generics " HAS_EXEC_INTERFACE=[lindex $argv 6]" }
create_project -in_memory -part $part
read_verilog -sv [lsort [glob source/*.sv]]
if {[file exists measurement_adapter.sv]} { read_verilog -sv measurement_adapter.sv }
read_verilog -sv context.sv
read_xdc context.xdc
set f [open setup.txt w]
puts $f "Vivado: [version]\nPart: $part\nGenerics: $generics"
close $f
proc reports {stage} {
    report_utilization -file ${stage}_utilization.rpt
    report_utilization -cells [get_cells core_under_test] -file ${stage}_tp_utilization.rpt
    report_utilization -hierarchical -file ${stage}_hierarchical.rpt
    report_timing_summary -delay_type min_max -report_unconstrained -max_paths 10 -file ${stage}_timing.rpt
    report_timing -delay_type max -max_paths 10 -file ${stage}_paths.rpt
    set f [open ${stage}.json w]
    set counts {}
    foreach type {LUT1 LUT2 LUT3 LUT4 LUT5 LUT6 LUT6_2 FDRE FDSE FDCE FDPE RAMB36E1 RAMB18E1 CARRY4 MUXF7 MUXF8} {
        set count [llength [get_cells -hier -filter "REF_NAME == $type && NAME =~ core_under_test/*"]]
        lappend counts "\"$type\":$count"
    }
    set setup_slack [get_property SLACK [get_timing_paths -delay_type max -max_paths 1]]
    set hold_slack [get_property SLACK [get_timing_paths -delay_type min -max_paths 1]]
    puts $f "{\"cells\":{[join $counts ,]},\"wns_ns\":$setup_slack,\"whs_ns\":$hold_slack}"
    close $f
}
synth_design -top registered_core_context -part $part -mode out_of_context -flatten_hierarchy rebuilt -generic $generics
write_checkpoint -force post_synth.dcp
reports post_synth
if {$stage eq {route}} {
    opt_design
    place_design
    phys_opt_design
    route_design
    phys_opt_design -directive AggressiveExplore
    route_design
    write_checkpoint -force post_route.dcp
    reports post_route
    report_route_status -file post_route_route_status.rpt
    report_drc -file post_route_drc.rpt
}
