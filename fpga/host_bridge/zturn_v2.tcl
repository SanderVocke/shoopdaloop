if {$argc ni {3 4 5}} {
    error "usage: vivado -mode batch -source fpga/host_bridge/zturn_v2.tcl -tclargs OUTPUT PS_PROPERTIES {generate|bitstream} ?{diagnostic|looping|native}? ?{generated|i2s}?"
}
set root [file normalize [file join [file dirname [info script]] ../..]]
set output [file normalize [lindex $argv 0]]
set ps_properties [file normalize [lindex $argv 1]]
set stage [lindex $argv 2]
set geometry [expr {$argc >= 4 ? [lindex $argv 3] : "diagnostic"}]
set audio_mode [expr {$argc == 5 ? [lindex $argv 4] : "generated"}]
set serial_synth [expr {[info exists ::env(SHOOP_VIVADO_SERIAL_SYNTH)] && $::env(SHOOP_VIVADO_SERIAL_SYNTH) eq "1"}]
if {$stage ni {generate bitstream}} {error "Unknown board build stage $stage"}
if {$geometry ni {diagnostic looping native}} {error "Unknown bridge geometry $geometry"}
if {$audio_mode ni {generated i2s}} {error "Unknown audio mode $audio_mode"}
set native_pcm [expr {$geometry eq "native"}]
if {$native_pcm && $audio_mode ne "i2s"} {error "Native audio requires the I2S adapter"}
set channel [open $ps_properties r]
set preset [read $channel]
close $channel
if {[dict size $preset] == 0} {error "Supply the MYIR V2 PS7 configuration"}
foreach name [dict keys $preset] {
    if {![string match CONFIG.PCW_* $name]} {error "Invalid PS7 property: $name"}
}
file mkdir $output
file copy -force $ps_properties [file join $output ps-properties.txt]
set abi [expr {$native_pcm ? "native_audio.json" : "registers.json"}]
file copy -force [file join $root fpga host_bridge $abi] [file join $output fpga-abi.json]
set channel [open [file join $output geometry.txt] w]
puts $channel $geometry
close $channel
set channel [open [file join $output audio.txt] w]
puts $channel $audio_mode
close $channel
set profile [file join $output board]
exec python3 [file join $root platforms zynq7000 boards.py] --board zturn-v2 --output $profile
set channel [open [file join $profile shoop_board.h] r]
set board_header [read $channel]
close $channel
foreach {macro variable} {DDR_BASE arena_base DDR_BYTES arena_bytes REG_BASE register_base CLOCK_HZ clock_hz} {
    if {![regexp "SHOOP_PL_$macro (0x\[0-9a-f\]+)u" $board_header _ $variable]} {
        error "Missing PL profile setting SHOOP_PL_$macro"
    }
}

cd $output
create_project zturn_v2 [file join $output project] -part xc7z020clg400-2 -force
if {[info exists ::env(SHOOP_IP_REPO)]} {
    set_property ip_repo_paths [list [file normalize $::env(SHOOP_IP_REPO)]] [current_project]
    update_ip_catalog
}
foreach ip {processing_system7:5.5 proc_sys_reset:5.0 xlconstant:1.1 axi_interconnect:2.1} {
    if {![llength [get_ipdefs -all -quiet xilinx.com:ip:$ip]]} {error "Missing Vivado catalog IP $ip"}
}
set host [file join $root fpga host_bridge rtl]
if {$native_pcm} {
    add_files [list [file join $host native_audio_registers.sv] [file join $host native_audio_mmio.sv] \
        [file join $host native_audio_bd.v]]
} else {
    add_files [file join $host host_registers.sv]
    add_files [lsort [glob [file join $root fpga state_processor rtl *.svh]]]
    foreach directory {state_processor job_processor chunk_memory media_store axi_memory} {
        add_files [lsort [glob [file join $root fpga $directory rtl *.sv]]]
    }
    foreach source {host_mailbox.sv host_dispatch.sv host_pipeline_axi.sv host_pipeline_bd.v host_generated_audio.v} {
        add_files [file join $host $source]
    }
}
foreach source {audio_lane_ram.sv audio_io.sv} {
    add_files [file join $root fpga audio_io rtl $source]
}
if {$audio_mode eq "i2s"} {
    add_files [file join $root fpga audio_io rtl i2s_duplex.sv]
    add_files [file join $host host_i2s_audio.v]
    add_files -fileset constrs_1 [file join $root fpga host_bridge zturn_v2_i2s.xdc]
}
set_property include_dirs [list [file join $root fpga state_processor rtl]] [current_fileset]
update_compile_order -fileset sources_1

