if {[version -short] ne {2021.1}} {error "Requires Vivado 2021.1"}
set_param general.maxThreads 2
open_checkpoint ../shared-final-100/post_opt.dcp
set all [get_cells -hierarchical -filter {IS_PRIMITIVE == 1}]
set file [open cells.tsv w]
puts $file "name\tref"
foreach cell $all { puts $file "[get_property NAME $cell]\t[get_property REF_NAME $cell]" }
close $file
set service [get_cells -hierarchical -filter {ORIG_REF_NAME == midi_job_service}]
set prefix [get_property NAME $service]
puts "MIDI service prefix: $prefix"
set cells [get_cells -hierarchical -filter "IS_PRIMITIVE == 1 && NAME =~ $prefix/*"]
set pinsfile [open midi-pins.tsv w]
puts $pinsfile "cell\tpin\tdirection\tnet"
foreach cell $cells {
    foreach pin [get_pins -quiet -of_objects $cell] {
        set nets [get_nets -quiet -of_objects $pin]
        puts $pinsfile "[get_property NAME $cell]\t[get_property REF_PIN_NAME $pin]\t[get_property DIRECTION $pin]\t[join $nets ,]"
    }
}
close $pinsfile
report_control_sets -verbose -file control-sets.rpt
report_utilization -cells $service -file midi-service-utilization.rpt
set dspfile [open dsp.tsv w]
puts $dspfile "name\tAREG\tBREG\tCREG\tMREG\tPREG\tUSE_MULT"
foreach cell [get_cells -hierarchical -filter {REF_NAME == DSP48E1}] {
    puts $dspfile "[get_property NAME $cell]\t[get_property AREG $cell]\t[get_property BREG $cell]\t[get_property CREG $cell]\t[get_property MREG $cell]\t[get_property PREG $cell]\t[get_property USE_MULT $cell]"
}
close $dspfile
exit 0
