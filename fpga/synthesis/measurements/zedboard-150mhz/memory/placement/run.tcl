if {[version -short] ne {2021.1}} {error "This experiment requires Vivado 2021.1"}
set stage [lindex $argv 0]
set_param general.maxThreads [lindex $argv 1]
set generics [lindex $argv 2]
set strategy [lindex $argv 3]
set part xc7z020clg484-1
create_project -in_memory -part $part
set_property include_dirs [list [file normalize source/fpga/state_processor/rtl]] [current_fileset]
set sources {}
foreach directory {state_processor job_processor transition_processor chunk_memory media_store loop_pipeline axi_memory} {
    foreach source [lsort [glob source/fpga/$directory/rtl/*.sv]] {lappend sources $source}
}
lappend sources context.sv
read_verilog -sv $sources
read_xdc context.xdc
set f [open setup.txt w]
puts $f "Vivado: [version]\nPart: $part\nGenerics: $generics\nClock: 150 MHz\nUncertainty: 0.2 ns\nStrategy: $strategy"
close $f
proc reports {stage} {
    report_utilization -file ${stage}_utilization.rpt
    report_utilization -cells [get_cells core_under_test] -file ${stage}_core_utilization.rpt
    report_utilization -hierarchical -file ${stage}_hierarchical_utilization.rpt
    report_timing_summary -delay_type min_max -report_unconstrained -check_timing_verbose -max_paths 20 -file ${stage}_timing_summary.rpt
    report_timing -delay_type max -max_paths 30 -path_type full_clock_expanded -file ${stage}_critical_paths.rpt
    check_timing -verbose -file ${stage}_check_timing.rpt
    set setup_slack [get_property SLACK [get_timing_paths -delay_type max -max_paths 1]]
    set hold_slack [get_property SLACK [get_timing_paths -delay_type min -max_paths 1]]
    set met [expr {$setup_slack >= 0 && $hold_slack >= 0}]
    set f [open ${stage}.json w]
    puts $f "{\"part\":\"xc7z020clg484-1\",\"clock_mhz\":150,\"wns_ns\":$setup_slack,\"whs_ns\":$hold_slack,\"met_150mhz\":[expr {$met ? {true} : {false}}]}"
    close $f
}
synth_design -top registered_core_context -part $part -mode out_of_context -flatten_hierarchy rebuilt -generic $generics
write_checkpoint -force post_synth.dcp
reports post_synth
if {$stage eq {route}} {
    if {$strategy eq {timing}} {
        set_clock_uncertainty -setup 0.900 [get_clocks core_clk]
        opt_design -directive Explore
        place_design -directive ExtraNetDelay_high
        phys_opt_design -directive AggressiveExplore
        set_clock_uncertainty -setup 0.200 [get_clocks core_clk]
    } else {
        opt_design
        place_design
        phys_opt_design
    }
    write_checkpoint -force post_place.dcp
    reports post_place
    if {$strategy eq {timing}} {
        set_clock_uncertainty -setup 0.900 [get_clocks core_clk]
        route_design -directive AggressiveExplore
        phys_opt_design -directive AggressiveExplore
        route_design -directive AggressiveExplore
        set_clock_uncertainty -setup 0.200 [get_clocks core_clk]
    } else {
        route_design
        phys_opt_design -directive AggressiveExplore
        route_design
    }
    write_checkpoint -force post_route.dcp
    reports post_route
    report_route_status -file post_route_route_status.rpt
    report_drc -file post_route_drc.rpt
}
