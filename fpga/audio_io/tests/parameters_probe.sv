module parameters_probe (
    input wire clk, reset, initialization_open,
    input wire request_valid, output wire request_ready,
    input wire [7:0] request_opcode, input wire request_state,
    input wire [15:0] request_address, input wire [639:0] request_data,
    output wire response_valid, output wire [3:0] response_error, output wire [128:0] response_data,
    output wire fault, output wire image_busy,
    input wire run_valid, output wire run_ready, output wire busy, done, output wire [7:0] error_code,
    input wire job_ready, output wire job_valid, output wire [255:0] job_data,
    input wire parameter_valid, output wire parameter_ready,
    input wire [7:0] parameter_opcode, input wire [15:0] parameter_key,
    input wire parameter_bank, input wire [63:0] parameter_nonce,
    input wire [31:0] parameter_count, input wire [15:0] parameter_address,
    input wire [128:0] parameter_value, input wire [31:0] parameter_target, parameter_duration,
    output wire parameter_busy, parameter_response_valid, output wire [3:0] parameter_response_error,
    output wire [128:0] parameter_response_data, output wire parameter_fault,
    output wire [31:0] smoothing_frames, output wire smoothing_linear
);
    wire iv,ir,is,rv;
    wire [7:0] io;
    wire [15:0] ia;
    wire [639:0] id;
    wire [3:0] re;
    wire [128:0] rd;
    audio_parameters #(.KEYS(8),.STATE_DEPTH(8)) parameters (
        .clk(clk),.reset(reset),.initialization_open(initialization_open),
        .request_valid(parameter_valid),.request_ready(parameter_ready),
        .request_opcode(parameter_opcode),.request_key(parameter_key),.request_bank(parameter_bank),
        .request_nonce(parameter_nonce),.request_count(parameter_count),.request_address(parameter_address),
        .request_value(parameter_value),.request_target(parameter_target),.request_duration(parameter_duration),
        .busy(parameter_busy),.response_valid(parameter_response_valid),.response_error(parameter_response_error),
        .response_data(parameter_response_data),.fault(parameter_fault),
        .smoothing_frames(smoothing_frames),.smoothing_linear(smoothing_linear),
        .image_valid(iv),.image_ready(ir && parameter_busy),.image_opcode(io),.image_state(is),
        .image_address(ia),.image_data(id),.image_response_valid(rv),.image_response_error(re),.image_response_data(rd)
    );
    assign request_ready=ir && !parameter_busy;
    assign response_valid=rv && !parameter_busy;
    assign response_error=re;
    assign response_data=rd;
    image_probe #(.BANKS(2),.CANONICAL(1)) image (
        .clk(clk),.reset(reset),.request_valid(parameter_busy ? iv : request_valid),.request_ready(ir),
        .request_opcode(parameter_busy ? io : request_opcode),.request_state(parameter_busy ? is : request_state),
        .request_address(parameter_busy ? ia : request_address),.request_data(parameter_busy ? id : request_data),
        .response_valid(rv),.response_error(re),.response_data(rd),.fault(fault),.image_busy(image_busy),
        .run_valid(run_valid),.run_ready(run_ready),.busy(busy),.done(done),.error_code(error_code),
        .job_ready(job_ready),.job_valid(job_valid),.job_data(job_data)
    );
endmodule
