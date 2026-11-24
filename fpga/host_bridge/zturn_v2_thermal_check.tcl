set thermal_monitors [get_cells -hierarchical -filter {REF_NAME == XADC}]
if {[llength $thermal_monitors] != 1} {error "Expected exactly one autonomous XADC thermal monitor"}
set thermal_shutdown [get_property BITSTREAM.CONFIG.OVERTEMPPOWERDOWN [current_design]]
if {![string equal -nocase $thermal_shutdown ENABLE]} {error "Automatic PL thermal shutdown is disabled"}
foreach {name expected} {INIT_40 0000 INIT_41 0000 INIT_42 0800 INIT_53 B0D3 INIT_57 ABC4} {
    set raw [get_property $name $thermal_monitors]
    if {![regexp -nocase {^(16'h|0x)?([0-9a-f]{4})$} $raw _ prefix digits] ||
        ![string equal -nocase $digits $expected]} {
        error "Unsafe XADC thermal configuration: $name=$raw, expected $expected"
    }
}
set thermal_report [open thermal.rpt w]
puts $thermal_report "Automatic PL overtemperature shutdown: $thermal_shutdown"
puts $thermal_report "Nominal OT trip: 75 C; nominal lower threshold: 65 C"
puts $thermal_report "Scope: PL only; does not power off the PS or board"
puts $thermal_report "XADC: $thermal_monitors"
foreach name {INIT_40 INIT_41 INIT_42 INIT_53 INIT_57} {
    puts $thermal_report "$name: [get_property $name $thermal_monitors]"
}
close $thermal_report
