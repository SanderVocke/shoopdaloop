set_param general.maxThreads 4
if {[version -short] ne "2021.1"} {error "Vivado 2021.1 required"}
open_checkpoint /input/component-synthesis.dcp
opt_design
report_utilization -hierarchical -file component-optimized-utilization.rpt
report_timing_summary -file component-optimized-timing.rpt
write_checkpoint -force component-optimized.dcp
set done [open complete.txt w]; puts $done passed; close $done
