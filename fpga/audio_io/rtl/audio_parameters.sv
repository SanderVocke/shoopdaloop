module audio_parameters #(
    parameter integer KEYS=768, STATE_DEPTH=512
) (
    input wire clk, input wire reset, input wire initialization_open,
    input wire request_valid, output wire request_ready,
    input wire [7:0] request_opcode, input wire [15:0] request_key,
    input wire request_bank, input wire [63:0] request_nonce,
    input wire [31:0] request_count, input wire [15:0] request_address,
    input wire [128:0] request_value, input wire [31:0] request_target, request_duration,
    output wire busy, output reg response_valid, output reg [3:0] response_error,
    output reg [128:0] response_data, output reg fault,
    output reg [31:0] smoothing_frames, output reg smoothing_linear,
    output wire image_valid, input wire image_ready,
    output wire [7:0] image_opcode, output wire image_state,
    output wire [15:0] image_address, output wire [639:0] image_data,
    input wire image_response_valid, input wire [3:0] image_response_error,
    input wire [128:0] image_response_data
);
    localparam integer K=KEYS > 1 ? $clog2(KEYS) : 1;
    localparam integer A=STATE_DEPTH > 1 ? $clog2(STATE_DEPTH) : 1;
    localparam integer M=2*STATE_DEPTH > 1 ? $clog2(2*STATE_DEPTH) : 1;
    localparam [31:0] NONE=32'hffffffff;
    localparam [3:0] INVALID_ID=3, INVALID_STATE=5, UNSUPPORTED=10;
    typedef enum reg [5:0] {IDLE, HEAD_READ, HEAD_WAIT, HEAD_GET,
        ACTIVE_READ, ACTIVE_WAIT, ACTIVE_GET, KEY_READ, KEY_WAIT, KEY_GET,
        LIVE_SEND, LIVE_WAIT, EVAL_SEND, EVAL_WAIT, KEY_WRITE,
        WRITE_SEND, WRITE_WAIT, SNAP_WRITE,
        MAP_READ, MAP_WAIT, MAP_GET, PATCH_SEND, PATCH_WAIT,
        BUILD_HEAD_READ, BUILD_HEAD_WAIT, BUILD_HEAD_GET, BUILD_WRITE} phase_t;
    phase_t phase;
    reg [7:0] operation;
    reg bank, writing, binding_ramp;
    reg [15:0] key, address;
    reg [63:0] nonce;
    reg [31:0] cursor, next_pointer, target, duration;
    reg [128:0] value;
    reg [KEYS-1:0] initialized, active_seen;
    reg [1:0] staged;
    reg [63:0] stage_nonce [0:1];
    reg [31:0] stage_count [0:1], stage_written [0:1];
    reg [31:0] active_count;
    assign busy=phase != IDLE;
    assign request_ready=phase == IDLE && !reset && !fault;
    wire accept=request_valid && request_ready;
    wire valid_key=32'(request_key) < 32'(KEYS);
    wire valid_stage=staged[request_bank] && stage_nonce[request_bank] == request_nonce;
    wire seed=accept && request_opcode == 0 && initialization_open && valid_key && request_value[128];
    wire bind_write=accept && (request_opcode == 2 || request_opcode == 10) && valid_stage
        && (request_opcode == 10 || (valid_key && initialized[K'(request_key)])) && request_count == stage_written[request_bank]
        && request_count < stage_count[request_bank] && 32'(request_address) < 32'(STATE_DEPTH);
    wire [128:0] key_data;
    sp_ram #(.WIDTH(129),.DEPTH(KEYS),.ADDR_BITS(K)) canonical (
        .clk(clk),.read_enable(phase == KEY_READ),.read_address(K'(key)),.read_data(key_data),
        .write_enable(seed || phase == KEY_WRITE || phase == SNAP_WRITE),
        .write_address(seed ? K'(request_key) : K'(key)),
        .write_data(seed ? request_value : value)
    );
    wire [64:0] map_data;
    sp_ram #(.WIDTH(65),.DEPTH(2*STATE_DEPTH),.ADDR_BITS(M)) staged_bindings (
        .clk(clk),.read_enable(phase == MAP_READ),
        .read_address(M'(bank)*M'(STATE_DEPTH)+M'(cursor)),.read_data(map_data),
        .write_enable(bind_write),
        .write_address(M'(request_bank)*M'(STATE_DEPTH)+M'(request_count)),
        .write_data({request_opcode == 2,request_value[31:0],request_key,request_address})
    );
    reg [31:0] binding_base;
    wire [96:0] active_data;
    sp_ram #(.WIDTH(97),.DEPTH(STATE_DEPTH),.ADDR_BITS(A)) active_bindings (
        .clk(clk),.read_enable(phase == ACTIVE_READ),.read_address(A'(cursor)),.read_data(active_data),
        .write_enable(phase == BUILD_WRITE),.write_address(A'(cursor)),
        .write_data({binding_ramp,next_pointer,binding_base,key,address})
    );
    wire [31:0] head_data;
    sp_ram #(.WIDTH(32),.DEPTH(KEYS),.ADDR_BITS(K)) heads (
        .clk(clk),.read_enable(phase == HEAD_READ || phase == BUILD_HEAD_READ),
        .read_address(K'(key)),.read_data(head_data),
        .write_enable(phase == BUILD_WRITE && binding_ramp),.write_address(K'(key)),.write_data(cursor)
    );
    assign image_valid=phase == LIVE_SEND || phase == EVAL_SEND || phase == WRITE_SEND || phase == PATCH_SEND;
    assign image_opcode=phase == LIVE_SEND ? 8'd8 : phase == EVAL_SEND ? 8'd11 : phase == PATCH_SEND ? 8'd9 : 8'd0;
    assign image_state=phase == PATCH_SEND ? binding_ramp : 1'b1;
    assign image_address=address;
    assign image_data=phase == EVAL_SEND ? {447'b0,duration,target,value}
        : phase == PATCH_SEND ? {447'b0,nonce,value} : {511'b0,value};
    task automatic reply(input [3:0] error, input [128:0] data);
        begin phase<=IDLE;response_valid<=1;response_error<=error;response_data<=data;end
    endtask
    task automatic fail;
        begin fault<=1;reply(INVALID_STATE,0);end
    endtask
    task automatic select_key;
        begin phase<=active_seen[K'(request_key)] ? HEAD_READ : KEY_READ;end
    endtask
    task automatic advance_active;
        begin if (cursor+1'b1 == active_count) reply(0,0);else begin cursor<=cursor+1'b1;phase<=ACTIVE_READ;end end
    endtask
    initial if (KEYS < 1 || KEYS > 65536 || STATE_DEPTH < 1 || STATE_DEPTH > 65536)
        $fatal(1,"Invalid parameter capacities");
    always @(posedge clk) begin
        response_valid<=0;
        if (reset) begin
            phase<=IDLE;operation<=0;bank<=0;writing<=0;binding_ramp<=0;binding_base<=0;key<=0;address<=0;nonce<=0;cursor<=0;next_pointer<=NONE;
            target<=0;duration<=0;value<=0;initialized<=0;active_seen<=0;staged<=0;active_count<=0;
            response_valid<=0;response_error<=0;response_data<=0;fault<=0;smoothing_frames<=0;smoothing_linear<=0;
            for (integer i=0;i<2;i=i+1) begin stage_nonce[i]<=0;stage_count[i]<=0;stage_written[i]<=0;end
        end else case (phase)
            IDLE: if (accept) begin
                operation<=request_opcode;bank<=request_bank;key<=request_key;nonce<=request_nonce;
                target<=request_target;duration<=request_duration;writing<=0;
                case (request_opcode)
                    0: if (!valid_key) reply(INVALID_ID,0);
                        else if (!initialization_open || !request_value[128]) reply(INVALID_STATE,0);
                        else begin initialized[K'(request_key)]<=1;reply(0,0);end
                    1: if (request_nonce == 0 || request_nonce[0] != request_bank) reply(INVALID_ID,0);
                        else if (request_count > 32'(STATE_DEPTH)) reply(INVALID_STATE,0);
                        else begin
                            staged[request_bank]<=1;stage_nonce[request_bank]<=request_nonce;
                            stage_count[request_bank]<=request_count;stage_written[request_bank]<=0;reply(0,0);
                        end
                    2,10: if (!valid_stage || (request_opcode == 2 && !valid_key)) reply(INVALID_ID,0);
                        else if (!bind_write) reply(INVALID_STATE,0);
                        else begin stage_written[request_bank]<=request_count+1'b1;reply(0,0);end
                    3,7: if (!valid_key) reply(INVALID_ID,0);
                        else if (!initialized[K'(request_key)]) reply(INVALID_STATE,0);
                        else select_key();
                    4: begin cursor<=0; if (active_count == 0) reply(0,0);else phase<=ACTIVE_READ;end
                    9: begin smoothing_frames<=request_target;smoothing_linear<=request_value[0];cursor<=0;
                        if (active_count == 0) reply(0,0);else phase<=ACTIVE_READ;end
                    5,6,8: if (!valid_stage) reply(INVALID_ID,0);
                        else if (stage_written[request_bank] != stage_count[request_bank]) reply(INVALID_STATE,0);
                        else if (request_opcode == 8) reply(0,0);
                        else if (request_opcode == 5) begin
                            cursor<=0;if (stage_count[request_bank] == 0) reply(0,0);else phase<=MAP_READ;
                        end else begin
                            active_count<=stage_count[request_bank];active_seen<=0;
                            if (stage_count[request_bank] == 0) begin staged[request_bank]<=0;reply(0,0);end
                            else begin cursor<=stage_count[request_bank]-1'b1;phase<=MAP_READ;end
                        end
                    default: reply(UNSUPPORTED,0);
                endcase
            end
            HEAD_READ: phase<=HEAD_WAIT;
            HEAD_WAIT: phase<=HEAD_GET;
            HEAD_GET: if (head_data >= active_count) fail();else begin cursor<=head_data;phase<=ACTIVE_READ;end
            ACTIVE_READ: phase<=ACTIVE_WAIT;
            ACTIVE_WAIT: phase<=ACTIVE_GET;
            ACTIVE_GET: begin
                address<=active_data[15:0];next_pointer<=active_data[95:64];
                if (operation == 9) begin
                    if (active_data[96]) advance_active();
                    else begin value<={65'b0,smoothing_frames,active_data[63:32]};phase<=WRITE_SEND;end
                end else if (operation == 4) begin
                    if (!active_data[96]) advance_active();else begin key<=active_data[31:16];phase<=LIVE_SEND;end
                end
                else if (active_data[31:16] != key) fail();
                else phase<=writing ? WRITE_SEND : LIVE_SEND;
            end
            KEY_READ: phase<=KEY_WAIT;
            KEY_WAIT: phase<=KEY_GET;
            KEY_GET: begin value<=key_data;phase<=operation == 5 ? PATCH_SEND : operation == 7 ? IDLE : EVAL_SEND;
                if (operation == 7) reply(0,key_data);
            end
            LIVE_SEND: if (image_ready) phase<=LIVE_WAIT;
            LIVE_WAIT: if (image_response_valid) begin
                if (image_response_error != 0 || !image_response_data[128]) fail();
                else begin
                    value<=image_response_data;
                    if (operation == 4) phase<=SNAP_WRITE;
                    else if (operation == 7) reply(0,image_response_data);
                    else phase<=EVAL_SEND;
                end
            end
            EVAL_SEND: if (image_ready) phase<=EVAL_WAIT;
            EVAL_WAIT: if (image_response_valid) begin
                if (image_response_error != 0 || !image_response_data[128]) fail();
                else begin value<=image_response_data;phase<=KEY_WRITE;end
            end
            KEY_WRITE: begin writing<=1;if (active_seen[K'(key)]) phase<=WRITE_SEND;else reply(0,value);end
            WRITE_SEND: if (image_ready) phase<=WRITE_WAIT;
            WRITE_WAIT: if (image_response_valid) begin
                if (image_response_error != 0) fail();
                else if (operation == 9) advance_active();
                else if (next_pointer == NONE) reply(0,value);
                else if (next_pointer <= cursor || next_pointer >= active_count) fail();
                else begin cursor<=next_pointer;phase<=ACTIVE_READ;end
            end
            SNAP_WRITE: advance_active();
            MAP_READ: phase<=MAP_WAIT;
            MAP_WAIT: phase<=MAP_GET;
            MAP_GET: begin
                address<=map_data[15:0];key<=map_data[31:16];
                binding_ramp<=map_data[64];binding_base<=map_data[63:32];
                if (operation == 6) begin
                    if (map_data[64]) phase<=BUILD_HEAD_READ;else begin next_pointer<=NONE;phase<=BUILD_WRITE;end
                end else if (map_data[64]) phase<=KEY_READ;
                else begin value<={65'b0,smoothing_frames,map_data[63:32]};phase<=PATCH_SEND;end
            end
            PATCH_SEND: if (image_ready) phase<=PATCH_WAIT;
            PATCH_WAIT: if (image_response_valid) begin
                if (image_response_error != 0) fail();
                else if (cursor+1'b1 == stage_count[bank]) reply(0,0);
                else begin cursor<=cursor+1'b1;phase<=MAP_READ;end
            end
            BUILD_HEAD_READ: phase<=BUILD_HEAD_WAIT;
            BUILD_HEAD_WAIT: phase<=BUILD_HEAD_GET;
            BUILD_HEAD_GET: begin next_pointer<=active_seen[K'(key)] ? head_data : NONE;phase<=BUILD_WRITE;end
            BUILD_WRITE: begin
                if (binding_ramp) active_seen[K'(key)]<=1;
                if (cursor == 0) begin staged[bank]<=0;reply(0,0);end
                else begin cursor<=cursor-1'b1;phase<=MAP_READ;end
            end
            default: fail();
        endcase
    end
endmodule
