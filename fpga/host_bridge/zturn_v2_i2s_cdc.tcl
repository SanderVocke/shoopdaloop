# These names are checked after the block design and RTL have been synthesized.
set audio_meta [get_cells -quiet -hier -regexp {.*audio/inst/io/((rx_request|rx_ack|tx_request|tx_ack|enable|fault|audio_fault)_meta_reg(\[.*\])?|audio_reset_sync_reg\[0\])}]
if {[llength $audio_meta] < 9} {error "Missing I2S first-stage CDC registers"}
set_false_path -to [get_pins -of_objects $audio_meta -filter {REF_PIN_NAME == D}]

source [file join [file dirname [info script]] zturn_v2_i2s_paths.tcl]
foreach group [zturn_v2_i2s_paths] {
    lassign $group name launch capture budget
    set_max_delay -datapath_only -from $launch -to $capture $budget
    set_bus_skew -from $launch -to $capture $budget
}
