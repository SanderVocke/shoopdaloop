if {$argc < 1 || $argc > 2} {
    error "usage: vivado -mode batch -source fpga/zedboard/build.tcl -tclargs BUILD_DIRECTORY ?BOARD_PART?"
}
source [file join [file normalize [lindex $argv 0]] manifest.tcl]
if {![info exists bench_flavor]} {set bench_flavor benchmark}
if {$bench_flavor ni {benchmark host}} {error "Unknown build flavor $bench_flavor"}
set bench_stage bitstream
if {[info exists ::env(SHOOP_BUILD_STAGE)]} {set bench_stage $::env(SHOOP_BUILD_STAGE)}
if {$bench_stage ni {generate synth bitstream}} {error "SHOOP_BUILD_STAGE must be generate, synth or bitstream"}
cd $bench_output
if {[info exists ::env(SHOOP_BOARD_REPO)]} {
    set_param board.repoPaths [concat [get_param board.repoPaths] [list [file normalize $::env(SHOOP_BOARD_REPO)]]]
}
create_project zedboard_timing [file join $bench_output project] -part xc7z020clg484-1 -force
if {[info exists ::env(SHOOP_IP_REPO)]} {
    set ip_repo [file normalize $::env(SHOOP_IP_REPO)]
    if {![file isdirectory $ip_repo]} {error "SHOOP_IP_REPO is not a directory: $ip_repo"}
    set_property ip_repo_paths [list $ip_repo] [current_project]
    if {[catch {update_ip_catalog} detail]} {
        error "Cannot load SHOOP_IP_REPO $ip_repo. Supply a complete IP repository matching Vivado [version -short]. $detail"
    }
}
set missing_ips {}
set required_ips {processing_system7:5.5 proc_sys_reset:5.0 xlconstant:1.1}
lappend required_ips {*}[expr {$bench_flavor eq "host" ? {axi_interconnect:2.1} : {vio:3.0 ila:6.2}}]
foreach ip $required_ips {
    if {[llength [get_ipdefs -all -quiet xilinx.com:ip:$ip]] == 0} {lappend missing_ips $ip}
}
if {[llength $missing_ips]} {
    error "Vivado [version -short] lacks required catalog IPs: [join $missing_ips {, }]. Install the matching IP catalog or set SHOOP_IP_REPO to its directory."
}
if {$argc == 2} {
    set board [lindex $argv 1]
} else {
    set board [lindex [lsort -dictionary [concat [get_board_parts -quiet *:zed:part0:*] [get_board_parts -quiet *:zedboard:part0:*]]] end]
}
if {$board eq ""} {error "Install the Digilent ZedBoard board files, or specify its BOARD_PART"}
set_property board_part $board [current_project]
add_files $bench_sources
if {$bench_flavor eq "host"} {
    set include_dir [file join $bench_root fpga host_bridge rtl]
    set includes {}
    foreach name {host_body.svh host_map.svh host_capacity.svh} {lappend includes [file join $include_dir $name]}
} else {
    set include_dir [file join $bench_root fpga zedboard rtl]
    set includes [list [file join $include_dir timing_body.svh]]
}
add_files [list [file join $bench_root fpga state_processor rtl state_processor_defs.svh] {*}$includes]
set_property include_dirs [list [file join $bench_root fpga state_processor rtl] $include_dir] [current_fileset]
create_bd_design system
create_bd_cell -type ip -vlnv xilinx.com:ip:processing_system7:5.5 ps
apply_bd_automation -rule xilinx.com:bd_rule:processing_system7 -config {
    make_external "FIXED_IO, DDR" apply_board_preset "1" Master "Disable" Slave "Disable"
} [get_bd_cells ps]
# AMD's ZedBoard PS preset; the board-file DDR settings do not initialize DDR on every board.
set_property CONFIG.preset {ZedBoard} [get_bd_cells ps]
set_property -dict [list CONFIG.PCW_USE_S_AXI_HP0 {1} CONFIG.PCW_S_AXI_HP0_DATA_WIDTH {64} \
    CONFIG.PCW_USE_M_AXI_GP0 [expr {$bench_flavor eq "host"}] CONFIG.PCW_EN_CLK0_PORT {1} CONFIG.PCW_EN_RST0_PORT {1} \
    CONFIG.PCW_FPGA0_PERIPHERAL_FREQMHZ $bench_fabric_mhz] [get_bd_cells ps]
