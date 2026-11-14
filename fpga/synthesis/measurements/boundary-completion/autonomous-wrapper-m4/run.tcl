set_param general.maxThreads [lindex $argv 0]
set part xc7z020clg400-1
create_project -in_memory -part $part
set_property include_dirs [list [file normalize source/fpga/state_processor/rtl]] [current_fileset]
set sources {}
foreach directory {state_processor job_processor transition_processor chunk_memory media_store loop_pipeline axi_memory audio_io} {
    foreach source [lsort [glob source/fpga/$directory/rtl/*.sv]] {lappend sources $source}
}
lappend sources registered_core_context.sv
read_verilog -sv $sources
read_xdc core.xdc
set generics {AXI_DATA_WIDTH=64 BURST_MEMORY=1 LOOP_DEPTH=64 REQUEST_DEPTH=128 EVENT_DEPTH=256 PROGRAM_DEPTH=512 STATE_DEPTH=512 JOB_DEPTH=512 MAX_FRAMES=32 BUFFERS=256 INPUT_PORTS=8 OUTPUT_PORTS=8 VIEWS=64 SLOTS=2048 CHUNK_FRAMES=32 BLOCK_CHUNKS=32 READ_BUFFERS=4 WRITE_BUFFERS=4}
set f [open setup.txt w]
puts $f "Vivado: [version]\nPart: $part\nTop: registered_core_context\nGenerics: $generics"
close $f
proc reports {stage} {
    report_utilization -file ${stage}_utilization.rpt
    report_utilization -cells [get_cells core_under_test] -file ${stage}_core_utilization.rpt
    report_utilization -hierarchical -file ${stage}_hierarchical_utilization.rpt
    report_timing_summary -delay_type min_max -report_unconstrained -check_timing_verbose -max_paths 10 -file ${stage}_timing_summary.rpt
    report_timing -delay_type max -max_paths 10 -path_type full_clock_expanded -file ${stage}_critical_paths.rpt
    check_timing -verbose -file ${stage}_check_timing.rpt
}
synth_design -top registered_core_context -part $part -mode out_of_context -flatten_hierarchy rebuilt -generic $generics
write_checkpoint -force post_synth.dcp
reports post_synth
opt_design
place_design
phys_opt_design
write_checkpoint -force post_place.dcp
reports post_place
route_design
phys_opt_design -directive AggressiveExplore
route_design
write_checkpoint -force post_route.dcp
reports post_route
report_route_status -file post_route_route_status.rpt
report_drc -file post_route_drc.rpt
set setup_slack [get_property SLACK [get_timing_paths -delay_type max -max_paths 1]]
set hold_slack [get_property SLACK [get_timing_paths -delay_type min -max_paths 1]]
set met [expr {$setup_slack >= 0 && $hold_slack >= 0}]
set f [open timing.json w]
puts $f "{\"part\":\"$part\",\"clock_mhz\":100,\"wns_ns\":$setup_slack,\"whs_ns\":$hold_slack,\"met_100mhz\":[expr {$met ? {true} : {false}}]}"
close $f
