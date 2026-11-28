module fifo_media_arbiter #(
    parameter integer MAX_WORDS=32
) (
    input wire clk, reset,
    input wire processing_due, frontend_busy, memory_fault,
    output wire faulted,
    input wire fence_request,
    input wire [31:0] pending_epoch, input wire [63:0] pending_sequence,
    output wire fence_response_valid, fence_response_error,
    output wire [31:0] fence_response_epoch,
    output wire [63:0] fence_response_sequence,
    input wire loader_cmd_valid, output wire loader_cmd_ready,
    input wire [3:0] loader_cmd_opcode,
    input wire [63:0] loader_cmd_view, loader_cmd_start, loader_cmd_length,
    input wire [31:0] loader_cmd_count,
    input wire loader_write_valid, output wire loader_write_ready,
    input wire [31:0] loader_write_data,
    output wire loader_read_valid, input wire loader_read_ready,
    output wire [31:0] loader_read_data,
    output wire loader_response_valid, input wire loader_response_ready,
    output wire [7:0] loader_response_error,
    output wire [31:0] loader_result_view, loader_result_count,
    output wire loader_result_empty,
    input wire job_cmd_valid, output wire job_cmd_ready,
    input wire [3:0] job_opcode,
    input wire [63:0] job_view, job_start, job_length,
    input wire [31:0] job_count,
    input wire job_write_valid, output wire job_write_ready,
    input wire [31:0] job_write_data,
    output wire job_read_valid, input wire job_read_ready,
    output wire [31:0] job_read_data,
    output wire job_done, output wire [7:0] job_error,
    output wire operation_valid, input wire operation_ready,
    output wire [3:0] operation_opcode,
    output wire [63:0] operation_view, operation_start, operation_length,
    output wire [31:0] operation_count,
    output wire operation_write_valid, input wire operation_write_ready,
    output wire [31:0] operation_write_data,
    input wire operation_read_valid, output wire operation_read_ready,
    input wire [31:0] operation_read_data,
    input wire operation_done, input wire [7:0] operation_error,
    input wire [31:0] operation_result_view, input wire operation_result_empty
);
    typedef enum logic [1:0] {NONE, JOB, LOADER, FENCE} owner_t;
    owner_t owner;
    reg fence_issued, retained_fault;
    reg [31:0] fence_epoch;
    reg [63:0] fence_sequence;
    wire loader_fault;
    wire load_valid, load_ready, load_write_valid, load_write_ready, load_read_valid, load_read_ready;
    wire [3:0] load_opcode;
    wire [63:0] load_view, load_start, load_length;
    wire [31:0] load_count, load_write_data;
    wire selected_job=job_cmd_valid;
    wire selected_fence=!selected_job && fence_request && !fence_issued;
    wire selected_loader=!selected_job && !selected_fence && !processing_due
        && !frontend_busy && !fence_request && load_valid;
    wire fence_failure=owner==FENCE && operation_done && operation_error!=0;
    assign faulted=retained_fault || loader_fault || memory_fault || fence_failure;
    assign operation_valid=owner==NONE && !faulted && !reset
        && (selected_job || selected_fence || selected_loader);
    assign operation_opcode=selected_job ? job_opcode : selected_fence ? 4'd7 : load_opcode;
    assign operation_view=selected_job ? job_view : selected_fence ? 64'd0 : load_view;
    assign operation_start=selected_job ? job_start : selected_fence ? 64'd0 : load_start;
    assign operation_length=selected_job ? job_length : selected_fence ? 64'd0 : load_length;
    assign operation_count=selected_job ? job_count : selected_fence ? 32'd0 : load_count;
    assign job_cmd_ready=operation_valid && selected_job && operation_ready;
    assign load_ready=operation_valid && selected_loader && operation_ready;
    assign operation_write_valid=!reset && (owner==JOB ? job_write_valid : owner==LOADER && load_write_valid);
    assign operation_write_data=owner==JOB ? job_write_data : load_write_data;
    assign job_write_ready=owner==JOB && operation_write_ready;
    assign load_write_ready=owner==LOADER && operation_write_ready;
    assign operation_read_ready=!reset && (owner==JOB ? job_read_ready : owner==LOADER && load_read_ready);
    assign job_read_valid=owner==JOB && operation_read_valid;
    assign job_read_data=operation_read_data;
    assign load_read_valid=owner==LOADER && operation_read_valid;
    assign job_done=owner==JOB && operation_done;
    assign job_error=owner==JOB ? operation_error : 8'd0;
    assign fence_response_valid=owner==FENCE && operation_done;
    assign fence_response_error=operation_error!=0 || faulted;
    assign fence_response_epoch=fence_epoch;
    assign fence_response_sequence=fence_sequence;
    always @(posedge clk) begin
        if (reset) begin
            owner<=NONE; fence_issued<=0; fence_epoch<=0; fence_sequence<=0; retained_fault<=0;
        end else begin
            if (!fence_request) fence_issued<=0;
            if (operation_valid && operation_ready) begin
                owner<=selected_job ? JOB : selected_fence ? FENCE : LOADER;
                if (selected_fence) begin
                    fence_issued<=1; fence_epoch<=pending_epoch; fence_sequence<=pending_sequence;
                end
            end
            if (operation_done && owner!=NONE) begin
                owner<=NONE;
                if (operation_error==5 || fence_failure) retained_fault<=1;
            end
        end
    end
    fifo_media_loader #(.MAX_WORDS(MAX_WORDS)) loader (
        .clk(clk), .reset(reset),
        .cmd_valid(loader_cmd_valid), .cmd_ready(loader_cmd_ready), .cmd_opcode(loader_cmd_opcode),
        .cmd_view(loader_cmd_view), .cmd_start(loader_cmd_start), .cmd_length(loader_cmd_length), .cmd_count(loader_cmd_count),
        .write_valid(loader_write_valid), .write_ready(loader_write_ready), .write_data(loader_write_data),
        .read_valid(loader_read_valid), .read_ready(loader_read_ready), .read_data(loader_read_data),
        .response_valid(loader_response_valid), .response_ready(loader_response_ready), .response_error(loader_response_error),
        .result_view(loader_result_view), .result_empty(loader_result_empty), .result_count(loader_result_count), .faulted(loader_fault),
        .backend_valid(load_valid), .backend_ready(load_ready), .backend_opcode(load_opcode),
        .backend_view(load_view), .backend_start(load_start), .backend_length(load_length), .backend_count(load_count),
        .backend_write_valid(load_write_valid), .backend_write_ready(load_write_ready), .backend_write_data(load_write_data),
        .backend_read_valid(load_read_valid), .backend_read_ready(load_read_ready), .backend_read_data(operation_read_data),
        .backend_done(owner==LOADER && operation_done), .backend_error(operation_error),
        .backend_result_view(operation_result_view), .backend_result_empty(operation_result_empty)
    );
endmodule
