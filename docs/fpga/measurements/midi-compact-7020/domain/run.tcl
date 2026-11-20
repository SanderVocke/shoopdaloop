if {[version -short] ne {2021.1}} {error "This analysis requires Vivado 2021.1"}
if {[llength $argv] == 0 || [llength $argv] % 2} {error "Pass CHECKPOINT OUTPUT_PREFIX pairs"}
set_param general.maxThreads 2
foreach {checkpoint prefix} $argv {
    open_checkpoint $checkpoint
    set roots [get_cells -quiet -hierarchical -filter {ORIG_REF_NAME == midi_job_port}]
    set adapters [get_cells -quiet -hierarchical -filter {ORIG_REF_NAME == word_axi && NAME =~ *midi_memory}]
    if {![llength $roots] || ![llength $adapters]} {error "Missing MIDI service or DDR adapter"}
    set registers {}
    foreach root [concat $roots $adapters] {
        set selected [get_cells -quiet -hierarchical -filter "NAME =~ $root/* && (IS_SEQUENTIAL || REF_NAME =~ RAMB* || REF_NAME =~ DSP*)"]
        set registers [concat $registers $selected]
    }
    set registers [lsort -unique $registers]
    if {![llength $registers]} {error "No clocked MIDI cells found"}
    file mkdir [file dirname $prefix]
    report_timing -from $registers -delay_type max -max_paths 10 -path_type full_clock_expanded -file ${prefix}_from_paths.rpt
    report_timing -to $registers -delay_type max -max_paths 10 -path_type full_clock_expanded -file ${prefix}_to_paths.rpt
    report_timing -from $registers -to $registers -delay_type max -max_paths 10 -path_type full_clock_expanded -file ${prefix}_internal_paths.rpt
    set from [get_timing_paths -from $registers -delay_type max -max_paths 1]
    set to [get_timing_paths -to $registers -delay_type max -max_paths 1]
    set internal [get_timing_paths -from $registers -to $registers -delay_type max -max_paths 1]
    set output [open ${prefix}.json w]
    puts $output "{\"clocked_cells\":[llength $registers],\"from_wns_ns\":[get_property SLACK $from],\"to_wns_ns\":[get_property SLACK $to],\"internal_wns_ns\":[get_property SLACK $internal]}"
    close $output
    close_design
}
exit 0
