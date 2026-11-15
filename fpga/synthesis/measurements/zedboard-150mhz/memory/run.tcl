if {[version -short] ne {2021.1}} {error "Vivado 2021.1 required"}
set_param general.maxThreads 1
open_checkpoint [lindex $argv 0]
set_clock_uncertainty 0.200 [get_clocks core_clk]
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
route_design
phys_opt_design -directive AggressiveExplore
route_design
write_checkpoint -force post_route.dcp
reports post_route
report_route_status -file post_route_route_status.rpt
report_drc -file post_route_drc.rpt
