# These names are checked after the block design and RTL have been synthesized.
set audio_meta [get_cells -quiet -hier -regexp {.*audio/inst/io/((rx_request|rx_ack|tx_request|tx_ack|enable|fault|audio_fault)_meta_reg(\[.*\])?|audio_reset_sync_reg\[0\])}]
if {[llength $audio_meta] < 9} {error "Missing I2S first-stage CDC registers"}
set_false_path -to [get_pins -of_objects $audio_meta -filter {REF_PIN_NAME == D}]

set to_core [get_cells -quiet -hier -regexp -filter {IS_SEQUENTIAL == 1} {.*audio/inst/io/(rx_sequence|rx_epoch|audio_reason|audio_failure_sequence)_reg.*}]
set to_audio [get_cells -quiet -hier -regexp -filter {IS_SEQUENTIAL == 1} {.*audio/inst/io/(tx_sequence|tx_epoch)_reg.*}]
set core_dest [get_cells -quiet -hier -regexp -filter {IS_SEQUENTIAL == 1} {.*audio/inst/io/(busy|watchdog|core_fault|recovery_reason|failure_sequence)_reg.*}]
set audio_dest [get_cells -quiet -hier -regexp -filter {IS_SEQUENTIAL == 1} {.*audio/inst/io/(selected_valid|selected_bank|played_quanta|missed_deadlines|discarded_quanta|tx_ack)_reg.*}]
if {![llength $to_core] || ![llength $to_audio] || ![llength $core_dest] || ![llength $audio_dest]} {
    error "Missing I2S bundled-data CDC register collection"
}
set_max_delay -datapath_only -from $to_core -to $core_dest 10
set_max_delay -datapath_only -from $to_audio -to $audio_dest 81.380
set_bus_skew -from $to_core -to $core_dest 10
set_bus_skew -from $to_audio -to $audio_dest 81.380

set missed_source [get_cells -quiet -hier -regexp {.*audio/inst/missed_gray_reg\[.*\]}]
set missed_target [get_cells -quiet -hier -regexp {.*audio/inst/missed_meta_reg\[.*\]}]
if {[llength $missed_source] || [llength $missed_target]} {
    if {[llength $missed_source] != 32 || [llength $missed_target] != 32} {
        error "Incomplete I2S xrun counter synchronizer"
    }
    set_max_delay -datapath_only -from $missed_source -to $missed_target 81.380
    set_bus_skew -from $missed_source -to $missed_target 81.380
}
