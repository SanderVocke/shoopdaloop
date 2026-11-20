if {[version -short] ne {2021.1}} {error "This experiment requires Vivado 2021.1"}
set_param general.maxThreads 2
create_project -in_memory -part xc7z020clg484-1
set includes {}
set sources {}
foreach library {state_processor job_processor chunk_memory media_store axi_memory midi} {
    set directory source/fpga/$library/rtl
    if {![file isdirectory $directory]} {continue}
    lappend includes [file normalize $directory]
    foreach source [lsort [glob $directory/*.sv]] {lappend sources $source}
}
set_property include_dirs $includes [current_fileset]
lappend sources context.sv
read_verilog -sv $sources
read_xdc context.xdc
set f [open setup.txt w]
puts $f "Vivado: [version]\nPart: xc7z020clg484-1\nThreads: 2\nUncertainty: 0.2 ns\nFlow: OOC, rebuilt hierarchy, default placement/routing, post-place phys_opt"
close $f

synth_design -top session_context -part xc7z020clg484-1 -mode out_of_context -flatten_hierarchy rebuilt
opt_design
write_checkpoint -force post_opt.dcp
report_utilization -file post_opt_utilization.rpt
report_utilization -hierarchical -hierarchical_depth 12 -file post_opt_hierarchical_utilization.rpt
report_timing_summary -delay_type min_max -report_unconstrained -file post_opt_timing_summary.rpt
report_timing -delay_type max -max_paths 10 -path_type full_clock_expanded -file post_opt_critical_paths.rpt
set f [open outcome.txt w]
puts $f "stage=opt\nstatus=0"
close $f
exit 0
