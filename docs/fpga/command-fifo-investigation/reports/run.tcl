
set_param general.maxThreads 4
puts "INVESTIGATION_TOOL [version -short]"
foreach {name top loops route} {kernel fifo_edit_kernel 128 1 probe16 fifo_edit_probe 16 0 probe32 fifo_edit_probe 32 0 probe128 fifo_edit_probe 128 1 probe512 fifo_edit_probe 512 0} {
    create_project -in_memory -part xc7z020clg400-2
    read_verilog -sv /source/edit_kernel.sv
    read_verilog -sv /source/probe.sv
    synth_design -mode out_of_context -flatten_hierarchy none -top $top -part xc7z020clg400-2 -generic LOOPS=$loops
    create_clock -name core -period 10 [get_ports clk]
    set_input_delay -clock core 1 [get_ports -filter {DIRECTION == IN && NAME != clk}]
    set_output_delay -clock core 1 [all_outputs]
    report_utilization -hierarchical -file $name-utilization.rpt
    report_timing_summary -file $name-synthesis-timing.rpt
    set report [open $name-primitives.txt w]
    foreach {label filter} {LUT {REF_NAME =~ LUT*} FF {REF_NAME =~ FD*} CARRY4 {REF_NAME == CARRY4} RAMB36 {REF_NAME == RAMB36E1} RAMB18 {REF_NAME == RAMB18E1} DSP {REF_NAME == DSP48E1}} {
        puts $report "$label [llength [get_cells -hierarchical -filter $filter]]"
    }
    puts $report "PORT_BITS [llength [get_ports]]"
    puts $report "SEQUENTIAL [llength [get_cells -hierarchical -filter {IS_SEQUENTIAL == 1}]]"
    close $report
    write_checkpoint -force $name-synthesis.dcp
    if {$route} {
        opt_design
        place_design
        route_design
        report_utilization -hierarchical -file $name-routed-utilization.rpt
        report_timing_summary -file $name-routed-timing.rpt
        report_timing -max_paths 10 -file $name-paths.rpt
        report_drc -file $name-drc.rpt
        write_checkpoint -force $name-routed.dcp
    }
    close_project
}
set done [open complete.txt w]; puts $done passed; close $done
