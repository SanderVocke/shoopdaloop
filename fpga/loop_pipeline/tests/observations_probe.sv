module observations_probe # (parameter integer SLOTS=8) (
    input wire clk,input wire reset,input wire [31:0] epoch,input wire [63:0] current_frame,
    input wire request_valid,output wire request_ready,input wire [7:0] request_opcode,
    input wire request_state,input wire [15:0] request_address,input wire [639:0] request_data,
    output wire response_valid,output wire [3:0] response_error,output wire [831:0] response_data,
    input wire cut_start,input wire [1:0] cut_mode,output wire cut_done,output wire fault,
    input wire [96:0] scalar_data,input wire [63:0] topology_epoch,
    input wire observe_valid,output wire observe_ready,input wire [831:0] observe_state,input wire [63:0] graph_epoch,
    output wire observe_done,output wire [3:0] observe_error,output wire [1727:0] observed_cut,
    input wire inspect_valid,output wire inspect_ready,input wire [31:0] inspect_slot,input wire [31:0] inspect_generation,
    output wire inspect_done,output wire [3:0] inspect_error,
    output wire media_valid,input wire media_ready,output wire [3:0] media_opcode,
    output wire [63:0] media_view,output wire [63:0] media_length,input wire media_done,
    input wire [7:0] media_error,input wire [31:0] media_result_view
);
    wire action_busy,action_fault,native_fault,observation_fault,media_fault;
    wire resident_valid,resident_ready,resident_done,resident_pending;
    wire [7:0] resident_opcode;
    wire [511:0] resident_data,resident_result;
    wire [63:0] resident_sequence,resident_frame;
    wire [47:0] resident_reference;
    wire [15:0] resident_slot;
    wire [31:0] resident_generation;
    wire [1:0] resident_mode;
    wire [3:0] resident_error;
    wire lookup_valid,lookup_ready,lookup_done,update_valid,update_ready,update_done;
    wire [15:0] lookup_slot,update_slot;
    wire [31:0] update_generation;
    wire [3:0] lookup_error,update_error;
    wire [767:0] lookup_data,update_data;
    wire record_request_valid,record_request_ready,record_request_ack,record_response_valid;
    wire [15:0] record_request_slot;
    wire [31:0] record_request_generation,record_request_epoch;
    wire [4:0] record_request_word;
    wire [3:0] record_response_error;
    wire [63:0] record_response_data;
    wire unused_cfg_valid,unused_cfg_state;
    wire [7:0] unused_cfg_opcode;
    wire [15:0] cfg_address;
    wire [639:0] unused_cfg_data;
    wire observation_valid,observation_ready,observation_done;
    wire [7:0] observation_opcode;
    wire [511:0] observation_data,observation_result;
    wire [3:0] observation_error;
    wire ledger_valid,ledger_ready,ledger_done;
    wire [2:0] ledger_opcode;
    wire ledger_latest;
    wire [31:0] ledger_slot,ledger_generation;
    wire [63:0] ledger_epoch;
    wire [3:0] ledger_error;
    wire record_valid,record_ready,record_done;
    wire [15:0] record_slot;
    wire [31:0] record_generation;
    wire [1919:0] record_data;
    wire [3:0] record_error;
    assign fault=action_fault || native_fault || observation_fault || media_fault;
    audio_actions #(.SLOTS(SLOTS),.NATIVE_ACTIONS(1)) actions (
        .busy(action_busy),.fault(action_fault),.cfg_valid(unused_cfg_valid),.cfg_ready(1'b0),.cfg_opcode(unused_cfg_opcode),
        .cfg_state(unused_cfg_state),.cfg_address(cfg_address),.cfg_data(unused_cfg_data),
        .cfg_response_valid(1'b0),.cfg_response_error(4'b0),.cfg_response_data(129'b0),.*
    );
    wire unused_tp_valid;
    wire [7:0] unused_tp_opcode;
    wire [31:0] unused_tp_index;
    wire [639:0] unused_tp_data;
    wire unused_configuration_valid;
    wire [7:0] unused_configuration_opcode;
    wire [511:0] unused_configuration_data;
    wire unused_signal_valid;
    wire [511:0] unused_signal_data;
    audio_native #(.REQUESTS(4)) native (
        .signal_valid(unused_signal_valid),.signal_ready(1'b0),.signal_data(unused_signal_data),.signal_done(1'b0),.signal_result(512'b0),
        .configuration_valid(unused_configuration_valid),.configuration_ready(1'b0),.configuration_opcode(unused_configuration_opcode),
        .configuration_data(unused_configuration_data),.configuration_done(1'b0),.configuration_result(512'b0),
        .clk(clk),.reset(reset),.request_valid(resident_valid),.request_ready(resident_ready),.request_opcode(resident_opcode),
        .request_flags(cfg_address),.request_mode(resident_mode),.request_data(resident_data),.request_sequence(resident_sequence),
        .request_frame(resident_frame),.request_reference(resident_reference),.response_valid(resident_done),.response_pending(resident_pending),
        .response_error(resident_error),.response_data(resident_result),.fault(native_fault),
        .tp_valid(unused_tp_valid),.tp_opcode(unused_tp_opcode),.tp_index(unused_tp_index),.tp_data(unused_tp_data),
        .tp_ready(1'b0),.tp_done(1'b0),.tp_error(4'b0),.tp_result(832'b0),.*
    );
    audio_observations #(.SLOTS(SLOTS)) observations (
        .clk(clk),.reset(reset),.epoch(epoch),.request_valid(observation_valid),.request_ready(observation_ready),.request_opcode(observation_opcode),
        .request_data(observation_data),.request_slot(resident_slot),.request_generation(resident_generation),.cut_data(observed_cut),
        .response_valid(observation_done),.response_error(observation_error),.response_data(observation_result),.fault(observation_fault),.*
    );
    cut_records #(.SLOTS(SLOTS),.WORDS(30)) records (
        .clk(clk),.reset(reset),.epoch(epoch),.commit_valid(record_valid),.commit_ready(record_ready),.commit_slot(record_slot),
        .commit_generation(record_generation),.commit_data(record_data),.commit_done(record_done),.commit_error(record_error),
        .request_valid(record_request_valid),.request_ready(record_request_ready),.request_ack(record_request_ack),.request_slot(record_request_slot),
        .request_generation(record_request_generation),.request_epoch(record_request_epoch),.request_word(record_request_word),
        .response_valid(record_response_valid),.response_error(record_response_error),.response_data(record_response_data)
    );
    wire ownership_ready,ownership_done,ownership_busy;
    wire [3:0] ownership_error;
    reg [1:0] owner;
    wire observing=observe_valid && !action_busy;
    wire inspecting=inspect_valid && !action_busy && !observing;
    assign observe_ready=ownership_ready && !action_busy;
    assign inspect_ready=ownership_ready && !action_busy && !observing;
    assign ledger_ready=ownership_ready && !observing && !inspecting;
    assign observe_done=ownership_done && owner == 0;
    assign inspect_done=ownership_done && owner == 1;
    assign ledger_done=ownership_done && owner == 2;
    assign observe_error=ownership_error;assign inspect_error=ownership_error;assign ledger_error=ownership_error;
    wire [31:0] unused_capture0,unused_capture1,unused_playback0,unused_playback1;
    wire unused=&{1'b0,ownership_busy};
    always @(posedge clk) begin
        if (reset) owner<=0;
        else if ((observing || inspecting || ledger_valid) && ownership_ready) owner<=observing ? 2'd0 : inspecting ? 2'd1 : 2'd2;
    end
    media_cuts ownership (
        .clk(clk),.reset(reset),.request_valid(observing || inspecting || ledger_valid),.request_ready(ownership_ready),
        .request_opcode(observing ? 3'd0 : inspecting ? 3'd4 : ledger_opcode),.request_latest(ledger_latest),
        .request_slot(inspecting ? inspect_slot : ledger_slot),.request_generation(inspecting ? inspect_generation : ledger_generation),
        .request_state(observe_state),.request_frame(current_frame),.request_epoch(observing ? graph_epoch : ledger_epoch),
        .response_valid(ownership_done),.response_error(ownership_error),.response_data(observed_cut),.busy(ownership_busy),.fault(media_fault),
        .capture_view0(unused_capture0),.capture_view1(unused_capture1),.playback_view0(unused_playback0),.playback_view1(unused_playback1),.*
    );
endmodule
