if {$argc != 3} {
    error "usage: vivado -mode batch -source fpga/host_bridge/zturn_v2.tcl -tclargs OUTPUT PS_PROPERTIES {generate|bitstream}"
}
set root [file normalize [file join [file dirname [info script]] ../..]]
set output [file normalize [lindex $argv 0]]
set ps_properties [file normalize [lindex $argv 1]]
set stage [lindex $argv 2]
if {$stage ni {generate bitstream}} {error "Unknown board build stage $stage"}
set channel [open $ps_properties r]
set preset [read $channel]
close $channel
if {[dict size $preset] == 0} {error "Supply the MYIR V2 PS7 configuration"}
foreach name [dict keys $preset] {
    if {![string match CONFIG.PCW_* $name]} {error "Invalid PS7 property: $name"}
}
file mkdir $output
file copy -force $ps_properties [file join $output ps-properties.txt]
set profile [file join $output board]
exec python3 [file join $root platforms zynq7000 boards.py] --board zturn-v2 --output $profile
set channel [open [file join $profile shoop_board.h] r]
set board_header [read $channel]
close $channel
if {![regexp {SHOOP_PL_DDR_BASE (0x[0-9a-f]+)u} $board_header _ arena_base] ||
    ![regexp {SHOOP_PL_DDR_BYTES (0x[0-9a-f]+)u} $board_header _ arena_bytes]} {
    error "The board profile has no PL DDR arena"
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
add_files [file join $host host_registers.sv]
foreach directory {state_processor job_processor chunk_memory media_store axi_memory} {
    add_files [lsort [glob [file join $root fpga $directory rtl *.sv]]]
}
foreach source {host_mailbox.sv host_dispatch.sv host_pipeline_axi.sv} {
    add_files [file join $host $source]
}
set_property include_dirs [list [file join $root fpga state_processor rtl]] [current_fileset]
update_compile_order -fileset sources_1

create_bd_design system
set ps [create_bd_cell -type ip -vlnv xilinx.com:ip:processing_system7:5.5 ps]
set_property -dict $preset $ps
set_property -dict [list CONFIG.PCW_USE_M_AXI_GP0 1 CONFIG.PCW_USE_S_AXI_HP0 1 \
    CONFIG.PCW_S_AXI_HP0_DATA_WIDTH 64 CONFIG.PCW_EN_CLK0_PORT 1 CONFIG.PCW_EN_RST0_PORT 1 \
    CONFIG.PCW_FPGA0_PERIPHERAL_FREQMHZ 100 CONFIG.PCW_USE_FABRIC_INTERRUPT 1 \
    CONFIG.PCW_IRQ_F2P_INTR 1] $ps
make_bd_intf_pins_external [get_bd_intf_pins ps/DDR] [get_bd_intf_pins ps/FIXED_IO]

set core [create_bd_cell -type module -reference host_pipeline_axi core]
set_property -dict [list CONFIG.ARENA_BASE [expr {$arena_base}] \
    CONFIG.ARENA_BYTES [expr {$arena_bytes}]] $core
create_bd_cell -type ip -vlnv xilinx.com:ip:proc_sys_reset:5.0 reset
set_property CONFIG.C_EXT_RESET_HIGH 0 [get_bd_cells reset]
create_bd_cell -type ip -vlnv xilinx.com:ip:axi_interconnect:2.1 gp0
set_property CONFIG.NUM_MI 1 [get_bd_cells gp0]
foreach {name width value} {zero 1 0 one 1 1 sample_zero 32 0 boot_epoch 32 1} {
    set constant [create_bd_cell -type ip -vlnv xilinx.com:ip:xlconstant:1.1 $name]
    set_property -dict [list CONFIG.CONST_WIDTH $width CONFIG.CONST_VAL $value] $constant
}

connect_bd_net [get_bd_pins ps/FCLK_CLK0] [get_bd_pins ps/S_AXI_HP0_ACLK] \
    [get_bd_pins ps/M_AXI_GP0_ACLK] [get_bd_pins gp0/ACLK] [get_bd_pins gp0/S00_ACLK] \
    [get_bd_pins gp0/M00_ACLK] [get_bd_pins core/clk] [get_bd_pins reset/slowest_sync_clk]
connect_bd_net [get_bd_pins ps/FCLK_RESET0_N] [get_bd_pins reset/ext_reset_in]
connect_bd_net [get_bd_pins reset/interconnect_aresetn] [get_bd_pins gp0/ARESETN]
connect_bd_net [get_bd_pins reset/peripheral_aresetn] [get_bd_pins gp0/S00_ARESETN] [get_bd_pins gp0/M00_ARESETN]
connect_bd_net [get_bd_pins reset/peripheral_reset] [get_bd_pins core/reset]
connect_bd_net [get_bd_pins zero/dout] [get_bd_pins reset/mb_debug_sys_rst] [get_bd_pins core/quantum_valid] \
    [get_bd_pins ps/S_AXI_HP0_RDISSUECAP1_EN] [get_bd_pins ps/S_AXI_HP0_WRISSUECAP1_EN]
connect_bd_net [get_bd_pins one/dout] [get_bd_pins reset/dcm_locked] [get_bd_pins reset/aux_reset_in] \
    [get_bd_pins core/sync_event_ready]
connect_bd_net [get_bd_pins sample_zero/dout] [get_bd_pins core/sample_input_data]
connect_bd_net [get_bd_pins boot_epoch/dout] [get_bd_pins core/epoch]
connect_bd_net [get_bd_pins core/irq] [get_bd_pins ps/IRQ_F2P]

connect_bd_intf_net [get_bd_intf_pins ps/M_AXI_GP0] [get_bd_intf_pins gp0/S00_AXI]
connect_bd_intf_net [get_bd_intf_pins gp0/M00_AXI] [get_bd_intf_pins core/s_axi]
connect_bd_intf_net [get_bd_intf_pins core/m_axi] [get_bd_intf_pins ps/S_AXI_HP0]
assign_bd_address -offset 0x40000000 -range 4K -target_address_space [get_bd_addr_spaces ps/Data] \
    [get_bd_addr_segs core/s_axi/reg0]
assign_bd_address
validate_bd_design
save_bd_design

set bd [get_files */system.bd]
generate_target all $bd
add_files [make_wrapper -files $bd -top]
set_property top system_wrapper [current_fileset]
update_compile_order -fileset sources_1
set scripts [glob -nocomplain [file join $output project zturn_v2.gen sources_1 bd system ip * ps7_init.tcl]]
if {[llength $scripts] != 1} {error "Could not identify the generated PS initialization script"}
file copy -force [lindex $scripts 0] [file join $output ps7_init.tcl]
if {$stage eq "generate"} {return}

launch_runs impl_1 -to_step write_bitstream -jobs 4
wait_on_run impl_1
if {[get_property PROGRESS [get_runs impl_1]] ne "100%"} {error "Vivado implementation failed"}
open_run impl_1
report_timing_summary -file [file join $output timing.rpt]
report_route_status -file [file join $output route_status.rpt]
report_cdc -file [file join $output cdc.rpt]
report_utilization -hierarchical -file [file join $output utilization.rpt]
report_drc -file [file join $output drc.rpt]
if {[get_property SLACK [get_timing_paths -delay_type max -max_paths 1]] < 0 ||
    [get_property SLACK [get_timing_paths -delay_type min -max_paths 1]] < 0} {
    error "Integrated Z-turn V2 design missed timing. See timing.rpt"
}
file copy -force [file join $output project zturn_v2.runs impl_1 system_wrapper.bit] [file join $output zturn_v2.bit]
write_hw_platform -fixed -include_bit -force -file [file join $output zturn_v2.xsa]
