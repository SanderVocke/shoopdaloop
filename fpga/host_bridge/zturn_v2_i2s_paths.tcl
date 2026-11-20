proc zturn_v2_i2s_paths {} {
    set rx_data [get_cells -quiet -hier -regexp -filter {IS_SEQUENTIAL == 1} {.*audio/inst/io/(rx_sequence|rx_epoch|audio_reason|audio_failure_sequence)_reg.*}]
    set tx_data [get_cells -quiet -hier -regexp -filter {IS_SEQUENTIAL == 1} {.*audio/inst/io/(tx_sequence|tx_epoch)_reg.*}]
    set core_dest [get_cells -quiet -hier -regexp -filter {IS_SEQUENTIAL == 1} {.*audio/inst/io/(busy|watchdog|core_fault|recovery_reason|failure_sequence)_reg.*}]
    set audio_dest [get_cells -quiet -hier -regexp -filter {IS_SEQUENTIAL == 1} {.*audio/inst/io/(selected_valid|selected_bank|played_quanta|missed_deadlines|discarded_quanta|tx_ack)_reg.*}]
    if {![llength $rx_data] || ![llength $tx_data] || ![llength $core_dest] || ![llength $audio_dest]} {
        error "Missing I2S bundled-data CDC register collection"
    }
    set groups [list [list adc_bank $rx_data $core_dest 10] [list dac_bank $tx_data $audio_dest 81.380]]

    set gray_source [get_cells -quiet -hier -regexp {.*audio/inst/missed_gray_reg\[.*\]}]
    set gray_target [get_cells -quiet -hier -regexp {.*audio/inst/missed_meta_reg\[.*\]}]
    if {[llength $gray_source] || [llength $gray_target]} {
        if {[llength $gray_source] != 32 || [llength $gray_target] != 32} {
            error "Incomplete I2S xrun counter synchronizer"
        }
        lappend groups [list xrun_gray $gray_source $gray_target 81.380]
    }
    return $groups
}
