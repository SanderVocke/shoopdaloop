if {[version -short] ne {2021.1}} {error "Vivado 2021.1 required"}
set_param general.maxThreads 1
create_project -in_memory -part xc7z020clg484-1
read_verilog -sv {source/fpga/midi/rtl/midi_expression_state.sv context.sv}
read_xdc context.xdc
proc reports {stage} {
    report_utilization -file ${stage}_utilization.rpt
    report_utilization -hierarchical -file ${stage}_hierarchical_utilization.rpt
    set cells [get_cells -quiet -hierarchical -filter {ORIG_REF_NAME == midi_expression_state}]
    report_utilization -cells $cells -file ${stage}_expression_utilization.rpt
    report_timing_summary -delay_type min_max -max_paths 10 -file ${stage}_timing_summary.rpt
    report_timing -max_paths 10 -file ${stage}_critical_paths.rpt
    report_control_sets -file ${stage}_control_sets.rpt
}
set stage synth
set status [catch {
    synth_design -top session_context -part xc7z020clg484-1 -mode out_of_context -flatten_hierarchy rebuilt
    reports post_synth
    opt_design
    reports post_opt
    if {"route" eq "route"} {
        set stage place
        place_design
        phys_opt_design
        reports post_place
        set stage route
        route_design
        reports post_route
        report_route_status -file post_route_route_status.rpt
    }
} message options]
set f [open outcome.txt w]
puts $f "stage=$stage\nstatus=$status\nmessage=$message"
close $f
if {$status} {puts stderr [dict get $options -errorinfo];exit 1}
exit 0
