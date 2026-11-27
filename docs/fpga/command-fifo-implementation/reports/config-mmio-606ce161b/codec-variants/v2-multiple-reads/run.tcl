set_param general.maxThreads 4
if {[version -short] ne "2021.1"} { error "Vivado 2021.1 required" }
create_project -in_memory -part xc7z020clg400-2
read_verilog -sv /source/fifo_config_service.sv
synth_design -mode out_of_context -flatten_hierarchy none -top fifo_config_service -part xc7z020clg400-2
create_clock -name core -period 10 [get_ports clk]
set_input_delay -clock core 1 [get_ports -filter {DIRECTION == IN && NAME != clk}]
set_output_delay -clock core 1 [all_outputs]
report_utilization -hierarchical -file codec-utilization.rpt
report_timing_summary -file codec-synthesis-timing.rpt
write_checkpoint -force codec-synthesis.dcp
opt_design
report_utilization -hierarchical -file codec-optimized-utilization.rpt
report_timing_summary -file codec-optimized-timing.rpt
write_checkpoint -force codec-optimized.dcp
set done [open complete.txt w]; puts $done passed; close $done