create_bd_design system
set ps [create_bd_cell -type ip -vlnv xilinx.com:ip:processing_system7:5.5 ps]
set_property -dict $preset $ps
set ddr_high [get_property CONFIG.PCW_DDR_RAM_HIGHADDR $ps]
if {$arena_base + $arena_bytes - 1 > $ddr_high} {
    error "The PS DDR window ending at $ddr_high excludes the PL arena"
}
set_property -dict [list CONFIG.PCW_USE_M_AXI_GP0 1 CONFIG.PCW_USE_S_AXI_HP0 [expr {!$native_pcm}] \
    CONFIG.PCW_S_AXI_HP0_DATA_WIDTH 64 CONFIG.PCW_EN_CLK0_PORT 1 CONFIG.PCW_EN_RST0_PORT 1 \
    CONFIG.PCW_FPGA0_PERIPHERAL_FREQMHZ [expr {$clock_hz/1000000.0}] CONFIG.PCW_USE_FABRIC_INTERRUPT 1 \
    CONFIG.PCW_IRQ_F2P_INTR 1] $ps
make_bd_intf_pins_external [get_bd_intf_pins ps/DDR] [get_bd_intf_pins ps/FIXED_IO]

if {$native_pcm} {
    set core [create_bd_cell -type module -reference native_audio_bd core]
    set_property CONFIG.CORE_CLOCK_HZ [expr {$clock_hz}] $core
} else {
    set core [create_bd_cell -type module -reference host_pipeline_bd core]
    set_property -dict [list CONFIG.ARENA_BASE [expr {$arena_base}] \
        CONFIG.ARENA_BYTES [expr {$arena_bytes}] CONFIG.CORE_CLOCK_HZ [expr {$clock_hz}] \
        CONFIG.EXTERNAL_IO 1 CONFIG.INPUT_PORTS 1] $core
}
if {$geometry eq "looping"} {
    # Match the production image packer and leave space for media metadata in HP0 DDR.
    set_property -dict [list CONFIG.PROGRAM_DEPTH 1024 CONFIG.STATE_DEPTH 512 CONFIG.JOB_DEPTH 1024 \
        CONFIG.BUFFERS 512 CONFIG.VIEWS 512 CONFIG.CHUNK_FRAMES 32 CONFIG.BLOCK_CHUNKS 32 \
        CONFIG.SLOTS [expr {31*1024*1024/32}]] $core
}
set audio_module [expr {$audio_mode eq "i2s" ? "host_i2s_audio" : "host_generated_audio"}]
set audio [create_bd_cell -type module -reference $audio_module audio]
set_property CONFIG.CORE_CLOCK_HZ [expr {$clock_hz}] $audio
if {$audio_mode eq "i2s"} {
    foreach {name direction type} {audio_ref I clk i2s_adc_data I data i2s_bclk O data i2s_lrclk O data i2s_dac_data O data} {
        set port [create_bd_port -dir $direction -type $type $name]
        if {$type eq "clk"} {set_property CONFIG.FREQ_HZ 12288000 $port}
        connect_bd_net $port [get_bd_pins audio/$name]
    }
}
create_bd_cell -type ip -vlnv xilinx.com:ip:proc_sys_reset:5.0 reset
set_property CONFIG.C_AUX_RESET_HIGH 1 [get_bd_cells reset]
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_interconnect:2.1 gp0
set_property CONFIG.NUM_MI 1 [get_bd_cells gp0]
foreach {name width value} {zero 1 0 one 1 1 boot_epoch 32 1} {
    set constant [create_bd_cell -type ip -vlnv xilinx.com:ip:xlconstant:1.1 $name]
    set_property -dict [list CONFIG.CONST_WIDTH $width CONFIG.CONST_VAL $value] $constant
}

connect_bd_net [get_bd_pins ps/FCLK_CLK0] \
    [get_bd_pins ps/M_AXI_GP0_ACLK] [get_bd_pins gp0/ACLK] [get_bd_pins gp0/S00_ACLK] \
    [get_bd_pins gp0/M00_ACLK] [get_bd_pins core/clk] [get_bd_pins audio/clk] [get_bd_pins reset/slowest_sync_clk]
connect_bd_net [get_bd_pins ps/FCLK_RESET0_N] [get_bd_pins reset/ext_reset_in]
connect_bd_net [get_bd_pins reset/interconnect_aresetn] [get_bd_pins gp0/ARESETN]
connect_bd_net [get_bd_pins reset/peripheral_aresetn] [get_bd_pins gp0/S00_ARESETN] [get_bd_pins gp0/M00_ARESETN]
connect_bd_net [get_bd_pins reset/peripheral_reset] [get_bd_pins core/reset] [get_bd_pins audio/reset]
connect_bd_net [get_bd_pins zero/dout] [get_bd_pins reset/mb_debug_sys_rst] [get_bd_pins reset/aux_reset_in]
connect_bd_net [get_bd_pins one/dout] [get_bd_pins reset/dcm_locked]
connect_bd_net [get_bd_pins boot_epoch/dout] [get_bd_pins audio/epoch]
connect_bd_net [get_bd_pins core/irq] [get_bd_pins ps/IRQ_F2P]
connect_bd_net [get_bd_pins core/audio_enable] [get_bd_pins audio/enable]
connect_bd_net [get_bd_pins audio/missed_deadlines] [get_bd_pins core/audio_xruns]
foreach name {audio_fault quantum_valid quantum_ready quantum_done sample_input_read sample_input_address
    sample_input_data sample_output_write sample_output_address sample_output_data} {
    connect_bd_net [get_bd_pins core/$name] [get_bd_pins audio/$name]
}

