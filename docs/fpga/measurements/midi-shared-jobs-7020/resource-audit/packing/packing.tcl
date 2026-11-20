if {[version -short] ne {2021.1}} {error "Requires Vivado 2021.1"}
set_param general.maxThreads 2
open_checkpoint ../shared-final-100/post_opt.dcp
set sample [lindex [get_cells -hier -filter {REF_NAME == LUT5}] 0]
set f [open lut-properties.rpt w]
foreach p [list_property $sample] {puts $f "$p\t[get_property $p $sample]"}
close $f
set f [open lut-sites.tsv w]
puts $f "name\tref\tLUTNM\tHLUTNM\tSOFT_HLUTNM"
set properties [list_property $sample]
foreach c [get_cells -hier -filter {REF_NAME =~ LUT*}] {
    set values {}
    foreach p {LUTNM HLUTNM SOFT_HLUTNM} {
        if {[lsearch -exact $properties $p] >= 0} {lappend values [get_property $p $c]} else {lappend values {}}
    }
    puts $f "[get_property NAME $c]\t[get_property REF_NAME $c]\t[join $values \t]"
}
close $f
set status [catch {place_design} message]
set f [open outcome.txt w]
puts $f "place_status=$status\nmessage=$message"
close $f
report_control_sets -verbose -file failed-place-control-sets.rpt
report_utilization -file failed-place-utilization.rpt
report_utilization -hierarchical -hierarchical_depth 12 -file failed-place-hierarchy.rpt
set f [open cells.tsv w]
puts $f "name\tref"
foreach c [get_cells -hier -filter {IS_PRIMITIVE == 1}] {puts $f "[get_property NAME $c]\t[get_property REF_NAME $c]"}
close $f
exit 0
