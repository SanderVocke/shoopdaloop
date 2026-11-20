module midi_job_port #(
    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8,
    parameter integer BLOCK_EVENTS=256, BLOCKS=4864, MAX_EVENTS=8192,
    parameter integer VIEWS=512, RETIREMENTS=256,
    parameter integer INPUT_EVENTS=256, MAINTENANCE=64,
    parameter integer TABLE_ENTRIES=(MAX_EVENTS+2*BLOCK_EVENTS+BLOCK_EVENTS-1)/BLOCK_EVENTS+1
) (
    output wire idle, input wire control_enable, input wire fault_request,
    output wire publication_valid, input wire publication_commit,
    input wire job_valid, output wire job_ready, input wire [255:0] job_data,
    output wire job_done, output wire [3:0] job_error,
    input wire [63:0] event_address, input wire [31:0] event_count,
    input wire clk, input wire reset, input wire [63:0] arena_base, input wire [31:0] quantum_frames,
    output wire [63:0] arena_bytes,
    input wire command_valid, output wire command_ready, input wire [5:0] command_op,
    input wire [63:0] command_arg0, input wire [63:0] command_arg1, input wire [63:0] command_arg2,
    input wire [63:0] command_arg3, input wire [63:0] command_arg4,
    output wire response_valid, input wire response_ready, output wire [3:0] response_error,
    output wire [63:0] response_handle, output wire [63:0] response_frame,
    output wire [31:0] response_count,
    output wire [23:0] response_packet, output wire [1:0] response_length,
    output wire packet_valid, input wire packet_ready, input wire packet_refused,
    output wire [23:0] packet_data, output wire [1:0] packet_length,
    output wire [31:0] packet_destination, output wire [31:0] packet_offset,
    output wire memory_valid, input wire memory_ready, output wire memory_write,
    output wire [63:0] memory_address, output wire [31:0] memory_write_data,
    input wire memory_response_valid, output wire memory_response_ready,
    input wire memory_response_error, input wire [31:0] memory_read_data
);
    reg pending, response_is_job, response_is_fault, fault_pending, fault_seen;
    reg response_publishes, publication_committed;
    wire service_ready, service_response;
    wire [3:0] service_error;
    wire [7:0] kind = job_data[7:0];
    wire [15:0] context_id = job_data[23:8], flags = job_data[39:24];
    wire [63:0] payload0 = job_data[127:64], payload1 = job_data[191:128];
    wire take_fault = fault_pending && !pending;
    wire take_job = job_valid && !pending && !fault_pending;
    wire take_control = command_valid && control_enable && !pending && !job_valid && !fault_pending;
    wire service_valid = take_fault || take_job || take_control;
    wire [5:0] service_op = take_fault ? 6'd18 : take_job ? 6'd24 + (6'(kind)-6'h20) : (command_op<24 ? command_op : 6'd63);
    wire [63:0] arg0 = take_job ? (kind==8'h20 ? payload0 : {48'b0,context_id}) : command_arg0;
    wire [63:0] arg1 = take_job ? (kind==8'h20 ? event_address : kind==8'h22 || kind==8'h25 || kind==8'h27 ? payload0 : {48'b0,flags}) : command_arg1;
    wire [63:0] arg2 = take_job ? (kind==8'h20 ? {32'b0,event_count} : kind==8'h25 || kind==8'h27 ? payload1 : 0) : command_arg2;
    wire [63:0] arg3 = take_job ? ((kind==8'h25 || kind==8'h27) ? {48'b0,flags} : 0) : command_arg3;
    wire [63:0] arg4 = take_job ? job_data[255:192] : command_arg4;
    wire publication_required = response_publishes && service_error==0;
    wire publication_complete = !publication_required || publication_committed;
    wire service_response_ready = response_is_job || response_is_fault || (response_ready && publication_complete);
    wire service_packet_valid;
    assign idle = !pending && !fault_pending && !fault_request;
    assign job_ready = service_ready && !pending && !fault_pending;
    assign command_ready = service_ready && control_enable && !pending && !job_valid && !fault_pending;
    assign job_done = service_response && response_is_job;
    assign job_error = service_error;
    assign publication_valid = service_response && !response_is_job && !response_is_fault && publication_required && !publication_committed;
    assign response_valid = service_response && !response_is_job && !response_is_fault && publication_complete;
    assign response_error = service_error;
    assign packet_valid = service_packet_valid && !(fault_request && response_is_job);
    always @(posedge clk) begin
        if(reset) begin pending<=0;response_is_job<=0;response_is_fault<=0;fault_pending<=0;fault_seen<=0;response_publishes<=0;publication_committed<=0;end
        else begin
            if(publication_valid && publication_commit) publication_committed<=1;
            if(!fault_request) fault_seen<=0;
            else if(!fault_seen) begin fault_pending<=1;fault_seen<=1;end
            if(service_valid && service_ready) begin
                pending<=1;response_is_job<=take_job;response_is_fault<=take_fault;
                publication_committed<=0;
                response_publishes<=take_control && (command_op==4 || command_op==5 || command_op==6 || command_op==7 || command_op==22 || command_op==23);
                if(take_fault) fault_pending<=0;
            end
            if(service_response && service_response_ready) pending<=0;
        end
    end
    midi_job_service #(.LOOPS(LOOPS),.INPUTS(INPUTS),.DESTINATIONS(DESTINATIONS),
        .BLOCK_EVENTS(BLOCK_EVENTS),.BLOCKS(BLOCKS),.MAX_EVENTS(MAX_EVENTS),
        .VIEWS(VIEWS),.RETIREMENTS(RETIREMENTS),
        .INPUT_EVENTS(INPUT_EVENTS),.MAINTENANCE(MAINTENANCE),.TABLE_ENTRIES(TABLE_ENTRIES)) service (
        .command_valid(service_valid),.command_ready(service_ready),.command_op(service_op),
        .command_arg0(take_fault ? 0 : arg0),.command_arg1(arg1),.command_arg2(arg2),.command_arg3(arg3),.command_arg4(arg4),
        .response_valid(service_response),.response_ready(service_response_ready),.response_error(service_error),
        .packet_valid(service_packet_valid),.packet_ready(packet_ready || (fault_request && response_is_job)),.packet_refused(packet_refused || (fault_request && response_is_job)),.*
    );
endmodule
