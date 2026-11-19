if {[version -short] ne {2021.1}} {error "This experiment requires Vivado 2021.1"}
set_param general.maxThreads 2
foreach mhz {100 150} {
    open_checkpoint ../shared-final-${mhz}/post_opt.dcp
    foreach {component original} {
        pipeline midi_audio_pipeline_axi
        audio media_audio_pipeline_axi
        state state_processor
        jobs job_processor
        midi midi_job_port
        midi_service midi_job_service
    } {
        set cells [get_cells -quiet -hierarchical -filter "ORIG_REF_NAME == $original"]
        puts "Attribution ${mhz} ${component}: [llength $cells] cells"
        if {![llength $cells]} {error "Missing component $original"}
        report_utilization -cells $cells -file ../shared-final-${mhz}/post_opt_${component}_utilization.rpt
    }
    close_design
}
exit 0
