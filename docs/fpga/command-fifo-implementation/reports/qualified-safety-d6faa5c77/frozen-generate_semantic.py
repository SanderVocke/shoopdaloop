#!/usr/bin/env python3
"""Connect the semantic group consumer to the execution-owned audio core."""
import argparse
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "axi_memory"))
from generate_tops import header


def generate():
    audio, ports = header((HERE.parent / "job_processor/rtl/audio_pipeline.sv").read_text())
    _, fifo = header((HERE / "rtl/fifo_group_consumer.sv").read_text())
    _, observations = header((HERE / "rtl/fifo_observations.sv").read_text())
    _, snapshots = header((HERE / "rtl/fifo_snapshots.sv").read_text())
    snapshot_links = {n: "snapshot_" + n for _, n in snapshots}
    snapshot_links.update({"clk": "clk", "reset": "reset", "epoch": "epoch", "capture_valid": "snapshot_owner",
                           "capture_fault": "snapshots_fault", "peak_cycles": "peak_snapshot_cycles"})
    snapshot_fixed = {"fault": "execution_fault", "capture_initial": "initial_capture", "capture_revision": "active_revision",
                      "capture_frame": "initial_capture ? 64'd0 : capture_frame",
                      "capture_quantum": "initial_capture ? run_quantum : snapshot_quantum", "memory_ready": "snapshot_owner && execution_local_ready",
                      "memory_response_valid": "snapshot_owner && execution_local_response_valid",
                      "memory_response_error": "execution_local_response_error", "memory_read_data": "execution_local_read_data",
                      "metadata_ready": "snapshot_owner && private_observation_read_ready",
                      "metadata_response_valid": "private_observation_response_valid && observation_response_snapshot",
                      "metadata_response_error": "private_observation_response_error", "metadata_read_data": "private_observation_read_data"}
    private_snapshots = {"clk", "reset", "epoch", "fault", "capture_fault"}
    private_snapshots.update(n for _, n in snapshots if n.startswith(("capture_", "memory_", "metadata_")))
    observation_links = {"revision": "active_revision", "gain_valid": "gain_update_valid", "gain_slot": "gain_update_slot",
                         "gain_bits": "gain_update_bits", "event_valid": "sync_event_valid", "event_data": "sync_event_data",
                         "faulted": "observations_fault", "fault_latched": "observation_abort"}
    observation_links.update({n: "observation_" + n for _, n in observations
                              if n.startswith(("read_", "response_")) or n in {"cfg_valid", "cfg_ready", "cfg_response_valid", "cfg_response_error", "cfg_read_data"}})
    observation_links.update({n: "private_observation_" + n for _, n in observations if n.startswith(("read_", "response_"))})
    constants = {"run_instruction_count": "32'd0", "run_sync_count": "32'd0",
                 "run_program_region": "1'b0", "run_state_count": "32'd0", "run_buffers": "32'd0",
                 "sync_event_ready": "1'b1", "capture_ready": "snapshot_capture_ready",
                 "handoff_complete": "handoff_valid && handoff_pending && !handoff_error && handoff_epoch == pending_epoch && handoff_sequence == pending_sequence"}
    audio_links = {
        "run_valid": "render_valid", "run_ready": "render_ready", "busy": "execution_busy", "abort_run": "execution_abort",
        "active_revision": "execution_revision", "switch_boundary": "select_valid",
        "switch_valid": "select_valid", "switch_ready": "select_ready", "switch_refused": "select_error",
        "switch_expected": "active_revision", "switch_revision": "select_revision",
        "switch_region": "select_region", "switch_instructions": "select_instructions",
        "switch_sync_count": "select_sync_count", "switch_states": "select_states", "switch_buffers": "select_buffers",
        "local_valid": "execution_local_valid", "local_ready": "execution_local_ready", "local_state": "execution_local_state",
        "local_write": "execution_local_write", "local_address": "execution_local_address", "local_write_data": "memory_write_data",
        "local_response_valid": "execution_local_response_valid", "local_response_error": "execution_local_response_error",
        "local_read_data": "execution_local_read_data",
        "cfg_valid": "execution_cfg_valid", "cfg_ready": "execution_cfg_ready",
        "cfg_response_valid": "execution_cfg_response_valid", "cfg_response_error": "execution_cfg_response_error",
        "cfg_read_data": "execution_cfg_read_data", "media_fault": "execution_media_fault",
    }
    fifo_links = {
        "boundary_valid": "run_valid && initial_done", "boundary_ready": "semantic_boundary_ready", "boundary_frame": "sync_frame",
        "pipeline_fault": "execution_fault", "handoff_valid": "handoff_valid && (handoff_error || handoff_request)",
    }
    private_fifo = {"boundary_valid", "boundary_ready", "boundary_frame", "render_valid", "render_ready",
                    "fault_event", "fault_reason", "fault_sequence", "fault_frame", "fault_opcode"}
    private_fifo.update(n for _, n in fifo if n.startswith(("memory_", "select_")))
    fifo_public = {n for _, n in fifo if n not in private_fifo}
    private_audio = set(constants) | set(audio_links)
    private_audio.update(n for _, n in ports if n.startswith("switch_"))
    retained_audio = {"cfg_valid", "cfg_ready", "cfg_response_valid", "cfg_response_error", "cfg_read_data", "media_fault"}
    public = [(p, n) for p, n in ports if n not in private_audio or n in retained_audio]
    public.extend((p, "observation_" + n if n.startswith(("read_", "response_")) else n) for p, n in observations if n.startswith(("read_", "response_", "trace_")))
    public.extend((p, snapshot_links[n]) for p, n in snapshots if n not in private_snapshots)
    public.extend((p, n) for p, n in fifo if n in fifo_public and n not in {n for _, n in public})
    public.extend((p, n) for p, n in ports if n in {"run_valid", "run_ready", "busy"})
    public.append(("input wire [31:0] ", "pipeline_fault_cause"))
    public.extend(("output wire ", n) for n in ["diagnostic_valid"])
    public.extend(("output wire [31:0] ", "diagnostic_" + n) for n in ["cause", "opcode", "cycles", "control_cycles", "detail"])
    public.extend(("output wire [63:0] ", "diagnostic_" + n) for n in ["sequence", "frame", "revision"])
    params = re.sub(r"\s*parameter integer (?:LIVE_LOADING|PROGRAM_REGIONS|CAPTURE_SYNC) = [^,]+,", "", audio[2]).strip()
    defaults = {"PROGRAM_DEPTH": 1024, "STATE_DEPTH": 512, "JOB_DEPTH": 1024, "LOOP_DEPTH": 128,
                "BUFFERS": 512, "INPUT_PORTS": 1, "CHUNK_FRAMES": 32}
    for name, value in defaults.items():
        params = re.sub(rf"(parameter integer {name} = )[^,\n]+", rf"\g<1>{value}", params)
    names = re.findall(r"parameter integer (\w+)\s*=", params)
    text = '// Generated by generate_semantic.py; do not edit by hand.\n`include "state_processor_defs.svh"\n\n'
    text += f"module fifo_audio_pipeline #(\n    {params}\n) (\n"
    text += ",\n".join("    " + p.replace("output reg", "output wire") + n for p, n in public) + "\n);\n"
    text += '    initial if (LOOP_DEPTH < 1 || LOOP_DEPTH > 128) $fatal(1,"semantic loop profile exceeds 128 slots");\n'
    declared = {n for _, n in public}
    for group, links, fixed in [(fifo, {}, fifo_links), (ports, audio_links, constants), (observations, observation_links, {}), (snapshots, snapshot_links, snapshot_fixed)]:
        for p, n in group:
            wire = links.get(n, n)
            if n in fixed or wire in declared:
                continue
            text += "    " + re.sub(r"(?:input|output)\s+(?:wire|reg)", "wire", p) + wire + ";\n"
            declared.add(wire)
    text += "    wire execution_fault = pipeline_fault || observations_fault || snapshots_fault || error_code != 0 || execution_revision != active_revision;\n"
    text += "    assign execution_media_fault = media_fault || observation_abort || snapshots_fault;\n"
    text += "    assign execution_abort = pipeline_fault || faulted || observation_abort || snapshots_fault || error_code != 0;\n"
    text += """    reg opportunity_active;
    reg [63:0] opportunity_frame;
    reg [31:0] opportunity_cycles;
    assign diagnostic_valid = fault_event;
    assign diagnostic_cause = (fault_reason[5] ? 32'd8 : 0)
        | ((fault_reason[3] || fault_reason[4]) ? 32'd1 : 0)
        | (error_code!=0 ? 32'd2 : 0)
        | (|{fault_reason[7:6],fault_reason[2:1]} ? 32'd64 : 0)
        | (fault_reason[0] ? ((pipeline_fault ? (pipeline_fault_cause!=0 ? pipeline_fault_cause : 32'd64) : 0)
            | ((observations_fault || snapshots_fault || error_code!=0 || execution_revision!=active_revision) ? 32'd64 : 0)) : 0);
    assign diagnostic_sequence = fault_sequence;
    assign diagnostic_frame = fault_sequence!=0 ? fault_frame : opportunity_active ? opportunity_frame : sync_frame;
    assign diagnostic_revision = active_revision;
    assign diagnostic_opcode = fault_sequence!=0 ? fault_opcode : 32'hffffffff;
    assign diagnostic_cycles = opportunity_active ? opportunity_cycles : 0;
    assign diagnostic_control_cycles = fault_sequence!=0 && !fault_reason[6] ? current_command_cycles : 0;
    assign diagnostic_detail = {6'b0,fault_reason,3'b0,execution_revision!=active_revision,
        snapshots_fault,observations_fault,capture_valid,command_phase,error_stage,error_code};
    always @(posedge clk) begin
        if (reset) begin opportunity_active<=0; opportunity_frame<=0; opportunity_cycles<=0; end
        else begin
            if (done || fault_event || faulted) opportunity_active<=0;
            if (opportunity_active && !(&opportunity_cycles)) opportunity_cycles<=opportunity_cycles+1'b1;
            if (run_valid && run_ready) begin opportunity_active<=1; opportunity_frame<=sync_frame; opportunity_cycles<=0; end
        end
    end
    reg [31:0] snapshot_quantum, initial_epoch;
    reg [1:0] initial_phase;
    reg observation_response_snapshot;
    wire initial_capture = initial_phase==1 && initial_epoch==epoch && epoch!=0 && !reset;
    wire initial_done = initial_phase==3 && initial_epoch==epoch;
    assign snapshot_owner = capture_valid || initial_capture;
    wire semantic_boundary_ready;
    assign run_ready = initial_done && semantic_boundary_ready;
    assign execution_local_valid = snapshot_owner ? snapshot_memory_valid : memory_valid;
    assign execution_local_state = !snapshot_owner && memory_state;
    assign execution_local_write = !snapshot_owner && memory_write;
    assign execution_local_address = snapshot_owner ? snapshot_memory_address : memory_address;
    assign memory_ready = !snapshot_owner && execution_local_ready;
    assign memory_response_valid = !snapshot_owner && execution_local_response_valid;
    assign memory_response_error = execution_local_response_error;
    assign memory_read_data = execution_local_read_data;
    assign private_observation_read_valid = snapshot_owner ? snapshot_metadata_valid : observation_read_valid;
    assign private_observation_read_slot = snapshot_owner ? snapshot_metadata_slot : observation_read_slot;
    assign observation_read_ready = !snapshot_owner && private_observation_read_ready;
    assign observation_response_valid = private_observation_response_valid && !observation_response_snapshot;
    assign observation_response_error = private_observation_response_error;
    assign observation_read_data = private_observation_read_data;
    always @(posedge clk) begin
        if (reset) begin initial_epoch<=epoch; initial_phase<=0; end
        else if (initial_epoch==epoch) case (initial_phase)
            0: if (epoch!=0 && run_quantum!=0) initial_phase<=1;
            1: if (snapshot_capture_ready) initial_phase<=2;
            2: if (!snapshot_capture_ready) initial_phase<=3;
            default: ;
        endcase
        if (reset) begin snapshot_quantum<=0; observation_response_snapshot<=0; end
        else begin
            if (render_valid && render_ready) snapshot_quantum<=run_quantum;
            if (private_observation_read_valid && private_observation_read_ready)
                observation_response_snapshot<=snapshot_owner;
        end
    end
    reg configuration_pending, configuration_observations;
    wire observation_config = cfg_state && cfg_loop;
    wire configuration_available = !configuration_pending && !run_valid && !capture_valid && initial_done && !reset && !faulted && !execution_fault;
    assign execution_cfg_valid = cfg_valid && !observation_config && configuration_available;
    assign observation_cfg_valid = cfg_valid && observation_config && configuration_available;
    assign cfg_ready = configuration_available && (observation_config ? observation_cfg_ready : execution_cfg_ready);
    assign cfg_response_valid = configuration_pending
        && (configuration_observations ? observation_cfg_response_valid : execution_cfg_response_valid);
    assign cfg_response_error = configuration_observations ? observation_cfg_response_error : execution_cfg_response_error;
    assign cfg_read_data = configuration_observations ? observation_cfg_read_data : execution_cfg_read_data;
    always @(posedge clk) begin
        if (reset) begin configuration_pending<=0; configuration_observations<=0; end
        else begin
            if (cfg_response_valid) configuration_pending<=0;
            if (cfg_valid && cfg_ready) begin configuration_pending<=1; configuration_observations<=observation_config; end
        end
    end
"""
    text += "    assign busy = execution_busy || command_phase || render_valid || !initial_done;\n"
    text += "    assign switch_response_ready = switch_response_error || handoff_pending;\n"
    text += "    fifo_group_consumer #(.LOOPS(LOOP_DEPTH), .STATES(STATE_DEPTH), .PROGRAM_DEPTH(PROGRAM_DEPTH), .BUFFERS(BUFFERS)) consumer (\n"
    text += ",\n".join(f"        .{n}({fifo_links.get(n, n)})" for _, n in fifo) + "\n    );\n"
    text += "    fifo_observations #(.LOOPS(LOOP_DEPTH)) observations (\n"
    text += ",\n".join(f"        .{n}({observation_links.get(n, n)})" for _, n in observations) + "\n    );\n"
    text += "    fifo_snapshots #(.LOOPS(LOOP_DEPTH)) snapshots (\n"
    text += ",\n".join(f"        .{n}({snapshot_fixed.get(n, snapshot_links[n])})" for _, n in snapshots) + "\n    );\n"
    text += "    audio_pipeline #(.LIVE_LOADING(1), .PROGRAM_REGIONS(2), .CAPTURE_SYNC(1),\n"
    text += ",\n".join(f"        .{n}({n})" for n in names) + "\n    ) execution (\n"
    text += ",\n".join(f"        .{n}({constants.get(n, audio_links.get(n, n))})" for _, n in ports) + "\n    );\nendmodule\n"
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    path = HERE / "rtl/fifo_audio_pipeline.sv"
    text = generate()
    if args.check:
        if not path.is_file() or path.read_text() != text:
            raise SystemExit(f"stale generated top: {path}; run {__file__}")
    else:
        path.write_text(text)


if __name__ == "__main__":
    main()
