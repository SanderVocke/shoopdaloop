if {[version -short] ne {2021.1}} {error "This experiment requires Vivado 2021.1"}
set_param general.maxThreads 2
proc checkpoint {path} {
    if {[info exists ::env(MIDI_SAVE_CHECKPOINTS)] && $::env(MIDI_SAVE_CHECKPOINTS) eq {1}} {write_checkpoint -force $path}
}
proc reports {stage} {
    report_utilization -file ${stage}_utilization.rpt
    report_utilization -hierarchical -hierarchical_depth 8 -file ${stage}_hierarchical_utilization.rpt
    foreach {component original} {
        pipeline midi_audio_pipeline_axi
        audio media_audio_pipeline_axi
        state state_processor
        jobs job_processor
        midi midi_data_jobs
        midi_store midi_store
    } {
        # Rebuilt hierarchy may insert generated wrappers between public tops.
        set cells [get_cells -quiet -hierarchical -filter "ORIG_REF_NAME == $original"]
        if {[llength $cells]} {report_utilization -cells $cells -file ${stage}_${component}_utilization.rpt}
    }
    report_timing_summary -delay_type min_max -report_unconstrained -check_timing_verbose -max_paths 20 -file ${stage}_timing_summary.rpt
    report_timing -delay_type max -max_paths 20 -path_type full_clock_expanded -file ${stage}_critical_paths.rpt
    set midi_roots [get_cells -quiet -hierarchical -filter {ORIG_REF_NAME == midi_data_jobs}]
    foreach root $midi_roots {
        set registers [get_cells -quiet -hierarchical -filter "IS_SEQUENTIAL && NAME =~ $root/*"]
        if {![llength $registers]} {error "No MIDI sequential cells found below $root"}
        report_timing -from $registers -delay_type max -max_paths 10 -path_type full_clock_expanded -file ${stage}_midi_from_paths.rpt
        report_timing -to $registers -delay_type max -max_paths 10 -path_type full_clock_expanded -file ${stage}_midi_to_paths.rpt
        set from_path [get_timing_paths -from $registers -delay_type max -max_paths 1]
        set to_path [get_timing_paths -to $registers -delay_type max -max_paths 1]
        set midi_file [open ${stage}_midi_timing.json w]
        puts $midi_file "{\"from_midi_wns_ns\":[get_property SLACK $from_path],\"to_midi_wns_ns\":[get_property SLACK $to_path]}"
        close $midi_file
    }
    report_control_sets -verbose -file ${stage}_control_sets.rpt
    check_timing -verbose -file ${stage}_check_timing.rpt
    set setup_paths [get_timing_paths -delay_type max -max_paths 1]
    set hold_paths [get_timing_paths -delay_type min -max_paths 1]
    set f [open ${stage}.json w]
    puts $f "{\"wns_ns\":[get_property SLACK $setup_paths],\"whs_ns\":[get_property SLACK $hold_paths]}"
    close $f
}
set stage opt
set status [catch {
    open_checkpoint input.dcp
    set stage opt
    opt_design
    reports post_opt
    checkpoint post_opt.dcp
    set stage place
    place_design
    phys_opt_design
    checkpoint post_place.dcp
    reports post_place
    set stage route
    route_design
    checkpoint post_route.dcp
    reports post_route
    report_route_status -file post_route_route_status.rpt
    report_drc -file post_route_drc.rpt
} message options]
set f [open outcome.txt w]
puts $f "stage=$stage\nstatus=$status\nmessage=$message"
close $f
if {$status} {
    puts stderr [dict get $options -errorinfo]
    exit 1
}
exit 0
