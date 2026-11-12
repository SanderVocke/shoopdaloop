source /workspace/shoopdaloop/work/zedboard/build2021/manifest.tcl
cd /workspace/shoopdaloop/work/zedboard/core2021
set_param general.maxThreads 4
create_project -in_memory -part xc7z020clg484-1
set_property include_dirs [list [file join $bench_root fpga state_processor rtl] [file join $bench_root fpga zedboard rtl]] [current_fileset]
read_verilog -sv $bench_sources
read_xdc /checks/core.xdc
synth_design -top zedboard_bench -part xc7z020clg484-1 -mode out_of_context
write_checkpoint -force post_synth.dcp
opt_design
place_design
phys_opt_design
route_design
write_checkpoint -force post_route.dcp
report_route_status -file route_status.rpt
report_drc -file drc.rpt
puts "BENCHMARK_RTL_BUILD_COMPLETE"
exit
