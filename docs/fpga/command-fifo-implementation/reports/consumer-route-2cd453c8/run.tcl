
set_param general.maxThreads 4
if {[version -short] ne "2021.1"} { error "Vivado 2021.1 required" }
create_project -in_memory -part xc7z020clg400-2
foreach source {fifo_edit_kernel.sv fifo_group_banks.sv fifo_group_consumer.sv} {
    read_verilog -sv /source/$source
}
synth_design -mode out_of_context -flatten_hierarchy none -top fifo_group_consumer -part xc7z020clg400-2 -generic {LOOPS=128 STATES=512 PROGRAM_DEPTH=1024 BUFFERS=512}
create_clock -name core -period 10 [get_ports clk]
set_input_delay -clock core 1 [get_ports -filter {DIRECTION == IN && NAME != clk}]
set_output_delay -clock core 1 [all_outputs]
report_utilization -hierarchical -file consumer-utilization.rpt
report_timing_summary -file consumer-synthesis-timing.rpt
set report [open consumer-primitives.txt w]
foreach {label filter} {LUT {REF_NAME =~ LUT*} FF {REF_NAME =~ FD*} CARRY4 {REF_NAME == CARRY4} RAMB36 {REF_NAME == RAMB36E1} RAMB18 {REF_NAME == RAMB18E1} DSP {REF_NAME == DSP48E1}} {
    puts $report "$label [llength [get_cells -hierarchical -filter $filter]]"
}
puts $report "PORT_BITS [llength [get_ports]]"
close $report
write_checkpoint -force consumer-synthesis.dcp
if {1} {
    opt_design
    place_design
    route_design
    report_utilization -hierarchical -file consumer-routed-utilization.rpt
    report_timing_summary -file consumer-routed-timing.rpt
    report_timing -max_paths 10 -file consumer-paths.rpt
    report_drc -file consumer-drc.rpt
    write_checkpoint -force consumer-routed.dcp
}
set done [open complete.txt w]; puts $done passed; close $done