connect_bd_intf_net [get_bd_intf_pins ps/M_AXI_GP0] [get_bd_intf_pins gp0/S00_AXI]
connect_bd_intf_net [get_bd_intf_pins gp0/M00_AXI] [get_bd_intf_pins core/s_axi]
if {$native_pcm} {
    connect_bd_net [get_bd_pins zero/dout] [get_bd_pins audio/process_fault]
} else {
    connect_bd_net [get_bd_pins ps/FCLK_CLK0] [get_bd_pins ps/S_AXI_HP0_ACLK]
    connect_bd_net [get_bd_pins zero/dout] [get_bd_pins ps/S_AXI_HP0_RDISSUECAP1_EN] [get_bd_pins ps/S_AXI_HP0_WRISSUECAP1_EN]
    connect_bd_net [get_bd_pins boot_epoch/dout] [get_bd_pins core/epoch]
    connect_bd_net [get_bd_pins core/fault] [get_bd_pins audio/process_fault]
    connect_bd_intf_net [get_bd_intf_pins core/m_axi] [get_bd_intf_pins ps/S_AXI_HP0]
}
assign_bd_address -offset $register_base -range 4K -target_address_space [get_bd_addr_spaces ps/Data] \
    [get_bd_addr_segs core/s_axi/reg0]
assign_bd_address
validate_bd_design
if {[get_property CONFIG.C_EXT_RESET_HIGH [get_bd_cells reset]] != 0} {
    error "The PS clock reset must propagate as active low"
}
save_bd_design

set bd [get_files */system.bd]
if {$serial_synth} {
    # Apply the synthesis settings in the project run instead of hidden IP runs.
    set_property synth_checkpoint_mode None $bd
}
generate_target all $bd
add_files [make_wrapper -files $bd -top]
set_property top system_wrapper [current_fileset]
update_compile_order -fileset sources_1
set scripts [glob -nocomplain [file join $output project zturn_v2.gen sources_1 bd system ip * ps7_init.tcl]]
if {[llength $scripts] != 1} {error "Could not identify the generated PS initialization script"}
file copy -force [lindex $scripts 0] [file join $output ps7_init.tcl]
if {$stage eq "generate"} {return}

if {$serial_synth} {
    set hook [file join $output serial_synth.tcl]
    set channel [open $hook w]
    puts $channel {set_param synth.enableParallelHelperSpawn none}
    puts $channel {set_param synth.maxThreads 1}
    close $channel
    add_files -fileset utils_1 -norecurse $hook
    set_property STEPS.SYNTH_DESIGN.TCL.PRE $hook [get_runs synth_1]
}
if {$audio_mode eq "i2s"} {
    set hook [file join $root fpga host_bridge zturn_v2_i2s_cdc.tcl]
    add_files -fileset utils_1 -norecurse $hook
    set_property STEPS.OPT_DESIGN.TCL.PRE $hook [get_runs impl_1]
    # In 2021.1 the full report can stall on independently clocked wide buses.
    set skew_report [get_report_configs -quiet impl_1_route_report_bus_skew_0]
    if {[llength $skew_report]} {delete_report_configs $skew_report}
}
launch_runs impl_1 -to_step write_bitstream -jobs 4
wait_on_run impl_1
if {[get_property PROGRESS [get_runs impl_1]] ne "100%"} {error "Vivado implementation failed"}
open_run impl_1
report_timing_summary -file [file join $output timing.rpt]
report_route_status -file [file join $output route_status.rpt]
report_cdc -details -file [file join $output cdc.rpt]
report_utilization -hierarchical -file [file join $output utilization.rpt]
report_drc -file [file join $output drc.rpt]
if {[get_property SLACK [get_timing_paths -delay_type max -max_paths 1]] < 0 ||
    [get_property SLACK [get_timing_paths -delay_type min -max_paths 1]] < 0} {
    error "Integrated Z-turn V2 design missed timing. See timing.rpt"
}
file copy -force [file join $output project zturn_v2.runs impl_1 system_wrapper.bit] [file join $output zturn_v2.bit]
write_hw_platform -fixed -include_bit -force -file [file join $output zturn_v2.xsa]
exec python3 [file join $root platforms zynq7000 fpga_pair.py] stamp $output [version -short]
if {$audio_mode eq "i2s" && [info exists ::env(SHOOP_VIVADO_FULL_SKEW)] && $::env(SHOOP_VIVADO_FULL_SKEW) eq "1"} {
    report_bus_skew -file [file join $output bus_skew.rpt]
}
