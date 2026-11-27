
set_param general.maxThreads 4
if {[version -short] ne "2021.1"} { error "Vivado 2021.1 required" }
create_project -in_memory -part xc7z020clg400-2
set_property include_dirs {/source/fpga/state_processor/rtl /source/fpga/host_bridge/rtl} [current_fileset]
read_verilog -sv [list /source/fpga/host_bridge/rtl/semantic_registers.sv /source/fpga/axi_memory/rtl/axi_burst_mux.sv /source/fpga/axi_memory/rtl/axi_master.sv /source/fpga/axi_memory/rtl/chunk_axi.sv /source/fpga/axi_memory/rtl/chunk_dma.sv /source/fpga/axi_memory/rtl/chunk_memory_axi.sv /source/fpga/axi_memory/rtl/fifo_media_audio_pipeline_axi.sv /source/fpga/axi_memory/rtl/fifo_recovery_pipeline_axi.sv /source/fpga/axi_memory/rtl/media_audio_pipeline_axi.sv /source/fpga/axi_memory/rtl/media_store_axi.sv /source/fpga/axi_memory/rtl/program_axi_mux.sv /source/fpga/axi_memory/rtl/shared_axi.sv /source/fpga/axi_memory/rtl/word_axi.sv /source/fpga/axi_memory/rtl/word_dma.sv /source/fpga/chunk_memory/rtl/chunk_memory.sv /source/fpga/chunk_memory/rtl/chunk_priority.sv /source/fpga/chunk_memory/rtl/chunk_ram.sv /source/fpga/host_bridge/rtl/fifo_audio_pipeline.sv /source/fpga/host_bridge/rtl/fifo_config_service.sv /source/fpga/host_bridge/rtl/fifo_edit_kernel.sv /source/fpga/host_bridge/rtl/fifo_group_banks.sv /source/fpga/host_bridge/rtl/fifo_group_consumer.sv /source/fpga/host_bridge/rtl/fifo_media_arbiter.sv /source/fpga/host_bridge/rtl/fifo_media_audio_pipeline.sv /source/fpga/host_bridge/rtl/fifo_media_loader.sv /source/fpga/host_bridge/rtl/fifo_mmio.sv /source/fpga/host_bridge/rtl/fifo_mmio_pipeline_axi.sv /source/fpga/host_bridge/rtl/fifo_observations.sv /source/fpga/host_bridge/rtl/fifo_snapshots.sv /source/fpga/host_bridge/rtl/host_fault_record.sv /source/fpga/host_bridge/rtl/host_recovery.sv /source/fpga/job_processor/rtl/audio_pipeline.sv /source/fpga/job_processor/rtl/job_processor.sv /source/fpga/job_processor/rtl/jp_divider.sv /source/fpga/job_processor/rtl/jp_sample_ram.sv /source/fpga/media_store/rtl/media_audio_pipeline.sv /source/fpga/media_store/rtl/media_metadata.sv /source/fpga/media_store/rtl/media_prefetch_resolver.sv /source/fpga/media_store/rtl/media_ram.sv /source/fpga/media_store/rtl/media_reclaimer.sv /source/fpga/media_store/rtl/media_store.sv /source/fpga/media_store/rtl/memory_delay.sv /source/fpga/media_store/rtl/metadata_cache.sv /source/fpga/state_processor/rtl/sp_engine.sv /source/fpga/state_processor/rtl/sp_job_fifo.sv /source/fpga/state_processor/rtl/sp_program_prefetch.sv /source/fpga/state_processor/rtl/sp_ram.sv /source/fpga/state_processor/rtl/state_processor.sv /source/semantic_profile.sv]
synth_design -mode out_of_context -flatten_hierarchy none -top semantic_profile -part xc7z020clg400-2
create_clock -name core -period 10 [get_ports clk]
set_input_delay -clock core 1 [get_ports -filter {DIRECTION == IN && NAME != clk}]
set_output_delay -clock core 1 [all_outputs]
report_utilization -hierarchical -file component-utilization.rpt
report_timing_summary -file component-synthesis-timing.rpt
set report [open component-primitives.txt w]
foreach {label filter} {LUT {REF_NAME =~ LUT*} FF {REF_NAME =~ FD*} CARRY4 {REF_NAME == CARRY4} RAMB36 {REF_NAME == RAMB36E1} RAMB18 {REF_NAME == RAMB18E1} DSP {REF_NAME == DSP48E1}} {
    puts $report "$label [llength [get_cells -hierarchical -filter $filter]]"
}
puts $report "PORT_BITS [llength [get_ports]]"
close $report
write_checkpoint -force component-synthesis.dcp
opt_design
report_utilization -hierarchical -file component-optimized-utilization.rpt
report_timing_summary -file component-optimized-timing.rpt
write_checkpoint -force component-optimized.dcp
if {0} {
    opt_design
    place_design
    route_design
    report_utilization -hierarchical -file component-routed-utilization.rpt
    report_timing_summary -file component-routed-timing.rpt
    report_timing -max_paths 10 -file component-paths.rpt
    report_drc -file component-drc.rpt
    write_checkpoint -force component-routed.dcp
}
set counts [dict create]
foreach cell [get_cells -hierarchical -filter {IS_PRIMITIVE == 1}] {
    set name [get_property NAME $cell]
    set region frontend_and_glue
    if {[string match "dut/pipeline/execution/core/pipeline/execution/*" $name]} {set region audio_execution}
    if {[string match "dut/pipeline/execution/core/media/*" $name]} {set region media_store}
    if {[string match "dut/pipeline/execution/memory_endpoint/*" $name]} {set region axi_backend}
    if {[string match "dut/pipeline/execution/ddr_program.program_bus/*" $name]} {set region program_backend}
    dict incr counts [list $region [get_property REF_NAME $cell]]
}
set report [open component-partitions.txt w]
puts $report "REGION PRIMITIVE COUNT"
foreach key [lsort [dict keys $counts]] {puts $report "$key [dict get $counts $key]"}
close $report
if {[llength [get_cells -quiet dut/pipeline/execution/core/pipeline/execution]] != 1} {error "missing accounting boundary dut/pipeline/execution/core/pipeline/execution"}
if {[llength [get_cells -quiet dut/pipeline/execution/core/media]] != 1} {error "missing accounting boundary dut/pipeline/execution/core/media"}
if {[llength [get_cells -quiet dut/pipeline/execution/memory_endpoint]] != 1} {error "missing accounting boundary dut/pipeline/execution/memory_endpoint"}
if {[llength [get_cells -quiet dut/pipeline/execution/ddr_program.program_bus]] != 1} {error "missing accounting boundary dut/pipeline/execution/ddr_program.program_bus"}
if {[llength [get_cells -quiet dut/pipeline/recovery]] != 1} {error "missing accounting boundary dut/pipeline/recovery"}
if {[llength [get_cells -quiet dut/pipeline/retained_fault]] != 1} {error "missing accounting boundary dut/pipeline/retained_fault"}
if {[llength [get_cells -quiet dut/pipeline/execution/core/pipeline/consumer]] != 1} {error "missing accounting boundary dut/pipeline/execution/core/pipeline/consumer"}
if {[llength [get_cells -quiet dut/pipeline/execution/core/pipeline/observations]] != 1} {error "missing accounting boundary dut/pipeline/execution/core/pipeline/observations"}
if {[llength [get_cells -quiet dut/pipeline/execution/core/pipeline/snapshots]] != 1} {error "missing accounting boundary dut/pipeline/execution/core/pipeline/snapshots"}
if {[llength [get_cells -quiet dut/pipeline/execution/core/owner/loader]] != 1} {error "missing accounting boundary dut/pipeline/execution/core/owner/loader"}
if {[llength [get_cells -quiet dut/pipeline/execution/core/owner]] != 1} {error "missing accounting boundary dut/pipeline/execution/core/owner"}
if {[llength [get_cells -quiet dut/mmio]] != 1} {error "missing accounting boundary dut/mmio"}
set done [open complete.txt w]; puts $done passed; close $done