create_bd_cell -type module -reference [expr {$bench_flavor eq "host" ? "zedboard_host" : "zedboard_bench"}] bench
create_bd_cell -type ip -vlnv xilinx.com:ip:proc_sys_reset:5.0 reset
set_property CONFIG.C_EXT_RESET_HIGH {0} [get_bd_cells reset]
create_bd_cell -type ip -vlnv xilinx.com:ip:xlconstant:1.1 zero
set_property CONFIG.CONST_VAL {0} [get_bd_cells zero]
create_bd_cell -type ip -vlnv xilinx.com:ip:xlconstant:1.1 one
set_property CONFIG.CONST_VAL {1} [get_bd_cells one]
connect_bd_net [get_bd_pins ps/FCLK_CLK0] [get_bd_pins ps/S_AXI_HP0_ACLK] [get_bd_pins bench/clk] [get_bd_pins reset/slowest_sync_clk]
connect_bd_net [get_bd_pins ps/FCLK_RESET0_N] [get_bd_pins reset/ext_reset_in]
connect_bd_net [get_bd_pins zero/dout] [get_bd_pins reset/mb_debug_sys_rst] \
    [get_bd_pins ps/S_AXI_HP0_RDISSUECAP1_EN] [get_bd_pins ps/S_AXI_HP0_WRISSUECAP1_EN]
