if {[version -short] ne {2021.1}} {error "Requires Vivado 2021.1"}
set_param general.maxThreads 2
open_checkpoint ../shared-final-100/post_opt.dcp
set cell [get_cells -quiet pipeline/reset_reg]
if {[llength $cell] != 1} {error "Missing measured wrapper reset register"}
set_property DONT_TOUCH true $cell
set net [get_nets -segments -of_objects [get_pins -of_objects $cell -filter {REF_PIN_NAME == Q}]]
set_property DONT_TOUCH true $net
set status [catch {place_design} message]
set f [open outcome.txt w]
puts $f "place_status=$status\nmessage=$message"
close $f
report_control_sets -verbose -file control-sets.rpt
report_utilization -file utilization.rpt
report_utilization -hierarchical -hierarchical_depth 12 -file hierarchy.rpt
report_timing_summary -delay_type min_max -report_unconstrained -file timing.rpt
if {!$status} {write_checkpoint -force post_place.dcp}
exit 0
