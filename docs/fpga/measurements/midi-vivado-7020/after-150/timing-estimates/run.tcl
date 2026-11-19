if {[version -short] ne {2021.1}} {error "This experiment requires Vivado 2021.1"}
set_param general.maxThreads 2
open_checkpoint [lindex $argv 0]
foreach component {audio midi} {
    set registers [get_cells -hierarchical -filter [format {IS_SEQUENTIAL && NAME =~ %s/*} $component]]
    if {![llength $registers]} {error "No sequential cells found for $component"}
    report_timing -from $registers -to $registers -delay_type max -max_paths 20 -path_type full_clock_expanded -file ${component}_critical_paths.rpt
    report_timing -from $registers -to $registers -delay_type min -max_paths 10 -file ${component}_hold_paths.rpt
    set setup_path [get_timing_paths -from $registers -to $registers -delay_type max -max_paths 1]
    set hold_path [get_timing_paths -from $registers -to $registers -delay_type min -max_paths 1]
    set f [open ${component}_timing.json w]
    puts $f "{\"wns_ns\":[get_property SLACK $setup_path],\"whs_ns\":[get_property SLACK $hold_path]}"
    close $f
}
exit 0
