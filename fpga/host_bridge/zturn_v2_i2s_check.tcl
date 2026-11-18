if {$argc != 1} {
    error "usage: vivado -mode batch -source zturn_v2_i2s_check.tcl -tclargs BUILD_DIRECTORY"
}
source [file join [file dirname [info script]] zturn_v2_i2s_paths.tcl]
set build [file normalize [lindex $argv 0]]
set evidence [file join $build skew-signoff]
file mkdir $evidence
open_checkpoint [file join $build project zturn_v2.runs impl_1 system_wrapper_routed.dcp]
set groups [zturn_v2_i2s_paths]

foreach group $groups {
    lassign $group name launch capture budget
    set path [get_timing_paths -from $launch -to $capture -delay_type max -max_paths 1]
    if {[llength $path] != 1 || [get_property EXCEPTION $path] ne [format "MaxDelay Path %.3fns -datapath_only" $budget]} {
        error "Missing routed datapath-only constraint for $name"
    }
    set max_delay_slack($name) [get_property SLACK $path]
    if {$max_delay_slack($name) < 0} {error "Routed $name max-delay check failed"}
}

# Datapath-only exceptions omit clock insertion delay from the path objects.
# Restore these delays in memory to bound the capture-relative arrival range.
set original [file join $evidence timing-original.xdc]
set analysis [file join $evidence timing-with-clock-paths.xdc]
write_xdc -type timing -force $original
set input [open $original r]
set output [open $analysis w]
set command ""
while {[gets $input line] >= 0} {
    append command $line "\n"
    if {[info complete $command]} {
        if {![regexp {^\s*set_max_delay\M} $command]} {puts -nonewline $output $command}
        set command ""
    }
}
close $input
close $output
if {[string trim $command] ne ""} {error "Incomplete generated XDC"}
reset_timing
read_xdc $analysis

proc zturn_v2_i2s_arrivals {launch capture delay} {
    set limit 20000
    set per_sink_limit 1000
    set paths [get_timing_paths -from $launch -to $capture -delay_type $delay \
        -max_paths $limit -nworst $per_sink_limit -unique_pins -no_report_unconstrained]
    if {![llength $paths] || [llength $paths] >= $limit} {error "Incomplete $delay path collection"}
    set lower Inf
    set upper -Inf
    set pairs {}
    set clocks {}
    set per_sink {}
    foreach path $paths {
        set sink [get_property ENDPOINT_PIN $path]
        if {$sink eq ""} {error "Missing path endpoint"}
        dict incr per_sink $sink
        set phase [expr {[get_property STARTPOINT_CLOCK_DELAY $path] +
            [get_property DATAPATH_DELAY $path] - [get_property ENDPOINT_CLOCK_DELAY $path]}]
        set uncertainty [get_property UNCERTAINTY $path]
        set lower [expr {min($lower, $phase - $uncertainty)}]
        set upper [expr {max($upper, $phase + $uncertainty)}]
        lappend pairs [list [get_property STARTPOINT_PIN $path] $sink]
        lappend clocks [list [get_property STARTPOINT_CLOCK $path] [get_property ENDPOINT_CLOCK $path]]
    }
    dict for {sink count} $per_sink {
        if {$count >= $per_sink_limit} {error "Clipped $delay paths at $sink"}
    }
    set clocks [lsort -unique $clocks]
    if {[llength $clocks] != 1 || [lindex $clocks 0 0] eq [lindex $clocks 0 1]} {
        error "Unexpected clock domains in $delay path collection"
    }
    return [list $lower $upper $pairs [lindex $clocks 0]]
}

set report [open [file join $evidence arrival-bounds.rpt] w]
puts $report "Routed arrival bounds with clock insertion and uncertainty across both timing corners"
puts $report [format "%-12s %10s %16s %16s %16s" Group Paths {Max-delay slack} {Arrival span} {Skew budget}]
foreach group $groups {
    lassign $group name launch capture budget
    lassign [zturn_v2_i2s_arrivals $launch $capture min] early_min late_min min_paths min_clocks
    lassign [zturn_v2_i2s_arrivals $launch $capture max] early_max late_max max_paths max_clocks
    if {$min_clocks ne $max_clocks || [lsort $min_paths] ne [lsort $max_paths]} {
        error "Different routed path coverage across timing corners for $name"
    }
    set span [expr {max($late_max, $late_min) - min($early_min, $early_max)}]
    puts $report [format "%-12s %10d %13.3f ns %13.3f ns %13.3f ns" \
        $name [llength $max_paths] $max_delay_slack($name) $span $budget]
    flush $report
    if {$span > $budget} {error "Routed $name arrival span exceeds its skew budget"}
}
close $report
puts "I2S arrival bounds meet the skew budgets"
