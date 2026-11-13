`include "state_processor_defs.svh"

module job_processor #(
    parameter integer EXTERNAL_IO = 0,
    parameter integer MAX_FRAMES = 32,
    parameter integer BUFFERS = 16,
    parameter integer INPUT_PORTS = 2,
    parameter integer OUTPUT_PORTS = 2,
    parameter integer CHUNK_FRAMES = 1,
    parameter integer ENABLE_MEDIA = 0,
    parameter integer BUFFER_BITS = (BUFFERS > 1) ? $clog2(BUFFERS) : 1,
    parameter integer SCRATCH_BITS = (BUFFERS * MAX_FRAMES > 1) ? $clog2(BUFFERS * MAX_FRAMES) : 1,
    parameter integer INPUT_BITS = (INPUT_PORTS * MAX_FRAMES > 1) ? $clog2(INPUT_PORTS * MAX_FRAMES) : 1,
    parameter integer OUTPUT_BITS = (OUTPUT_PORTS * MAX_FRAMES > 1) ? $clog2(OUTPUT_PORTS * MAX_FRAMES) : 1
) (
    output wire sample_input_read,
    output wire [31:0] sample_input_address,
    input wire [31:0] sample_input_data,
    output wire sample_output_write,
    output wire [31:0] sample_output_address,
    output wire [31:0] sample_output_data,
    input wire clk,
    input wire reset,
    input wire run_valid,
    output wire run_ready,
    input wire [31:0] run_quantum,
    input wire [31:0] run_chunk_frames,
    input wire [31:0] run_buffers,
    input wire [31:0] run_inputs,
    input wire [31:0] run_outputs,
    input wire [31:0] run_jobs,
    output wire busy,
    output reg done,
    output reg [7:0] error_code,
    output reg [BUFFERS-1:0] buffer_ready,
    input wire job_valid,
    output wire job_ready,
    input wire job_enable,
    input wire [`SHOOP_SP_JOB_BITS-1:0] job_data,
    input wire cfg_valid,
    output wire cfg_ready,
    input wire [1:0] cfg_space,
    input wire cfg_write,
    input wire [31:0] cfg_address,
    input wire [31:0] cfg_write_data,
    output wire cfg_response_valid,
    output wire cfg_response_error,
    output wire [31:0] cfg_read_data,
    output wire media_cmd_valid, input wire media_cmd_ready,
    output wire [3:0] media_cmd_opcode, output wire [63:0] media_cmd_view,
    output wire [63:0] media_cmd_start, output wire [63:0] media_cmd_length,
    output wire [31:0] media_cmd_count,
    input wire media_done, input wire [7:0] media_error, input wire media_fault,
    output wire media_write_valid, input wire media_write_ready, output wire [31:0] media_write_data,
    input wire media_read_valid, output wire media_read_ready, input wire [31:0] media_read_data,
    output wire media_hint_valid, input wire media_hint_ready,
    output reg [63:0] media_hint_view, output reg [63:0] media_hint_position, output reg [63:0] media_hint_length
);
    localparam [7:0] OK = 0, INVALID_BUFFER = 1, INVALID_SHAPE = 2, UNSUPPORTED = 3, INVALID_JOB = 4;
    localparam integer FRAME_BITS = $clog2(MAX_FRAMES+1);
    localparam integer BUFFER_COUNT_BITS = $clog2(64'(BUFFERS)+1);
    localparam integer INPUT_COUNT_BITS = $clog2(64'(INPUT_PORTS)+1);
    localparam integer OUTPUT_COUNT_BITS = $clog2(64'(OUTPUT_PORTS)+1);
    typedef enum logic [5:0] {
        IDLE, CLEAR_OUTPUT, FETCH, DECODE, DISPATCH, READ_REQUEST, READ_CAPTURE, READ_RESULT,
        RAMP_PREPARE, RAMP_OPERAND, RAMP_MULTIPLY, RAMP_PRODUCT,
        INITIAL_DIVIDE, INITIAL_WAIT, STEP_DIVIDE, STEP_WAIT,
        STORE, GAIN_STREAM, MIX_STREAM,
        ARITH_B_REQUEST, ARITH_B_CAPTURE, ARITH_B_RESULT, ARITH_OPERAND, ARITH_MULTIPLY, ARITH_PRODUCT, ARITH_RESULT,
        SMOOTH_LOAD, SMOOTH_PREPARE, SMOOTH_OPERAND, SMOOTH_MULTIPLY, SMOOTH_PRODUCT, SMOOTH_RESULT, SMOOTH_CLAMP,
        MEDIA_COMMAND, MEDIA_READ, MEDIA_SOURCE_REQUEST, MEDIA_SOURCE_RESULT, MEDIA_WRITE, MEDIA_DONE
    } phase_t;
    localparam [5:0] CHUNK_SHIFT = shift_for(32'(CHUNK_FRAMES));
    reg media_shape_valid;
    function automatic [5:0] shift_for(input [31:0] frames);
        begin
            shift_for = 0;
            for (integer bit_index=0; bit_index<32; bit_index=bit_index+1)
                if (frames[bit_index]) shift_for = 6'(bit_index);
        end
    endfunction
    phase_t phase;
    wire media_active = phase == MEDIA_COMMAND || phase == MEDIA_READ || phase == MEDIA_SOURCE_REQUEST
        || phase == MEDIA_SOURCE_RESULT || phase == MEDIA_WRITE || phase == MEDIA_DONE;
    reg [FRAME_BITS-1:0] quantum, frame, stream_frame;
    reg [BUFFER_COUNT_BITS-1:0] logical_buffers;
    reg [INPUT_COUNT_BITS-1:0] input_count;
    reg [OUTPUT_COUNT_BITS-1:0] output_count, clear_port;
    reg [31:0] remaining_jobs;
    reg jobs_pending;
    reg [`SHOOP_SP_JOB_BITS-1:0] packet;
    wire [7:0] opcode = packet[7:0];
    wire [15:0] id0 = packet[23:8], id1 = packet[39:24];
    wire unused_packet = &{1'b0, packet[63:40], packet[255:192]};
    wire unused_media = &{1'b0, media_cmd_ready, media_done, media_error, media_write_ready, media_read_valid, media_read_data};
    reg [15:0] source_a, source_b, destination, port_index;
    reg [31:0] sample_a, result_sample;
    reg [31:0] gain_read_gain;
    reg source0_valid, source1_valid, destination0_valid, destination1_valid, payload_destination_valid;
    reg input_port_valid, output_port_valid, history_index_valid, arithmetic_mode_valid;
    reg gain_read_valid, gain_sample_valid, gain_operand_valid, gain_multiply_valid, gain_product_valid, stream_store_valid;
    reg mix_second, mix_read_valid, mix_read_second, mix_sample_valid, mix_sample_second;
    reg [31:0] sample_operand, stream_sample, stream_gain;
    reg [31:0] current_gain, ramp_start, ramp_target, duration, elapsed;
    reg [31:0] fraction, fraction_limit, step_gain, step_gain_extra, step_remainder, delta_magnitude;
    reg ramp_negative, constant_gain;
    wire signed [32:0] delta = $signed({packet[127], packet[127:96]})
        - $signed({packet[95], packet[95:64]});
    wire carry_fraction = fraction >= fraction_limit;
    wire [31:0] next_fraction = carry_fraction ? fraction-fraction_limit : fraction+step_remainder;
    (* KEEP = "yes" *) wire [31:0] gain_step = current_gain+step_gain;
    (* KEEP = "yes" *) wire [31:0] gain_step_extra = current_gain+step_gain_extra;
    (* DONT_TOUCH = "yes" *) reg [63:0] product;
    reg product_negative;
    reg hint_pending, hinted_head;
    assign media_hint_valid = (ENABLE_MEDIA != 0) && hint_pending && !media_fault && !reset;
    reg [BUFFERS-1:0] smooth_valid;
    wire [127:0] smooth_history;
    reg [BUFFER_BITS-1:0] smooth_index;
    reg [31:0] anchor_value;
    (* DONT_TOUCH = "yes" *) reg [31:0] multiply_a, multiply_b;
    reg multiply_negative;
    reg signed [32:0] ramp_delta;
    reg signed [32:0] smooth_difference;
    reg signed [41:0] smooth_value;
    reg [31:0] arithmetic_factor;
    (* DONT_TOUCH = "yes" *) reg [31:0] partial_ll, partial_lh, partial_hl, partial_hh;
    wire [63:0] multiplied = {32'b0,partial_ll} + {16'b0,partial_lh,16'b0}
        + {16'b0,partial_hl,16'b0} + {partial_hh,32'b0};
    reg multiplied_negative;
    always @(posedge clk) begin
        if (reset) begin
            partial_ll <= 0; partial_lh <= 0; partial_hl <= 0; partial_hh <= 0;
            multiplied_negative <= 0;
        end
        else if (phase == RAMP_MULTIPLY || phase == SMOOTH_MULTIPLY || phase == ARITH_MULTIPLY
            || (phase == GAIN_STREAM && gain_operand_valid)) begin
            partial_ll <= multiply_a[15:0] * multiply_b[15:0];
            partial_lh <= multiply_a[15:0] * multiply_b[31:16];
            partial_hl <= multiply_a[31:16] * multiply_b[15:0];
            partial_hh <= multiply_a[31:16] * multiply_b[31:16];
            multiplied_negative <= multiply_negative;
        end
    end
    wire divide_done;
    wire [63:0] divide_quotient;
    wire [31:0] divide_remainder;
    wire unused_divide_high = &{1'b0, divide_quotient[63:32]};
    reg cfg_pending, cfg_error, cfg_was_write;
    reg [1:0] cfg_response_space;
    wire cfg_accept = cfg_valid && cfg_ready;
    wire cfg_shape_in_range = (cfg_space == 0) ? cfg_address < 32'(INPUT_PORTS * MAX_FRAMES)
        : (cfg_space == 1) ? cfg_address < 32'(OUTPUT_PORTS * MAX_FRAMES)
        : (cfg_space == 2) && (cfg_address < 32'(BUFFERS * MAX_FRAMES));
    wire cfg_in_range = cfg_shape_in_range && ((EXTERNAL_IO == 0) || cfg_space == 2);
    wire [31:0] input_data, output_data, scratch_data;
    wire [31:0] local_input_data;
    assign input_data = EXTERNAL_IO != 0 ? sample_input_data : local_input_data;
    assign sample_input_read = EXTERNAL_IO != 0 && input_read && !reset;
    assign sample_input_address = 32'(port_index)*MAX_FRAMES+32'(frame);
    assign sample_output_write = EXTERNAL_IO != 0 && output_write && !reset
        && !((ENABLE_MEDIA != 0) && media_fault);
    assign sample_output_address = 32'(output_address);
    assign sample_output_data = phase == CLEAR_OUTPUT ? 32'b0 : result_sample;
    wire scratch_access = cfg_accept && cfg_in_range && (cfg_space == 2);
    wire input_read = (phase == READ_REQUEST) && (opcode == `SHOOP_SP_JOB_INPUT);
    wire streaming = phase == GAIN_STREAM || phase == MIX_STREAM;
    wire stream_read = streaming && frame < quantum;
    wire stream_write = streaming && stream_store_valid && !((ENABLE_MEDIA != 0) && media_fault);
    wire playback_write = (ENABLE_MEDIA != 0) && phase == MEDIA_READ
        && media_read_valid && media_read_ready && !media_fault;
    wire scratch_read = ((phase == READ_REQUEST) && (opcode != `SHOOP_SP_JOB_INPUT))
        || stream_read || (phase == MEDIA_SOURCE_REQUEST) || (phase == ARITH_B_REQUEST);
    wire scratch_write = ((phase == STORE) && (opcode != `SHOOP_SP_JOB_OUTPUT)) || stream_write || playback_write;
    wire output_write = (phase == CLEAR_OUTPUT) || ((phase == STORE) && (opcode == `SHOOP_SP_JOB_OUTPUT));
    wire [SCRATCH_BITS-1:0] scratch_read_address = SCRATCH_BITS'(32'(((phase == MIX_STREAM && mix_second) || phase == ARITH_B_REQUEST) ? source_b : source_a)
        * 32'(MAX_FRAMES) + frame);
    wire [SCRATCH_BITS-1:0] scratch_write_address = SCRATCH_BITS'(32'(destination) * 32'(MAX_FRAMES)
        + (streaming ? 32'(stream_frame) : 32'(frame)));
    wire [OUTPUT_BITS-1:0] output_address = OUTPUT_BITS'(((phase == CLEAR_OUTPUT) ? 32'(clear_port) : 32'(port_index))
        * 32'(MAX_FRAMES) + frame);
    wire [SCRATCH_BITS-1:0] scratch_ram_read_address = busy ? SCRATCH_BITS'(scratch_read_address) : SCRATCH_BITS'(cfg_address);
    wire [SCRATCH_BITS-1:0] scratch_ram_write_address = busy ? SCRATCH_BITS'(scratch_write_address) : SCRATCH_BITS'(cfg_address);

    function automatic source_valid(input [15:0] index);
        source_valid = (32'(index) < 32'(logical_buffers)) && buffer_ready[BUFFER_BITS'(index)];
    endfunction
    function automatic destination_valid(input [15:0] index);
        destination_valid = (32'(index) < 32'(logical_buffers)) && !buffer_ready[BUFFER_BITS'(index)];
    endfunction
    function automatic [31:0] ramp_value(input [31:0] start_value, input negative, input [31:0] change);
        reg signed [33:0] value;
        begin
            value = $signed({{2{start_value[31]}}, start_value});
            value = negative ? value - $signed({2'b0, change}) : value + $signed({2'b0, change});
            ramp_value = value[31:0];
        end
    endfunction
    function automatic [31:0] saturating_mix(input [31:0] left, input [31:0] right);
        reg signed [32:0] sum;
        begin
            sum = $signed({left[31], left}) + $signed({right[31], right});
            saturating_mix = (sum > 33'sh07fffffff) ? 32'h7fffffff
                : (sum < -33'sh080000000) ? 32'h80000000 : sum[31:0];
        end
    endfunction
    function automatic [31:0] scaled_product(input [39:0] scaled, input negative);
        begin
            if (negative)
                scaled_product = (scaled >= 40'h0080000000) ? 32'h80000000 : -scaled[31:0];
            else
                scaled_product = (scaled > 40'h007fffffff) ? 32'h7fffffff : scaled[31:0];
        end
    endfunction

    initial begin
        if ((MAX_FRAMES < 1) || (MAX_FRAMES > 65536) || (BUFFERS < 1) || (BUFFERS > 65536)
            || (INPUT_PORTS < 1) || (OUTPUT_PORTS < 1))
            $fatal(1, "Invalid job processor capacities");
    end
    assign busy = phase != IDLE;
    assign cfg_ready = !busy && !cfg_pending && !reset;
    assign run_ready = !busy && !cfg_pending && !cfg_valid && !reset;
    assign job_ready = (phase == FETCH) && jobs_pending && job_enable && !reset && ((ENABLE_MEDIA == 0) || !media_fault);
    assign cfg_response_valid = cfg_pending && !reset;
    assign cfg_response_error = cfg_error;
    assign cfg_read_data = (cfg_error || cfg_was_write) ? 32'b0
        : (cfg_response_space == 0) ? input_data : (cfg_response_space == 1) ? output_data : scratch_data;
    assign media_cmd_valid = (ENABLE_MEDIA != 0) && (phase == MEDIA_COMMAND) && !reset;
    assign media_cmd_opcode = (ENABLE_MEDIA == 0) ? 4'b0 : (opcode == `SHOOP_SP_JOB_BEGIN_CAPTURE) ? 4'd8 : (opcode == `SHOOP_SP_JOB_READ) ? 4'd6 : 4'd5;
    assign media_cmd_view = (ENABLE_MEDIA == 0) ? 64'b0 : packet[127:64];
    assign media_cmd_start = (ENABLE_MEDIA == 0) ? 64'b0 : packet[191:128] << CHUNK_SHIFT;
    assign media_cmd_length = (ENABLE_MEDIA == 0) ? 64'b0 : packet[255:192] << CHUNK_SHIFT;
    assign media_cmd_count = (ENABLE_MEDIA == 0) ? 32'b0 : 32'(quantum);
    wire media_range_overflow;
    generate if (CHUNK_SHIFT == 0) begin : unshifted_media
        assign media_range_overflow = 0;
    end else begin : shifted_media
        assign media_range_overflow = (|packet[191 -: CHUNK_SHIFT]) || (|packet[255 -: CHUNK_SHIFT]);
    end endgenerate
    assign media_write_valid = (ENABLE_MEDIA != 0) && (phase == MEDIA_WRITE) && !reset;
    assign media_write_data = (ENABLE_MEDIA == 0) ? 32'b0 : sample_a;
    assign media_read_ready = (ENABLE_MEDIA != 0) && (phase == MEDIA_READ) && !reset;

    jp_divider divide (
        .clk(clk), .reset(reset), .start((phase == INITIAL_DIVIDE) || (phase == STEP_DIVIDE)),
        .numerator((phase == STEP_DIVIDE) ? {32'b0, delta_magnitude} : product),
        .denominator(duration), .done(divide_done), .quotient(divide_quotient), .remainder(divide_remainder)
    );
    generate if (EXTERNAL_IO == 0) begin : local_io
        wire input_access = cfg_accept && cfg_in_range && (cfg_space == 0);
        wire output_access = cfg_accept && cfg_in_range && (cfg_space == 1);
        wire [INPUT_BITS-1:0] input_address = INPUT_BITS'(32'(port_index) * 32'(MAX_FRAMES) + frame);
        wire [INPUT_BITS-1:0] input_ram_read_address = busy ? INPUT_BITS'(input_address) : INPUT_BITS'(cfg_address);
        wire [OUTPUT_BITS-1:0] output_ram_write_address = busy ? OUTPUT_BITS'(output_address) : OUTPUT_BITS'(cfg_address);
    jp_sample_ram #(.DEPTH(INPUT_PORTS * MAX_FRAMES), .ADDR_BITS(INPUT_BITS)) inputs_memory (
        .clk(clk), .read_enable(!reset && (input_read || (input_access && !cfg_write))),
        .read_address(input_ram_read_address), .read_data(local_input_data),
        .write_enable(input_access && cfg_write), .write_address(INPUT_BITS'(cfg_address)), .write_data(cfg_write_data)
    );
    jp_sample_ram #(.DEPTH(OUTPUT_PORTS * MAX_FRAMES), .ADDR_BITS(OUTPUT_BITS)) outputs_memory (
        .clk(clk), .read_enable(output_access && !cfg_write),
        .read_address(OUTPUT_BITS'(cfg_address)), .read_data(output_data),
        .write_enable(!reset && (output_write || (output_access && cfg_write))),
        .write_address(output_ram_write_address),
        .write_data(busy ? ((phase == CLEAR_OUTPUT) ? 32'b0 : result_sample) : cfg_write_data)
    );
    end else begin : external_io
        assign local_input_data = 0;
        assign output_data = 0;
    end endgenerate
    jp_sample_ram #(.DEPTH(BUFFERS * MAX_FRAMES), .ADDR_BITS(SCRATCH_BITS)) scratch_memory (
        .clk(clk), .read_enable(!reset && (scratch_read || (scratch_access && !cfg_write))),
        .read_address(scratch_ram_read_address), .read_data(scratch_data),
        .write_enable(!reset && (scratch_write || (scratch_access && cfg_write))),
        .write_address(scratch_ram_write_address),
        .write_data(busy ? (playback_write ? media_read_data : result_sample) : cfg_write_data)
    );

    wire smooth_commit = phase == STORE && opcode == `SHOOP_SP_JOB_SMOOTHING
        && frame + 1 == quantum && !reset && !(ENABLE_MEDIA != 0 && media_fault);
    wire [31:0] smooth_next_elapsed = elapsed < duration ? elapsed + 1 : elapsed;
    sp_ram #(.WIDTH(128), .DEPTH(BUFFERS), .ADDR_BITS(BUFFER_BITS)) smoothing_memory (
        .clk(clk), .read_enable(phase == DECODE && opcode == `SHOOP_SP_JOB_SMOOTHING),
        .read_address(BUFFER_BITS'(packet[95:64])), .read_data(smooth_history),
        .write_enable(smooth_commit), .write_address(smooth_index),
        .write_data({result_sample, anchor_value, packet[128] ? smooth_next_elapsed : elapsed, duration})
    );

    always @(posedge clk) begin
        if (reset) begin
            hint_pending <= 0; hinted_head <= 0;
            media_hint_view <= 0; media_hint_position <= 0; media_hint_length <= 0;
        end else if (media_fault && ENABLE_MEDIA != 0) begin hint_pending <= 0; hinted_head <= 0; end
        else begin
            if (!hint_pending) begin
                media_hint_view <= job_data[127:64]; media_hint_position <= job_data[191:128]; media_hint_length <= job_data[255:192];
            end
            if (media_hint_valid && media_hint_ready) hint_pending <= 0;
            if (job_valid && job_ready) hinted_head <= 0;
            if (ENABLE_MEDIA != 0 && !hint_pending && !hinted_head && phase != IDLE && phase != FETCH
                && job_valid && job_data[7:0] == `SHOOP_SP_JOB_READ) begin
                hint_pending <= 1; hinted_head <= 1;
            end
        end
    end

    always @(posedge clk) begin
        if (reset) begin
            cfg_pending <= 0;
            cfg_error <= 0;
            cfg_was_write <= 0;
            cfg_response_space <= 0;
        end else begin
            cfg_pending <= cfg_accept;
            if (cfg_accept) begin
                cfg_error <= !cfg_in_range;
                cfg_was_write <= cfg_write;
                cfg_response_space <= cfg_space;
            end
        end
    end

    always @(posedge clk) begin
        if (reset) begin
            phase <= IDLE;
            smooth_valid <= 0;
            smooth_index<=0;anchor_value<=0;
            done <= 0;
            error_code <= OK;
            buffer_ready <= 0;
            quantum <= 0; media_shape_valid <= 0;
            logical_buffers <= 0;
            input_count <= 0;
            output_count <= 0;
            remaining_jobs <= 0;
            jobs_pending <= 0;
            frame <= 0;
            clear_port <= 0;
            packet <= 0;
            source_a <= 0;
            source_b <= 0;
            destination <= 0;
            port_index <= 0;
            sample_a <= 0;
            result_sample <= 0;
            stream_frame <= 0; gain_read_gain <= 0;
            gain_read_valid <= 0; gain_sample_valid <= 0; gain_operand_valid <= 0; gain_multiply_valid <= 0; gain_product_valid <= 0; stream_store_valid <= 0;
            sample_operand <= 0; stream_sample <= 0; stream_gain <= 0;
            multiply_a <= 0; multiply_b <= 0; multiply_negative <= 0;
            smooth_difference <= 0; arithmetic_factor <= 0;
            smooth_value <= 0;
            source0_valid <= 0; source1_valid <= 0; destination0_valid <= 0; destination1_valid <= 0;
            payload_destination_valid <= 0; input_port_valid <= 0; output_port_valid <= 0;
            history_index_valid <= 0; arithmetic_mode_valid <= 0;
            ramp_delta <= 0;
            mix_second <= 0; mix_read_valid <= 0; mix_read_second <= 0; mix_sample_valid <= 0; mix_sample_second <= 0;
            current_gain <= 0;
            ramp_start <= 0;
            ramp_target <= 0;
            duration <= 0;
            elapsed <= 0;
            fraction <= 0;
            fraction_limit <= 0;
            step_gain <= 0; step_gain_extra <= 0;
            step_remainder <= 0;
            delta_magnitude <= 0;
            ramp_negative <= 0;
            constant_gain <= 0;
            product <= 0;
            product_negative <= 0;
        end else begin
            done <= 0;
            gain_read_valid <= 0; gain_sample_valid <= 0; gain_operand_valid <= 0; gain_multiply_valid <= 0; gain_product_valid <= 0;
            mix_read_valid <= 0; mix_sample_valid <= 0; stream_store_valid <= 0;
            if ((ENABLE_MEDIA != 0) && media_fault && phase != IDLE) begin
                error_code <= 8; done <= 1; phase <= IDLE;
            end else if ((ENABLE_MEDIA != 0) && media_done && media_active) begin
                if (media_error != 0) begin
                    case (media_error)
                        1: error_code <= 5;
                        2: error_code <= 7;
                        3: error_code <= 6;
                        4: error_code <= INVALID_SHAPE;
                        default: error_code <= 8;
                    endcase
                    done <= 1;
                    phase <= IDLE;
                end else phase <= FETCH;
            end else case (phase)
                IDLE: if (run_valid && run_ready) begin
                    error_code <= OK;
                    if ((run_quantum == 0) || (run_quantum > 32'(MAX_FRAMES))
                        || (run_buffers > 32'(BUFFERS)) || (run_inputs > 32'(INPUT_PORTS))
                        || (run_outputs > 32'(OUTPUT_PORTS))) begin
                        error_code <= INVALID_SHAPE;
                        done <= 1;
                    end else begin
                        quantum <= FRAME_BITS'(run_quantum);
                        media_shape_valid <= run_chunk_frames == 32'(CHUNK_FRAMES);
                        logical_buffers <= BUFFER_COUNT_BITS'(run_buffers);
                        input_count <= INPUT_COUNT_BITS'(run_inputs);
                        output_count <= OUTPUT_COUNT_BITS'(run_outputs);
                        remaining_jobs <= run_jobs;
                        jobs_pending <= run_jobs != 0;
                        buffer_ready <= 0;
                        frame <= 0;
                        clear_port <= 0;
                        phase <= (run_outputs == 0) ? FETCH : CLEAR_OUTPUT;
                    end
                end
                CLEAR_OUTPUT: begin
                    if (frame + 1 == quantum) begin
                        frame <= 0;
                        if (clear_port + 1 == output_count)
                            phase <= FETCH;
                        else
                            clear_port <= clear_port + 1;
                    end else
                        frame <= frame + 1;
                end
                FETCH: begin
                    if (!jobs_pending) begin
                        done <= 1;
                        phase <= IDLE;
                    end else if (job_valid && job_ready) begin
                        packet <= job_data;
                        remaining_jobs <= remaining_jobs - 1;
                        jobs_pending <= remaining_jobs > 1;
                        phase <= DECODE;
                    end
                end
                DECODE: begin
                    source0_valid <= source_valid(id0); source1_valid <= source_valid(id1);
                    destination0_valid <= destination_valid(id0); destination1_valid <= destination_valid(id1);
                    payload_destination_valid <= destination_valid(packet[79:64]);
                    input_port_valid <= 32'(id0) < 32'(input_count);
                    output_port_valid <= 32'(id1) < 32'(output_count);
                    history_index_valid <= packet[95:64] < 32'(BUFFERS);
                    arithmetic_mode_valid <= packet[95:80] <= 3;
                    phase <= DISPATCH;
                end
                DISPATCH: begin
                    frame <= 0;
                    stream_frame <= 0; mix_second <= 0;
                    case (opcode)
                        `SHOOP_SP_JOB_INPUT: begin
                            if (!destination1_valid || !input_port_valid) begin
                                error_code <= INVALID_BUFFER;
                                done <= 1;
                                phase <= IDLE;
                            end else begin
                                port_index <= id0;
                                destination <= id1;
                                phase <= READ_REQUEST;
                            end
                        end
                        `SHOOP_SP_JOB_CONSTANT, `SHOOP_SP_JOB_SILENCE: begin
                            if (!destination0_valid) begin
                                error_code <= INVALID_BUFFER;
                                done <= 1;
                                phase <= IDLE;
                            end else begin
                                destination <= id0;
                                result_sample <= opcode == `SHOOP_SP_JOB_CONSTANT ? packet[95:64] : 0;
                                phase <= STORE;
                            end
                        end
                        `SHOOP_SP_JOB_SMOOTHING: begin
                            if (!source0_valid || !destination1_valid || !history_index_valid) begin
                                error_code<=INVALID_BUFFER;done<=1;phase<=IDLE;
                            end else begin
                                source_a<=id0;destination<=id1;smooth_index<=BUFFER_BITS'(packet[95:64]);
                                phase <= SMOOTH_LOAD;
                            end
                        end
                        `SHOOP_SP_JOB_GAIN: begin
                            if (!source0_valid || !destination1_valid) begin
                                error_code <= INVALID_BUFFER;
                                done <= 1;
                                phase <= IDLE;
                            end else begin
                                source_a <= id0;
                                destination <= id1;
                                phase <= RAMP_PREPARE;
                            end
                        end
                        `SHOOP_SP_JOB_ARITHMETIC: begin
                            if (!source0_valid || !source1_valid || !payload_destination_valid || !arithmetic_mode_valid) begin
                                error_code <= INVALID_BUFFER; done <= 1; phase <= IDLE;
                            end else begin
                                source_a <= id0; source_b <= id1; destination <= packet[79:64]; phase <= READ_REQUEST;
                            end
                        end
                        `SHOOP_SP_JOB_MIX: begin
                            if (!source0_valid || !source1_valid || !payload_destination_valid) begin
                                error_code <= INVALID_BUFFER;
                                done <= 1;
                                phase <= IDLE;
                            end else begin
                                source_a <= id0;
                                source_b <= id1;
                                destination <= packet[79:64];
                                phase <= MIX_STREAM;
                            end
                        end
                        `SHOOP_SP_JOB_OUTPUT: begin
                            if (!source0_valid || !output_port_valid) begin
                                error_code <= INVALID_BUFFER;
                                done <= 1;
                                phase <= IDLE;
                            end else begin
                                source_a <= id0;
                                port_index <= id1;
                                phase <= READ_REQUEST;
                            end
                        end
                        `SHOOP_SP_JOB_READ: begin
                            if (ENABLE_MEDIA == 0) begin
                                error_code <= UNSUPPORTED; done <= 1; phase <= IDLE;
                            end else if (!media_shape_valid) begin
                                error_code <= INVALID_SHAPE; done <= 1; phase <= IDLE;
                            end else if (!destination0_valid) begin
                                error_code <= INVALID_BUFFER; done <= 1; phase <= IDLE;
                            end else if (media_range_overflow) begin
                                error_code <= 5; done <= 1; phase <= IDLE;
                            end else begin destination <= id0; phase <= MEDIA_COMMAND; end
                        end
                        `SHOOP_SP_JOB_BEGIN_CAPTURE: begin
                            if (ENABLE_MEDIA == 0) begin error_code <= UNSUPPORTED; done <= 1; phase <= IDLE; end
                            else phase <= MEDIA_COMMAND;
                        end
                        `SHOOP_SP_JOB_CAPTURE: begin
                            if (ENABLE_MEDIA == 0) begin
                                error_code <= UNSUPPORTED; done <= 1; phase <= IDLE;
                            end else if (!media_shape_valid) begin
                                error_code <= INVALID_SHAPE; done <= 1; phase <= IDLE;
                            end else if (!source0_valid) begin
                                error_code <= INVALID_BUFFER; done <= 1; phase <= IDLE;
                            end else begin source_a <= id0; phase <= MEDIA_COMMAND; end
                        end
                        default: begin
                            error_code <= INVALID_JOB;
                            done <= 1;
                            phase <= IDLE;
                        end
                    endcase
                end
                SMOOTH_LOAD: begin
                    if (!packet[128]) begin
                        anchor_value <= 0; elapsed <= packet[127:96]; duration <= packet[127:96];
                        result_sample <= 0; phase <= STORE;
                    end else begin
                        anchor_value <= !smooth_valid[smooth_index] ? 0
                            : packet[129] ? smooth_history[127:96] : smooth_history[95:64];
                        elapsed <= packet[129] || !smooth_valid[smooth_index] ? 0 : smooth_history[63:32];
                        duration <= packet[129] ? packet[127:96]
                            : smooth_valid[smooth_index] ? smooth_history[31:0] : 0;
                        phase <= SMOOTH_PREPARE;
                    end
                end
                SMOOTH_PREPARE: begin
                    if (duration == 0 || elapsed >= duration) phase <= READ_REQUEST;
                    else begin
                        ramp_start <= 0; ramp_negative <= 0; delta_magnitude <= 32'h01000000;
                        multiply_a <= 32'h01000000; multiply_b <= elapsed;
                        phase <= RAMP_MULTIPLY;
                    end
                end
                READ_REQUEST: phase <= (opcode == `SHOOP_SP_JOB_ARITHMETIC || opcode == `SHOOP_SP_JOB_SMOOTHING)
                    ? READ_CAPTURE : READ_RESULT;
                READ_CAPTURE: begin sample_operand <= scratch_data; phase <= READ_RESULT; end
                READ_RESULT: begin
                    if (opcode == `SHOOP_SP_JOB_SMOOTHING) begin
                        if (duration==0 || elapsed>=duration) begin result_sample<=sample_operand;phase<=STORE;end
                        else begin
                            smooth_difference <= $signed({sample_operand[31], sample_operand})
                                - $signed({anchor_value[31], anchor_value});
                            phase <= SMOOTH_OPERAND;
                        end
                    end else if (opcode == `SHOOP_SP_JOB_ARITHMETIC && packet[95:80]!=3) begin
                        sample_a <= sample_operand; phase <= ARITH_B_REQUEST;
                    end else begin
                        result_sample <= opcode == `SHOOP_SP_JOB_ARITHMETIC ? 32'h01000000-sample_operand
                            : (opcode == `SHOOP_SP_JOB_INPUT) ? input_data : scratch_data;
                        phase <= STORE;
                    end
                end
                SMOOTH_OPERAND: begin
                    multiply_a <= smooth_difference[32] ? 32'(-smooth_difference) : smooth_difference[31:0];
                    multiply_b <= current_gain; multiply_negative <= smooth_difference[32];
                    phase <= SMOOTH_MULTIPLY;
                end
                SMOOTH_MULTIPLY: phase <= SMOOTH_PRODUCT;
                SMOOTH_PRODUCT: begin
                    product <= multiplied; product_negative <= multiplied_negative; phase <= SMOOTH_RESULT;
                end
                SMOOTH_RESULT: begin
                    smooth_value <= product_negative
                        ? $signed({{10{anchor_value[31]}}, anchor_value}) - $signed({2'b0, product[63:24]})
                        : $signed({{10{anchor_value[31]}}, anchor_value}) + $signed({2'b0, product[63:24]});
                    phase <= SMOOTH_CLAMP;
                end
                SMOOTH_CLAMP: begin
                    result_sample <= smooth_value > 42'sh007fffffff ? 32'h7fffffff
                        : smooth_value < -42'sh0080000000 ? 32'h80000000 : smooth_value[31:0];
                    phase <= STORE;
                end
                ARITH_B_REQUEST: phase <= ARITH_B_CAPTURE;
                ARITH_B_CAPTURE: begin sample_operand <= scratch_data; phase <= ARITH_B_RESULT; end
                ARITH_B_RESULT: begin
                    arithmetic_factor <= packet[95:80]==1 ? (sample_operand[31] ? 32'h01000000 : 32'h01000000-sample_operand)
                        : packet[95:80]==2 ? (sample_operand[31] ? 32'h01000000+sample_operand : 32'h01000000)
                        : sample_operand;
                    phase <= ARITH_OPERAND;
                end
                ARITH_OPERAND: begin
                    multiply_a <= sample_a[31] ? -sample_a : sample_a;
                    multiply_b <= arithmetic_factor[31] ? -arithmetic_factor : arithmetic_factor;
                    multiply_negative <= sample_a[31]^arithmetic_factor[31];
                    phase <= ARITH_MULTIPLY;
                end
                ARITH_MULTIPLY: phase <= ARITH_PRODUCT;
                ARITH_PRODUCT: begin product <= multiplied; product_negative <= multiplied_negative; phase <= ARITH_RESULT; end
                ARITH_RESULT: begin result_sample <= scaled_product(product[63:24],product_negative); phase <= STORE; end
                RAMP_PREPARE: begin
                    ramp_start <= packet[95:64];
                    ramp_target <= packet[127:96];
                    duration <= packet[159:128];
                    elapsed <= packet[191:160];
                    ramp_negative <= delta[32];
                    ramp_delta <= delta;
                    if ((packet[159:128] == 0) || (packet[191:160] >= packet[159:128]) || (delta == 0)) begin
                        current_gain <= packet[127:96];
                        constant_gain <= 1;
                        phase <= GAIN_STREAM;
                    end else begin
                        constant_gain <= 0;
                        phase <= RAMP_OPERAND;
                    end
                end
                RAMP_OPERAND: begin
                    delta_magnitude <= ramp_delta[32] ? 32'(-ramp_delta) : ramp_delta[31:0];
                    multiply_a <= ramp_delta[32] ? 32'(-ramp_delta) : ramp_delta[31:0];
                    multiply_b <= elapsed;
                    phase <= RAMP_MULTIPLY;
                end
                RAMP_MULTIPLY: phase <= RAMP_PRODUCT;
                RAMP_PRODUCT: begin
                    product <= multiplied;
                    phase <= INITIAL_DIVIDE;
                end
                INITIAL_DIVIDE: phase <= INITIAL_WAIT;
                INITIAL_WAIT: if (divide_done) begin
                    fraction <= divide_remainder;
                    current_gain <= ramp_value(ramp_start, ramp_negative, divide_quotient[31:0]);
                    phase <= STEP_DIVIDE;
                end
                STEP_DIVIDE: phase <= STEP_WAIT;
                STEP_WAIT: if (divide_done) begin
                    step_gain <= ramp_value(0, ramp_negative, divide_quotient[31:0]);
                    step_gain_extra <= ramp_value(ramp_negative ? 32'hffffffff : 32'd1,
                        ramp_negative, divide_quotient[31:0]);
                    step_remainder <= divide_remainder;
                    fraction_limit <= duration-divide_remainder;
                    phase <= opcode == `SHOOP_SP_JOB_SMOOTHING ? READ_REQUEST : GAIN_STREAM;
                end
                GAIN_STREAM, MIX_STREAM: begin
                    if (phase == GAIN_STREAM) begin
                        gain_read_valid <= stream_read;
                        gain_sample_valid <= gain_read_valid;
                        gain_operand_valid <= gain_sample_valid;
                        gain_multiply_valid <= gain_operand_valid;
                        gain_product_valid <= gain_multiply_valid;
                        stream_store_valid <= gain_product_valid;
                        if (stream_read) begin
                            gain_read_gain <= current_gain;
                            frame <= frame+1;
                            if (frame+1 < quantum && !constant_gain) begin
                                elapsed <= elapsed+1;
                                if ({1'b0,elapsed}+1 >= {1'b0,duration}) begin
                                    current_gain <= ramp_target; constant_gain <= 1;
                                end else begin
                                    fraction <= next_fraction;
                                    current_gain <= carry_fraction ? gain_step_extra : gain_step;
                                end
                            end
                        end
                        if (gain_read_valid) begin
                            stream_sample <= scratch_data; stream_gain <= gain_read_gain;
                        end
                        if (gain_sample_valid) begin
                            multiply_a <= stream_sample[31] ? -stream_sample : stream_sample;
                            multiply_b <= stream_gain[31] ? -stream_gain : stream_gain;
                            multiply_negative <= stream_sample[31] ^ stream_gain[31];
                        end
                        if (gain_multiply_valid) begin
                            product <= multiplied;
                            product_negative <= multiplied_negative;
                        end
                        if (gain_product_valid)
                            result_sample <= scaled_product(product[63:24], product_negative);
                    end else begin
                        mix_read_valid <= stream_read;
                        mix_read_second <= mix_second;
                        mix_sample_valid <= mix_read_valid;
                        mix_sample_second <= mix_read_second;
                        stream_store_valid <= mix_sample_valid && mix_sample_second;
                        if (stream_read) begin
                            mix_second <= !mix_second;
                            if (mix_second) frame <= frame+1;
                        end
                        if (mix_read_valid) stream_sample <= scratch_data;
                        if (mix_sample_valid) begin
                            if (mix_sample_second) result_sample <= saturating_mix(sample_a, stream_sample);
                            else sample_a <= stream_sample;
                        end
                    end
                    if (stream_write) begin
                        if (stream_frame+1 == quantum) begin
                            buffer_ready[BUFFER_BITS'(destination)] <= 1;
                            phase <= FETCH;
                        end else stream_frame <= stream_frame+1;
                    end
                end
                STORE: begin
                    if (opcode == `SHOOP_SP_JOB_SMOOTHING) begin
                        if (smooth_commit) smooth_valid[smooth_index] <= 1;
                        if (packet[128] && elapsed < duration) begin
                            elapsed <= smooth_next_elapsed;
                            fraction <= next_fraction;
                            current_gain <= carry_fraction ? gain_step_extra : gain_step;
                        end
                    end
                    if (frame + 1 == quantum) begin
                        if (opcode != `SHOOP_SP_JOB_OUTPUT)
                            buffer_ready[BUFFER_BITS'(destination)] <= 1;
                        frame <= 0;
                        phase <= FETCH;
                    end else begin
                        frame <= frame + 1;
                        phase <= (opcode == `SHOOP_SP_JOB_SILENCE || opcode == `SHOOP_SP_JOB_CONSTANT || (opcode==`SHOOP_SP_JOB_SMOOTHING && !packet[128])) ? STORE : READ_REQUEST;
                    end
                end
                MEDIA_COMMAND: if (media_cmd_valid && media_cmd_ready)
                    phase <= opcode == `SHOOP_SP_JOB_BEGIN_CAPTURE ? MEDIA_DONE : (opcode == `SHOOP_SP_JOB_READ) ? MEDIA_READ : MEDIA_SOURCE_REQUEST;
                MEDIA_READ: if (media_read_valid && media_read_ready) begin
                    if (frame+1 == quantum) begin
                        buffer_ready[BUFFER_BITS'(destination)] <= 1;
                        frame <= 0;
                        phase <= MEDIA_DONE;
                    end else frame <= frame+1;
                end
                MEDIA_SOURCE_REQUEST: phase <= MEDIA_SOURCE_RESULT;
                MEDIA_SOURCE_RESULT: begin sample_a <= scratch_data; phase <= MEDIA_WRITE; end
                MEDIA_WRITE: if (media_write_valid && media_write_ready) begin
                    if (frame + 1 == quantum) begin frame <= 0; phase <= MEDIA_DONE; end
                    else begin frame <= frame + 1; phase <= MEDIA_SOURCE_REQUEST; end
                end
                MEDIA_DONE: phase <= MEDIA_DONE;
                default: begin
                    error_code <= INVALID_JOB;
                    done <= 1;
                    phase <= IDLE;
                end
            endcase
        end
    end
endmodule