# aux_reset_in is active-low (C_AUX_RESET_HIGH=0): tie it inactive.
connect_bd_net [get_bd_pins one/dout] [get_bd_pins reset/dcm_locked] [get_bd_pins reset/aux_reset_in]
connect_bd_net [get_bd_pins reset/peripheral_aresetn] [get_bd_pins bench/aresetn]
connect_bd_intf_net [get_bd_intf_pins bench/m_axi] [get_bd_intf_pins ps/S_AXI_HP0]
if {$bench_flavor eq "host"} {
    # The ARM reaches the bridge's AXI-lite registers through GP0 at a fixed address.
    create_bd_cell -type ip -vlnv xilinx.com:ip:axi_interconnect:2.1 gp0
    set_property CONFIG.NUM_MI {1} [get_bd_cells gp0]
    connect_bd_intf_net [get_bd_intf_pins ps/M_AXI_GP0] [get_bd_intf_pins gp0/S00_AXI]
    connect_bd_intf_net [get_bd_intf_pins gp0/M00_AXI] [get_bd_intf_pins bench/s_axi]
    connect_bd_net [get_bd_pins ps/FCLK_CLK0] [get_bd_pins ps/M_AXI_GP0_ACLK] [get_bd_pins gp0/ACLK] \
        [get_bd_pins gp0/S00_ACLK] [get_bd_pins gp0/M00_ACLK]
    connect_bd_net [get_bd_pins reset/interconnect_aresetn] [get_bd_pins gp0/ARESETN]
    connect_bd_net [get_bd_pins reset/peripheral_aresetn] [get_bd_pins gp0/S00_ARESETN] [get_bd_pins gp0/M00_ARESETN]
    assign_bd_address -offset 0x40000000 -range 4K -target_address_space [get_bd_addr_spaces ps/Data] \
        [get_bd_addr_segs bench/s_axi/reg0]
}
assign_bd_address
if {$bench_flavor eq "benchmark"} {
create_bd_cell -type ip -vlnv xilinx.com:ip:vio:3.0 vio
set settings [list CONFIG.C_NUM_PROBE_IN {17} CONFIG.C_NUM_PROBE_OUT {7} CONFIG.C_PROBE_IN0_WIDTH {32}]
for {set i 1} {$i <= 16} {incr i} {lappend settings CONFIG.C_PROBE_IN${i}_WIDTH 64}
foreach i {0 1 2} {lappend settings CONFIG.C_PROBE_OUT${i}_WIDTH 1 CONFIG.C_PROBE_OUT${i}_INIT_VAL 0x0}
foreach i {3 4 5 6} {lappend settings CONFIG.C_PROBE_OUT${i}_WIDTH 32}
lappend settings CONFIG.C_PROBE_OUT3_INIT_VAL 0x3e8 CONFIG.C_PROBE_OUT4_INIT_VAL [format 0x%x $bench_warmup] \
    CONFIG.C_PROBE_OUT5_INIT_VAL 0x0 CONFIG.C_PROBE_OUT6_INIT_VAL 0x989680
set_property -dict $settings [get_bd_cells vio]
connect_bd_net [get_bd_pins ps/FCLK_CLK0] [get_bd_pins vio/clk]
connect_bd_net [get_bd_pins bench/status] [get_bd_pins vio/probe_in0]
for {set i 0} {$i < 16} {incr i} {
    connect_bd_net [get_bd_pins bench/stat$i] [get_bd_pins vio/probe_in[expr {$i+1}]]
}
set i 0
foreach signal {start stop snapshot quanta_limit warmup_quanta gap_cycles watchdog_cycles} {
    connect_bd_net [get_bd_pins vio/probe_out$i] [get_bd_pins bench/$signal]
    incr i
}
create_bd_cell -type ip -vlnv xilinx.com:ip:ila:6.2 ila
set_property -dict [list CONFIG.C_MONITOR_TYPE {Native} CONFIG.C_NUM_OF_PROBES {2} CONFIG.C_PROBE0_WIDTH {128} \
    CONFIG.C_PROBE1_WIDTH {96} CONFIG.C_DATA_DEPTH {2048}] [get_bd_cells ila]
connect_bd_net [get_bd_pins ps/FCLK_CLK0] [get_bd_pins ila/clk]
connect_bd_net [get_bd_pins bench/debug_state] [get_bd_pins ila/probe0]
connect_bd_net [get_bd_pins bench/debug_axi] [get_bd_pins ila/probe1]
}
validate_bd_design
save_bd_design
set bd [get_files */system.bd]
generate_target all $bd
add_files [make_wrapper -files $bd -top]
set_property top system_wrapper [current_fileset]
update_compile_order -fileset sources_1
set init_files [glob -nocomplain [file join $bench_output project zedboard_timing.gen sources_1 bd system ip * ps7_init.tcl]]
if {[llength $init_files] != 1} {error "Cannot locate generated board-specific ps7_init.tcl: $init_files"}
file copy -force [lindex $init_files 0] [file join $bench_output ps7_init.tcl]
if {$bench_stage eq "generate"} {puts "Block design and IP generation complete: $bench_output"; return}
launch_runs synth_1 -jobs 4
wait_on_run synth_1
if {[get_property PROGRESS [get_runs synth_1]] ne "100%"} {error "Synthesis did not complete"}
if {$bench_stage eq "synth"} {
    open_run synth_1
    write_checkpoint -force [file join $bench_output bench_synth.dcp]
    puts "Benchmark synthesis complete: $bench_output"
    return
}
launch_runs impl_1 -to_step write_bitstream -jobs 4
wait_on_run impl_1
if {[get_property PROGRESS [get_runs impl_1]] ne "100%"} {error "Bitstream generation did not complete"}
open_run impl_1
report_utilization -file [file join $bench_output utilization.rpt]
report_utilization -hierarchical -hierarchical_depth 12 -file [file join $bench_output utilization_hierarchical.rpt]
if {$bench_flavor eq "benchmark"} {write_debug_probes -force [file join $bench_output bench.ltx]}
file copy -force [file join $bench_output project zedboard_timing.runs impl_1 system_wrapper.bit] [file join $bench_output bench.bit]
write_hw_platform -fixed -include_bit -force -file [file join $bench_output bench.xsa]
puts "Benchmark deployment artifacts: $bench_output"
