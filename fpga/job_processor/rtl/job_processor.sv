`include "state_processor_defs.svh"

module job_processor #(
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
    output wire [2:0] media_cmd_opcode, output wire [63:0] media_cmd_view,
    output wire [63:0] media_cmd_start, output wire [63:0] media_cmd_length,
    output wire [31:0] media_cmd_count,
    input wire media_done, input wire [7:0] media_error, input wire media_fault,
    output wire media_write_valid, input wire media_write_ready, output wire [31:0] media_write_data,
    input wire media_read_valid, output wire media_read_ready, input wire [31:0] media_read_data,
    output wire media_hint_valid, input wire media_hint_ready,
    output reg [63:0] media_hint_view, output reg [63:0] media_hint_position, output reg [63:0] media_hint_length
);
    localparam [7:0] OK = 0, INVALID_BUFFER = 1, INVALID_SHAPE = 2, UNSUPPORTED = 3, INVALID_JOB = 4;
    typedef enum logic [4:0] {
        IDLE, CLEAR_OUTPUT, FETCH, DECODE, READ_REQUEST, READ_RESULT,
        RAMP_PREPARE, RAMP_MULTIPLY,
        INITIAL_DIVIDE, INITIAL_WAIT, STEP_DIVIDE, STEP_WAIT,
        STORE, GAIN_STREAM, MIX_STREAM,
        MEDIA_COMMAND, MEDIA_READ, MEDIA_SOURCE_REQUEST, MEDIA_SOURCE_RESULT, MEDIA_WRITE, MEDIA_DONE
    } phase_t;
    reg [5:0] chunk_shift;
    reg media_shape_valid;
    function automatic [5:0] shift_for(input [31:0] frames);
        begin
            shift_for = 0;
            for (integer bit_index=0; bit_index<32; bit_index=bit_index+1)
                if (frames[bit_index]) shift_for = 6'(bit_index);
        end
    endfunction
    phase_t phase;
    reg [31:0] quantum, logical_buffers, input_count, output_count, remaining_jobs;
    reg [31:0] frame, clear_port;
    reg [`SHOOP_SP_JOB_BITS-1:0] packet;
    wire [7:0] opcode = packet[7:0];
    wire [15:0] id0 = packet[23:8], id1 = packet[39:24];
    wire unused_packet = &{1'b0, packet[63:40], packet[255:192]};
    wire unused_media = &{1'b0, media_cmd_ready, media_done, media_error, media_write_ready, media_read_valid, media_read_data};
    reg [15:0] source_a, source_b, destination, port_index;
    reg [31:0] sample_a, result_sample;
    reg [31:0] stream_frame, gain_read_gain;
    reg gain_read_valid, gain_product_valid, stream_store_valid;
    reg mix_second, mix_read_valid, mix_read_second;
    reg [31:0] current_gain, ramp_start, ramp_target, duration, elapsed;
    reg [31:0] magnitude, fraction, step_quotient, step_remainder, delta_magnitude;
    reg ramp_negative, constant_gain;
    wire signed [32:0] delta = $signed({packet[127], packet[127:96]})
        - $signed({packet[95], packet[95:64]});
    wire [32:0] fraction_sum = {1'b0, fraction} + {1'b0, step_remainder};
    wire carry_fraction = fraction_sum >= {1'b0, duration};
    wire [31:0] next_magnitude = magnitude + step_quotient + 32'(carry_fraction);
    reg [63:0] product;
    reg product_negative;
    reg hint_pending, hinted_head;
    assign media_hint_valid = (ENABLE_MEDIA != 0) && hint_pending && !media_fault && !reset;
    wire [31:0] multiply_a = (phase == RAMP_MULTIPLY) ? delta_magnitude
        : (scratch_data[31] ? -scratch_data : scratch_data);
    wire [31:0] multiply_b = (phase == RAMP_MULTIPLY) ? elapsed
        : (gain_read_gain[31] ? -gain_read_gain : gain_read_gain);
    wire [63:0] multiplied = multiply_a * multiply_b;
    wire divide_done;
    wire [63:0] divide_quotient;
    wire [31:0] divide_remainder;
    wire unused_divide_high = &{1'b0, divide_quotient[63:32]};
    reg cfg_pending, cfg_error, cfg_was_write;
    reg [1:0] cfg_response_space;
    wire cfg_accept = cfg_valid && cfg_ready;
    wire cfg_in_range = (cfg_space == 0) ? cfg_address < 32'(INPUT_PORTS * MAX_FRAMES)
        : (cfg_space == 1) ? cfg_address < 32'(OUTPUT_PORTS * MAX_FRAMES)
        : (cfg_space == 2) && (cfg_address < 32'(BUFFERS * MAX_FRAMES));
    wire [31:0] input_data, output_data, scratch_data;
    wire input_access = cfg_accept && cfg_in_range && (cfg_space == 0);
    wire output_access = cfg_accept && cfg_in_range && (cfg_space == 1);
    wire scratch_access = cfg_accept && cfg_in_range && (cfg_space == 2);
    wire input_read = (phase == READ_REQUEST) && (opcode == `SHOOP_SP_JOB_INPUT);
    wire streaming = phase == GAIN_STREAM || phase == MIX_STREAM;
    wire stream_read = streaming && frame < quantum;
    wire stream_write = streaming && stream_store_valid && !((ENABLE_MEDIA != 0) && media_fault);
    wire scratch_read = ((phase == READ_REQUEST) && (opcode != `SHOOP_SP_JOB_INPUT))
        || stream_read || (phase == MEDIA_SOURCE_REQUEST);
    wire scratch_write = ((phase == STORE) && (opcode != `SHOOP_SP_JOB_OUTPUT)) || stream_write;
    wire output_write = (phase == CLEAR_OUTPUT) || ((phase == STORE) && (opcode == `SHOOP_SP_JOB_OUTPUT));
    wire [INPUT_BITS-1:0] input_address = INPUT_BITS'(32'(port_index) * 32'(MAX_FRAMES) + frame);
    wire [SCRATCH_BITS-1:0] scratch_read_address = SCRATCH_BITS'(32'((phase == MIX_STREAM && mix_second) ? source_b : source_a)
        * 32'(MAX_FRAMES) + frame);
    wire [SCRATCH_BITS-1:0] scratch_write_address = SCRATCH_BITS'(32'(destination) * 32'(MAX_FRAMES)
        + (streaming ? stream_frame : frame));
    wire [OUTPUT_BITS-1:0] output_address = OUTPUT_BITS'(((phase == CLEAR_OUTPUT) ? clear_port : 32'(port_index))
        * 32'(MAX_FRAMES) + frame);
    wire [INPUT_BITS-1:0] input_ram_read_address = busy ? INPUT_BITS'(input_address) : INPUT_BITS'(cfg_address);
    wire [OUTPUT_BITS-1:0] output_ram_write_address = busy ? OUTPUT_BITS'(output_address) : OUTPUT_BITS'(cfg_address);
    wire [SCRATCH_BITS-1:0] scratch_ram_read_address = busy ? SCRATCH_BITS'(scratch_read_address) : SCRATCH_BITS'(cfg_address);
    wire [SCRATCH_BITS-1:0] scratch_ram_write_address = busy ? SCRATCH_BITS'(scratch_write_address) : SCRATCH_BITS'(cfg_address);

    function automatic source_valid(input [15:0] index);
        source_valid = (32'(index) < logical_buffers) && buffer_ready[BUFFER_BITS'(index)];
    endfunction
    function automatic destination_valid(input [15:0] index);
        destination_valid = (32'(index) < logical_buffers) && !buffer_ready[BUFFER_BITS'(index)];
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
    assign job_ready = (phase == FETCH) && (remaining_jobs != 0) && job_enable && !reset && ((ENABLE_MEDIA == 0) || !media_fault);
    assign cfg_response_valid = cfg_pending && !reset;
    assign cfg_response_error = cfg_error;
    assign cfg_read_data = (cfg_error || cfg_was_write) ? 32'b0
        : (cfg_response_space == 0) ? input_data : (cfg_response_space == 1) ? output_data : scratch_data;
    assign media_cmd_valid = (ENABLE_MEDIA != 0) && (phase == MEDIA_COMMAND) && !reset;
    assign media_cmd_opcode = (ENABLE_MEDIA == 0) ? 3'b0 : (opcode == `SHOOP_SP_JOB_READ) ? 3'd6 : 3'd5;
    assign media_cmd_view = (ENABLE_MEDIA == 0) ? 64'b0 : packet[127:64];
    assign media_cmd_start = (ENABLE_MEDIA == 0) ? 64'b0 : packet[191:128] << chunk_shift;
    assign media_cmd_length = (ENABLE_MEDIA == 0) ? 64'b0 : packet[255:192] << chunk_shift;
    assign media_cmd_count = (ENABLE_MEDIA == 0) ? 32'b0 : quantum;
    assign media_write_valid = (ENABLE_MEDIA != 0) && (phase == MEDIA_WRITE) && !reset;
    assign media_write_data = (ENABLE_MEDIA == 0) ? 32'b0 : sample_a;
    assign media_read_ready = (ENABLE_MEDIA != 0) && (phase == MEDIA_READ) && !reset;

    jp_divider divide (
        .clk(clk), .reset(reset), .start((phase == INITIAL_DIVIDE) || (phase == STEP_DIVIDE)),
        .numerator((phase == STEP_DIVIDE) ? {32'b0, delta_magnitude} : product),
        .denominator(duration), .done(divide_done), .quotient(divide_quotient), .remainder(divide_remainder)
    );
    jp_sample_ram #(.DEPTH(INPUT_PORTS * MAX_FRAMES), .ADDR_BITS(INPUT_BITS)) inputs_memory (
        .clk(clk), .read_enable(!reset && (input_read || (input_access && !cfg_write))),
        .read_address(input_ram_read_address), .read_data(input_data),
        .write_enable(input_access && cfg_write), .write_address(INPUT_BITS'(cfg_address)), .write_data(cfg_write_data)
    );
    jp_sample_ram #(.DEPTH(OUTPUT_PORTS * MAX_FRAMES), .ADDR_BITS(OUTPUT_BITS)) outputs_memory (
        .clk(clk), .read_enable(output_access && !cfg_write),
        .read_address(OUTPUT_BITS'(cfg_address)), .read_data(output_data),
        .write_enable(!reset && (output_write || (output_access && cfg_write))),
        .write_address(output_ram_write_address),
        .write_data(busy ? ((phase == CLEAR_OUTPUT) ? 32'b0 : result_sample) : cfg_write_data)
    );
    jp_sample_ram #(.DEPTH(BUFFERS * MAX_FRAMES), .ADDR_BITS(SCRATCH_BITS)) scratch_memory (
        .clk(clk), .read_enable(!reset && (scratch_read || (scratch_access && !cfg_write))),
        .read_address(scratch_ram_read_address), .read_data(scratch_data),
        .write_enable(!reset && (scratch_write || (scratch_access && cfg_write))),
        .write_address(scratch_ram_write_address), .write_data(busy ? result_sample : cfg_write_data)
    );

    always @(posedge clk) begin
        if (reset) begin
            hint_pending <= 0; hinted_head <= 0;
            media_hint_view <= 0; media_hint_position <= 0; media_hint_length <= 0;
        end else if (media_fault && ENABLE_MEDIA != 0) begin hint_pending <= 0; hinted_head <= 0; end
        else begin
            if (media_hint_valid && media_hint_ready) hint_pending <= 0;
            if (job_valid && job_ready) hinted_head <= 0;
            if (ENABLE_MEDIA != 0 && !hint_pending && !hinted_head && phase != IDLE && phase != FETCH
                && job_valid && job_data[7:0] == `SHOOP_SP_JOB_READ) begin
                hint_pending <= 1; hinted_head <= 1;
                media_hint_view <= job_data[127:64]; media_hint_position <= job_data[191:128]; media_hint_length <= job_data[255:192];
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
            done <= 0;
            error_code <= OK;
            buffer_ready <= 0;
            quantum <= 0; chunk_shift <= 0; media_shape_valid <= 0;
            logical_buffers <= 0;
            input_count <= 0;
            output_count <= 0;
            remaining_jobs <= 0;
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
            gain_read_valid <= 0; gain_product_valid <= 0; stream_store_valid <= 0;
            mix_second <= 0; mix_read_valid <= 0; mix_read_second <= 0;
            current_gain <= 0;
            ramp_start <= 0;
            ramp_target <= 0;
            duration <= 0;
            elapsed <= 0;
            magnitude <= 0;
            fraction <= 0;
            step_quotient <= 0;
            step_remainder <= 0;
            delta_magnitude <= 0;
            ramp_negative <= 0;
            constant_gain <= 0;
            product <= 0;
            product_negative <= 0;
        end else begin
            done <= 0;
            gain_read_valid <= 0; gain_product_valid <= 0; mix_read_valid <= 0; stream_store_valid <= 0;
            if ((ENABLE_MEDIA != 0) && media_fault && phase != IDLE) begin
                error_code <= 8; done <= 1; phase <= IDLE;
            end else if ((ENABLE_MEDIA != 0) && media_done && (phase >= MEDIA_COMMAND)) begin
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
                        quantum <= run_quantum; chunk_shift <= shift_for(run_chunk_frames);
                        media_shape_valid <= run_chunk_frames == 32'(CHUNK_FRAMES);
                        logical_buffers <= run_buffers;
                        input_count <= run_inputs;
                        output_count <= run_outputs;
                        remaining_jobs <= run_jobs;
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
                    if (remaining_jobs == 0) begin
                        done <= 1;
                        phase <= IDLE;
                    end else if (job_valid && job_ready) begin
                        packet <= job_data;
                        remaining_jobs <= remaining_jobs - 1;
                        phase <= DECODE;
                    end
                end
                DECODE: begin
                    frame <= 0;
                    stream_frame <= 0; mix_second <= 0;
                    case (opcode)
                        `SHOOP_SP_JOB_INPUT: begin
                            if (!destination_valid(id1) || (32'(id0) >= input_count)) begin
                                error_code <= INVALID_BUFFER;
                                done <= 1;
                                phase <= IDLE;
                            end else begin
                                port_index <= id0;
                                destination <= id1;
                                phase <= READ_REQUEST;
                            end
                        end
                        `SHOOP_SP_JOB_SILENCE: begin
                            if (!destination_valid(id0)) begin
                                error_code <= INVALID_BUFFER;
                                done <= 1;
                                phase <= IDLE;
                            end else begin
                                destination <= id0;
                                result_sample <= 0;
                                phase <= STORE;
                            end
                        end
                        `SHOOP_SP_JOB_GAIN: begin
                            if (!source_valid(id0) || !destination_valid(id1)) begin
                                error_code <= INVALID_BUFFER;
                                done <= 1;
                                phase <= IDLE;
                            end else begin
                                source_a <= id0;
                                destination <= id1;
                                phase <= RAMP_PREPARE;
                            end
                        end
                        `SHOOP_SP_JOB_MIX: begin
                            if (!source_valid(id0) || !source_valid(id1) || !destination_valid(packet[79:64])) begin
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
                            if (!source_valid(id0) || (32'(id1) >= output_count)) begin
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
                            end else if (!destination_valid(id0)) begin
                                error_code <= INVALID_BUFFER; done <= 1; phase <= IDLE;
                            end else if (packet[191:128] > (64'hffffffffffffffff >> chunk_shift)
                                || packet[255:192] > (64'hffffffffffffffff >> chunk_shift)) begin
                                error_code <= 5; done <= 1; phase <= IDLE;
                            end else begin destination <= id0; phase <= MEDIA_COMMAND; end
                        end
                        `SHOOP_SP_JOB_CAPTURE: begin
                            if (ENABLE_MEDIA == 0) begin
                                error_code <= UNSUPPORTED; done <= 1; phase <= IDLE;
                            end else if (!media_shape_valid) begin
                                error_code <= INVALID_SHAPE; done <= 1; phase <= IDLE;
                            end else if (!source_valid(id0)) begin
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
                READ_REQUEST: phase <= READ_RESULT;
                READ_RESULT: begin
                    result_sample <= (opcode == `SHOOP_SP_JOB_INPUT) ? input_data : scratch_data;
                    phase <= STORE;
                end
                RAMP_PREPARE: begin
                    ramp_start <= packet[95:64];
                    ramp_target <= packet[127:96];
                    duration <= packet[159:128];
                    elapsed <= packet[191:160];
                    ramp_negative <= delta[32];
                    delta_magnitude <= delta[32] ? 32'(-delta) : delta[31:0];
                    if ((packet[159:128] == 0) || (packet[191:160] >= packet[159:128]) || (delta == 0)) begin
                        current_gain <= packet[127:96];
                        constant_gain <= 1;
                        phase <= GAIN_STREAM;
                    end else begin
                        constant_gain <= 0;
                        phase <= RAMP_MULTIPLY;
                    end
                end
                RAMP_MULTIPLY: begin
                    product <= multiplied;
                    phase <= INITIAL_DIVIDE;
                end
                INITIAL_DIVIDE: phase <= INITIAL_WAIT;
                INITIAL_WAIT: if (divide_done) begin
                    magnitude <= divide_quotient[31:0];
                    fraction <= divide_remainder;
                    current_gain <= ramp_value(ramp_start, ramp_negative, divide_quotient[31:0]);
                    phase <= STEP_DIVIDE;
                end
                STEP_DIVIDE: phase <= STEP_WAIT;
                STEP_WAIT: if (divide_done) begin
                    step_quotient <= divide_quotient[31:0];
                    step_remainder <= divide_remainder;
                    phase <= GAIN_STREAM;
                end
                GAIN_STREAM, MIX_STREAM: begin
                    if (phase == GAIN_STREAM) begin
                        gain_read_valid <= stream_read;
                        gain_product_valid <= gain_read_valid;
                        stream_store_valid <= gain_product_valid;
                        if (stream_read) begin
                            gain_read_gain <= current_gain;
                            frame <= frame+1;
                            if (frame+1 < quantum && !constant_gain) begin
                                elapsed <= elapsed+1;
                                if ({1'b0,elapsed}+1 >= {1'b0,duration}) begin
                                    current_gain <= ramp_target; constant_gain <= 1;
                                end else begin
                                    magnitude <= next_magnitude;
                                    fraction <= carry_fraction ? 32'(fraction_sum-{1'b0,duration}) : fraction_sum[31:0];
                                    current_gain <= ramp_value(ramp_start, ramp_negative, next_magnitude);
                                end
                            end
                        end
                        if (gain_read_valid) begin
                            product <= multiplied;
                            product_negative <= scratch_data[31] ^ gain_read_gain[31];
                        end
                        if (gain_product_valid)
                            result_sample <= scaled_product(product[63:24], product_negative);
                    end else begin
                        mix_read_valid <= stream_read;
                        mix_read_second <= mix_second;
                        stream_store_valid <= mix_read_valid && mix_read_second;
                        if (stream_read) begin
                            mix_second <= !mix_second;
                            if (mix_second) frame <= frame+1;
                        end
                        if (mix_read_valid) begin
                            if (mix_read_second) result_sample <= saturating_mix(sample_a, scratch_data);
                            else sample_a <= scratch_data;
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
                    if (frame + 1 == quantum) begin
                        if (opcode != `SHOOP_SP_JOB_OUTPUT)
                            buffer_ready[BUFFER_BITS'(destination)] <= 1;
                        frame <= 0;
                        phase <= (opcode == `SHOOP_SP_JOB_READ) ? MEDIA_DONE : FETCH;
                    end else begin
                        frame <= frame + 1;
                        phase <= (opcode == `SHOOP_SP_JOB_SILENCE) ? STORE
                            : (opcode == `SHOOP_SP_JOB_READ) ? MEDIA_READ : READ_REQUEST;
                    end
                end
                MEDIA_COMMAND: if (media_cmd_valid && media_cmd_ready)
                    phase <= (opcode == `SHOOP_SP_JOB_READ) ? MEDIA_READ : MEDIA_SOURCE_REQUEST;
                MEDIA_READ: if (media_read_valid && media_read_ready) begin
                    result_sample <= media_read_data;
                    phase <= STORE;
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
