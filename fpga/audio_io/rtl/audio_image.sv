module audio_image #(
    parameter integer PROGRAM_DEPTH=256, STATE_DEPTH=128, JOB_DEPTH=256, BUFFERS=128
) (
    input wire clk, input wire reset, input wire execution_idle,
    input wire [31:0] current_state_count,
    input wire request_valid, output wire request_ready,
    input wire [7:0] request_opcode, input wire request_state,
    input wire [15:0] request_address, input wire [639:0] request_data,
    output wire busy, output reg response_valid, output reg [3:0] response_error,
    output reg [128:0] response_data, output reg fault,
    output reg active_override,
    output reg [31:0] active_instructions, active_states, active_jobs, active_buffers,
    output wire cfg_valid, input wire cfg_ready, output wire cfg_state, output wire cfg_write,
    output wire [15:0] cfg_address, output wire [128:0] cfg_write_data,
    input wire cfg_response_valid, input wire cfg_response_error, input wire [128:0] cfg_read_data
);
    localparam integer DEPTH=PROGRAM_DEPTH+STATE_DEPTH;
    localparam integer A=DEPTH > 1 ? $clog2(DEPTH) : 1;
    localparam integer M=STATE_DEPTH > 1 ? $clog2(STATE_DEPTH) : 1;
    localparam [3:0] INVALID_CONFIG=1, CAPACITY=2, INVALID_ID=3, INVALID_STATE=5, EXHAUSTED=8, UNSUPPORTED=10;
    typedef enum reg [4:0] {IDLE, LEGACY_SEND, LEGACY_WAIT,
        MAP_FETCH, MAP_WAIT, MAP_GET, SOURCE_SEND, SOURCE_WAIT,
        PROGRAM_FETCH, PROGRAM_WAIT, PROGRAM_SEND, PROGRAM_ACK,
        STATE_FETCH, STATE_WAIT, STATE_SEND, STATE_ACK, COMMIT,
        RAMP_READ, RAMP_WAIT, RAMP_PRODUCT, RAMP_DIV_START, RAMP_DIV_WAIT,
        RAMP_WRITE, RAMP_ACK} phase_t;
    phase_t phase;
    reg [63:0] version, nonce, base_version;
    reg staging;
    reg [31:0] programs, states, jobs, buffers, maps, p_written, s_written, m_written, cursor;
    reg [15:0] source, destination, address;
    reg expected_ramp, selected_state, selected_read;
    reg [128:0] selected_data;
    reg [31:0] target, duration, ramp_start, ramp_duration, ramp_elapsed, magnitude, next_start;
    reg negative;
    reg [63:0] numerator;
    wire [31:0] live_states = active_override ? active_states : current_state_count;
    assign busy = phase != IDLE;
    wire exclusive = request_opcode == 0 || request_opcode == 5 || request_opcode == 6 || request_opcode == 8;
    assign request_ready = phase == IDLE && !fault && !reset && (!exclusive || execution_idle);
    wire accept = request_valid && request_ready;
    wire token = staging && request_data[192:129] == nonce;
    wire upload_program = request_opcode == 2 && !request_state && token
        && 32'(request_address) == p_written && p_written < programs;
    wire upload_state = request_opcode == 3 && request_state && token
        && 32'(request_address) == s_written && s_written < states;
    wire upload_map = request_opcode == 4 && token && 32'(request_address) == m_written
        && m_written < maps && {16'b0,request_data[15:0]} < live_states
        && {16'b0,request_data[31:16]} < states;
    wire copy_valid = !cfg_response_error && cfg_read_data[128] == expected_ramp
        && stage_read_data[128] == expected_ramp;
    wire copy_write = phase == SOURCE_WAIT && cfg_response_valid && copy_valid;
    wire upload = accept && (upload_program || upload_state);
    wire stage_write = upload || copy_write;
    wire [A-1:0] stage_write_address = upload ?
        (upload_program ? A'(request_address) : A'(PROGRAM_DEPTH)+A'(request_address))
        : A'(PROGRAM_DEPTH)+A'(destination);
    wire [128:0] stage_write_data = upload ?
        (upload_program ? {33'b0,request_data[95:0]} : request_data[128:0]) : cfg_read_data;
    wire stage_read = phase == PROGRAM_FETCH || phase == STATE_FETCH || phase == MAP_GET;
    wire [A-1:0] stage_read_address = phase == PROGRAM_FETCH ? A'(cursor)
        : A'(PROGRAM_DEPTH)+(phase == MAP_GET ? A'(map_read_data[31:16]) : A'(cursor));
    wire [128:0] stage_read_data;
    wire [32:0] map_read_data;
    sp_ram #(.WIDTH(129),.DEPTH(DEPTH),.ADDR_BITS(A)) staging_memory (
        .clk(clk),.read_enable(stage_read),.read_address(stage_read_address),.read_data(stage_read_data),
        .write_enable(stage_write),.write_address(stage_write_address),.write_data(stage_write_data)
    );
    sp_ram #(.WIDTH(33),.DEPTH(STATE_DEPTH),.ADDR_BITS(M)) copy_map (
        .clk(clk),.read_enable(phase == MAP_FETCH),.read_address(M'(cursor)),.read_data(map_read_data),
        .write_enable(accept && upload_map),.write_address(M'(request_address)),.write_data(request_data[32:0])
    );
    wire divide_done;
    wire [63:0] quotient;
    wire [31:0] remainder;
    wire unused = &{1'b0,remainder,quotient[63:32],destination,request_data[639:193]};
    jp_divider divider (.clk(clk),.reset(reset),.start(phase == RAMP_DIV_START),
        .numerator(numerator),.denominator(ramp_duration),.done(divide_done),.quotient(quotient),.remainder(remainder));
    wire [128:0] next_ramp = {1'b1,32'b0,duration,target,next_start};
    assign cfg_valid = phase == LEGACY_SEND || phase == SOURCE_SEND || phase == PROGRAM_SEND
        || phase == STATE_SEND || phase == RAMP_READ || phase == RAMP_WRITE;
    assign cfg_state = phase == LEGACY_SEND ? selected_state : phase != PROGRAM_SEND;
    assign cfg_write = phase == LEGACY_SEND ? !selected_read : phase != SOURCE_SEND && phase != RAMP_READ;
    assign cfg_address = phase == SOURCE_SEND ? source
        : (phase == PROGRAM_SEND || phase == STATE_SEND) ? 16'(cursor) : address;
    assign cfg_write_data = phase == RAMP_WRITE ? next_ramp
        : (phase == PROGRAM_SEND || phase == STATE_SEND) ? stage_read_data : selected_data;
    task automatic reply(input [3:0] error, input [128:0] data);
        begin response_valid<=1; response_error<=error; response_data<=data; phase<=IDLE; end
    endtask
    task automatic activation_failed;
        begin fault<=1; reply(INVALID_STATE,0); end
    endtask
    initial if (PROGRAM_DEPTH < 1 || PROGRAM_DEPTH > 65536 || STATE_DEPTH < 1 || STATE_DEPTH > 65536
        || JOB_DEPTH < 1 || BUFFERS < 1) $fatal(1,"Invalid image capacities");
    always @(posedge clk) begin : sequencer
        reg signed [32:0] difference;
        response_valid<=0;
        if (reset) begin
            phase<=IDLE; response_valid<=0; response_error<=0; response_data<=0; fault<=0;
            active_override<=0; active_instructions<=0; active_states<=0; active_jobs<=0; active_buffers<=0;
            version<=0; nonce<=0; base_version<=0; staging<=0;
            programs<=0; states<=0; jobs<=0; buffers<=0; maps<=0;
            p_written<=0; s_written<=0; m_written<=0; cursor<=0;
            source<=0; destination<=0; address<=0; expected_ramp<=0;
            selected_state<=0; selected_read<=0; selected_data<=0;
            target<=0; duration<=0; ramp_start<=0; ramp_duration<=0; ramp_elapsed<=0;
            magnitude<=0; next_start<=0; negative<=0; numerator<=0;
        end else case (phase)
            IDLE: if (accept) begin
                case (request_opcode)
                    0,8: begin
                        if (request_opcode == 0 && !request_state && &version) reply(EXHAUSTED,0);
                        else begin
                            selected_state<=request_state; selected_read<=request_opcode == 8;
                            selected_data<=request_data[128:0]; address<=request_address; phase<=LEGACY_SEND;
                        end
                    end
                    1: begin
                        if (request_data[63:0] != version) reply(INVALID_ID,0);
                        else if (request_data[95:64] > 32'(PROGRAM_DEPTH) || request_data[127:96] > 32'(STATE_DEPTH)
                            || request_data[159:128] > 32'(JOB_DEPTH) || request_data[191:160] > 32'(BUFFERS)
                            || request_data[223:192] > request_data[127:96]) reply(CAPACITY,0);
                        else if (&nonce) reply(EXHAUSTED,0);
                        else begin
                            nonce<=nonce+1'b1; base_version<=version; staging<=1;
                            programs<=request_data[95:64]; states<=request_data[127:96]; jobs<=request_data[159:128];
                            buffers<=request_data[191:160]; maps<=request_data[223:192];
                            p_written<=0; s_written<=0; m_written<=0;
                            reply(0,{65'b0,nonce+64'd1});
                        end
                    end
                    2,3: begin
                        if (!token) reply(INVALID_ID,0);
                        else if (!(upload_program || upload_state)) reply(INVALID_STATE,0);
                        else begin
                            if (upload_program) p_written<=p_written+1'b1;
                            else s_written<=s_written+1'b1;
                            reply(0,0);
                        end
                    end
                    4: begin
                        if (!token) reply(INVALID_ID,0);
                        else if (!upload_map) reply(INVALID_STATE,0);
                        else begin m_written<=m_written+1'b1; reply(0,0); end
                    end
                    5: begin
                        if (!staging || request_data[63:0] != nonce || base_version != version) reply(INVALID_ID,0);
                        else if (p_written != programs || s_written != states || m_written != maps) reply(INVALID_STATE,0);
                        else if (&version) reply(EXHAUSTED,0);
                        else begin cursor<=0; phase<=maps != 0 ? MAP_FETCH : programs != 0 ? PROGRAM_FETCH : states != 0 ? STATE_FETCH : COMMIT; end
                    end
                    6: begin
                        if (!request_state || 32'(request_address) >= live_states) reply(INVALID_ID,0);
                        else if (request_data[192:129] != version) reply(INVALID_ID,0);
                        else begin
                            address<=request_address; target<=request_data[31:0]; duration<=request_data[63:32];
                            phase<=RAMP_READ;
                        end
                    end
                    7: reply(0,{staging,nonce,version});
                    default: reply(UNSUPPORTED,0);
                endcase
            end
            LEGACY_SEND: if (cfg_ready) phase<=LEGACY_WAIT;
            LEGACY_WAIT: if (cfg_response_valid) begin
                if (!cfg_response_error && !selected_read && !selected_state) begin version<=version+1'b1; staging<=0; end
                reply(cfg_response_error ? INVALID_CONFIG : 4'd0,selected_read && !cfg_response_error ? cfg_read_data : 129'b0);
            end
            MAP_FETCH: phase<=MAP_WAIT;
            MAP_WAIT: phase<=MAP_GET;
            MAP_GET: begin
                source<=map_read_data[15:0]; destination<=map_read_data[31:16]; expected_ramp<=map_read_data[32];
                phase<=SOURCE_SEND;
            end
            SOURCE_SEND: if (cfg_ready) phase<=SOURCE_WAIT;
            SOURCE_WAIT: if (cfg_response_valid) begin
                if (!copy_valid) reply(INVALID_STATE,0);
                else if (cursor+1 == maps) begin cursor<=0; phase<=programs != 0 ? PROGRAM_FETCH : states != 0 ? STATE_FETCH : COMMIT; end
                else begin cursor<=cursor+1'b1; phase<=MAP_FETCH; end
            end
            PROGRAM_FETCH: phase<=PROGRAM_WAIT;
            PROGRAM_WAIT: phase<=PROGRAM_SEND;
            PROGRAM_SEND: if (cfg_ready) phase<=PROGRAM_ACK;
            PROGRAM_ACK: if (cfg_response_valid) begin
                if (cfg_response_error) activation_failed();
                else if (cursor+1 == programs) begin cursor<=0; phase<=states != 0 ? STATE_FETCH : COMMIT; end
                else begin cursor<=cursor+1'b1; phase<=PROGRAM_FETCH; end
            end
            STATE_FETCH: phase<=STATE_WAIT;
            STATE_WAIT: phase<=STATE_SEND;
            STATE_SEND: if (cfg_ready) phase<=STATE_ACK;
            STATE_ACK: if (cfg_response_valid) begin
                if (cfg_response_error) activation_failed();
                else if (cursor+1 == states) phase<=COMMIT;
                else begin cursor<=cursor+1'b1; phase<=STATE_FETCH; end
            end
            COMMIT: begin
                version<=version+1'b1; staging<=0; active_override<=1;
                active_instructions<=programs; active_states<=states; active_jobs<=jobs; active_buffers<=buffers;
                reply(0,{65'b0,version+64'd1});
            end
            RAMP_READ: if (cfg_ready) phase<=RAMP_WAIT;
            RAMP_WAIT: if (cfg_response_valid) begin
                if (cfg_response_error || !cfg_read_data[128]) reply(INVALID_STATE,0);
                else if (cfg_read_data[95:64] == 0 || cfg_read_data[127:96] >= cfg_read_data[95:64]) begin
                    next_start<=cfg_read_data[63:32]; phase<=RAMP_WRITE;
                end else begin
                    difference=$signed({cfg_read_data[63],cfg_read_data[63:32]})-$signed({cfg_read_data[31],cfg_read_data[31:0]});
                    magnitude<=difference[32] ? 32'(-difference) : 32'(difference); negative<=difference[32];
                    ramp_start<=cfg_read_data[31:0]; ramp_duration<=cfg_read_data[95:64]; ramp_elapsed<=cfg_read_data[127:96];
                    phase<=RAMP_PRODUCT;
                end
            end
            RAMP_PRODUCT: begin numerator<=64'(magnitude)*64'(ramp_elapsed); phase<=RAMP_DIV_START; end
            RAMP_DIV_START: phase<=RAMP_DIV_WAIT;
            RAMP_DIV_WAIT: if (divide_done) begin
                next_start<=negative ? ramp_start-quotient[31:0] : ramp_start+quotient[31:0]; phase<=RAMP_WRITE;
            end
            RAMP_WRITE: if (cfg_ready) phase<=RAMP_ACK;
            RAMP_ACK: if (cfg_response_valid) begin
                if (cfg_response_error) activation_failed(); else reply(0,next_ramp);
            end
            default: activation_failed();
        endcase
    end
endmodule
