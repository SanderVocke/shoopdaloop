module fifo_edit_kernel #(
    parameter integer LOOPS = 128
) (
    input wire clk, input wire reset,
    input wire command_valid, output wire command_ready,
    input wire [2:0] command_kind,
    input wire [15:0] command_slot, input wire [31:0] command_generation,
    input wire command_self_sync, input wire [63:0] command_frame,
    input wire [159:0] command_args, input wire [15:0] command_gain_address,
    output wire memory_valid, input wire memory_ready,
    output wire memory_write, output wire memory_state,
    output wire [15:0] memory_address, output wire [128:0] memory_write_data,
    input wire memory_response_valid, input wire memory_response_error,
    input wire [128:0] memory_read_data,
    output wire result_valid, input wire result_ready,
    output reg [3:0] result_code, output reg write_fault,
    output reg [31:0] command_cycles
);
    localparam [3:0] APPLIED=1, TARGET=4, INVALID=5, UNSUPPORTED=6,
        CANNOT=7, EXHAUSTED=8, SCHEDULED=10;
    localparam [31:0] ABSENT=32'hffffffff;
    localparam [63:0] PLAN_MASK=64'h000003ff00000000;
    typedef enum logic [4:0] {
        IDLE, READ_WORD, READ_WAIT, CHECK, NORMALIZE, NOW_CHECK, NOW_APPLY,
        PLANS, PLAN_STEP, INSERT_NOW, PACK, WRITE_WORD, WRITE_WAIT,
        GAIN, GAIN_WAIT, MULTIPLY, ITER_CHECK, RESULT
    } phase_t;
    phase_t phase;
    reg [2:0] kind;
    reg [15:0] slot, gain_address;
    reg [31:0] generation;
    reg self_sync;
    reg [63:0] frame;
    reg [159:0] args;
    reg [3:0] word_index;
    reg [63:0] row [0:15];
    reg [31:0] now_mode, first_mode, second_mode, first_skip, second_skip;
    reg now_operation;
    reg step_index;
    reg [4:0] plan_flags [0:1];
    reg [63:0] plan_frame [0:1], plan_ticks [0:1], plan_position [0:1];
    reg [1:0] plan_sequence [0:1];
    reg [63:0] product, multiplicand;
    reg [31:0] multiplier;
    reg shifted_overflow;
    reg [4:0] multiply_bit;
    wire composite = row[1][45];
    wire queued = composite || row[1][44];
    wire [1:0] mode = row[0][34:33] == 1 ? 2'd1 : row[0][34:33] == 2 ? 2'd2 : 2'd0;
    wire [63:0] playable_length = mode == 2 ? (composite ? 64'd0 : row[4]) : row[3];
    wire [31:0] step_mode = step_index ? second_mode : first_mode;
    wire [31:0] step_skip = step_index ? second_skip : first_skip;
    wire [63:0] add_left = phase == MULTIPLY ? product : phase == PLAN_STEP ? frame : row[7];
    wire [63:0] add_right = phase == MULTIPLY ? multiplicand : 64'd1;
    wire [64:0] sum = {1'b0,add_left}+{1'b0,add_right};
    wire rebased = queued && self_sync && now_operation;
    wire busy = phase != IDLE && phase != RESULT;

    assign command_ready = phase == IDLE && !reset;
    assign result_valid = phase == RESULT && !reset;
    assign memory_valid = !reset && (phase == READ_WORD || phase == WRITE_WORD || phase == GAIN);
    assign memory_write = phase == WRITE_WORD || phase == GAIN;
    assign memory_state = phase == GAIN;
    assign memory_address = phase == GAIN ? gain_address : {slot[11:0],word_index};
    assign memory_write_data = phase == GAIN ? {1'b1,64'd0,args[31:0],args[31:0]} : {65'd0,row[word_index]};

    task automatic refuse(input [3:0] code);
        begin result_code<=code; phase<=RESULT; end
    endtask

    always @(posedge clk) begin
        if (reset) begin
            phase<=IDLE; result_code<=0; write_fault<=0; command_cycles<=0;
        end else begin
            if (busy) command_cycles<=command_cycles+1;
            case (phase)
                IDLE: if (command_valid) begin
                    kind<=command_kind; slot<=command_slot; generation<=command_generation;
                    self_sync<=command_self_sync; frame<=command_frame; args<=command_args;
                    gain_address<=command_gain_address; command_cycles<=0; write_fault<=0;
                    word_index<=0;
                    if (32'(command_slot)>=32'(LOOPS)) refuse(TARGET);
                    else phase<=READ_WORD;
                end
                READ_WORD: if (memory_ready) phase<=READ_WAIT;
                READ_WAIT: if (memory_response_valid) begin
                    if (memory_response_error) refuse(CANNOT);
                    else begin
                        row[word_index]<=memory_read_data[63:0];
                        if (word_index==15) phase<=CHECK;
                        else begin word_index<=word_index+1'b1; phase<=READ_WORD; end
                    end
                end
                CHECK: begin
                    word_index<=0;
                    if (!row[0][0] || row[0][32:1]!=generation) refuse(TARGET);
                    else case (kind)
                        0: begin
                            now_mode<=args[31:0]; first_mode<=args[63:32]; first_skip<=args[95:64];
                            second_mode<=args[127:96]; second_skip<=args[159:128];
                            if ((args[31:0]!=ABSENT && args[31:0]>2)
                                || (args[63:32]!=ABSENT && args[63:32]>2)
                                || (args[127:96]!=ABSENT && args[127:96]>2)
                                || (args[63:32]==ABSENT && args[95:64]!=0)
                                || (args[127:96]==ABSENT && args[159:128]!=0)) refuse(INVALID);
                            else phase<=NORMALIZE;
                        end
                        1: if (args[159:32]!=0 || args[31:0]==0 || args[31:0]>3) refuse(INVALID);
                           else begin
                               if (args[0] && (!queued || row[1][33])) row[1][32]<=0;
                               if (args[1] && (!queued || row[1][38])) row[1][37]<=0;
                               result_code<=APPLIED; phase<=WRITE_WORD;
                           end
                        2: if (args[159:1]!=0) refuse(INVALID);
                           else begin row[0][35]<=args[0]; result_code<=APPLIED; phase<=WRITE_WORD; end
                        3: if (args[159:32]!=0) refuse(INVALID); else phase<=GAIN;
                        4: if (args[159:66]!=0) refuse(INVALID);
                           else if (!composite) refuse(TARGET);
                           else if (row[6][63:32]!=0) refuse(CANNOT);
                           else if (row[6][31:0]==0) refuse(TARGET);
                           else begin
                               product<=0; multiplicand<=args[63:0]; multiplier<=row[6][31:0];
                               shifted_overflow<=0; multiply_bit<=0; phase<=MULTIPLY;
                           end
                        default: refuse(UNSUPPORTED);
                    endcase
                end
                NORMALIZE: begin
                    if (self_sync && mode!=1 && now_mode==ABSENT) begin
                        if (first_mode!=ABSENT && second_mode!=ABSENT) refuse(CANNOT);
                        else begin
                            now_mode<=first_mode!=ABSENT ? first_mode : second_mode;
                            first_mode<=ABSENT; second_mode<=ABSENT; phase<=NOW_CHECK;
                        end
                    end else phase<=NOW_CHECK;
                end
                NOW_CHECK: begin
                    if (self_sync && mode!=1 && now_mode!=1 && (first_mode!=ABSENT || second_mode!=ABSENT)) refuse(CANNOT);
                    else if (composite && now_mode==2) refuse(CANNOT);
                    else if (now_mode==2 && mode!=2 && sum[64]) refuse(EXHAUSTED);
                    else if (now_mode==1 && !(composite && row[3]==0) && playable_length==0) refuse(CANNOT);
                    else begin
                        now_operation<=now_mode!=ABSENT && !(now_mode==2 && mode==2);
                        phase<=NOW_APPLY;
                    end
                end
                NOW_APPLY: begin
                    if (!queued) case (now_mode)
                        0: if (mode!=0) begin
                            row[0][34:33]<=0; row[2]<=0;
                            if (mode==2) begin row[3]<=row[4]; row[6]<=row[7]; end
                        end
                        1: begin
                            row[0][34:33]<=1; row[2]<=0; row[5]<=0;
                            if (mode==2) begin row[3]<=row[4]; row[6]<=row[7]; end
                        end
                        2: if (mode!=2) begin row[0][34:33]<=2; row[4]<=0; row[7]<=sum[63:0]; end
                        default: ;
                    endcase
                    phase<=PLANS;
                end
                PLANS: begin
                    plan_flags[0]<=0; plan_flags[1]<=0;
                    plan_sequence[0]<=0; plan_sequence[1]<=0;
                    step_index<=0; phase<=PLAN_STEP;
                end
                PLAN_STEP: begin
                    if (step_mode!=ABSENT) begin
                        if (composite && step_mode==2) refuse(CANNOT);
                        else if (rebased && sum[64]) refuse(EXHAUSTED);
                        else begin
                            plan_flags[step_index]<=5'd3 | {1'b0,step_mode[1:0],2'b0}
                                | ((!queued && step_mode==1) ? 5'd16 : 5'd0);
                            plan_frame[step_index]<=rebased ? sum[63:0] : frame;
                            plan_ticks[step_index]<={32'd0,step_skip}; plan_position[step_index]<=0;
                            plan_sequence[step_index]<=queued ? (step_index ? 2'd2 : 2'd1) : 2'd0;
                            if (step_index) phase<=INSERT_NOW; else step_index<=1;
                        end
                    end else if (step_index) phase<=INSERT_NOW; else step_index<=1;
                end
                INSERT_NOW: begin
                    if (queued && now_operation) begin
                        if (plan_flags[0][0] && plan_flags[1][0]) refuse(CANNOT);
                        else begin
                            if (!plan_flags[0][0]) begin
                                plan_flags[0]<=5'd1 | {1'b0,now_mode[1:0],2'b0} | (now_mode==1 ? 5'd16 : 5'd0);
                                plan_frame[0]<=frame; plan_ticks[0]<=0; plan_position[0]<=0; plan_sequence[0]<=0;
                            end else begin
                                plan_flags[1]<=5'd1 | {1'b0,now_mode[1:0],2'b0} | (now_mode==1 ? 5'd16 : 5'd0);
                                plan_frame[1]<=frame; plan_ticks[1]<=0; plan_position[1]<=0; plan_sequence[1]<=0;
                            end
                            phase<=PACK;
                        end
                    end else phase<=PACK;
                end
                PACK: begin
                    row[0][56]<=plan_flags[1][0] && (!plan_flags[0][0] || plan_sequence[1]>=plan_sequence[0]);
                    row[1]<=(row[1] & ~PLAN_MASK) | ({59'd0,plan_flags[0]}<<32) | ({59'd0,plan_flags[1]}<<37);
                    if (plan_flags[0][0]) begin row[10]<=plan_frame[0]; row[11]<=plan_ticks[0]; row[12]<=plan_position[0]; end
                    if (plan_flags[1][0]) begin row[13]<=plan_frame[1]; row[14]<=plan_ticks[1]; row[15]<=plan_position[1]; end
                    result_code<=((plan_flags[0]&3)==3 || (plan_flags[1]&3)==3) ? SCHEDULED : APPLIED;
                    word_index<=0; phase<=WRITE_WORD;
                end
                MULTIPLY: begin
                    if (multiplier[0] && (shifted_overflow || sum[64])) refuse(INVALID);
                    else begin
                        if (multiplier[0]) product<=sum[63:0];
                        multiplicand<=multiplicand<<1; shifted_overflow<=shifted_overflow || multiplicand[63];
                        multiplier<=multiplier>>1; multiply_bit<=multiply_bit+1'b1;
                        if (multiply_bit==31) phase<=ITER_CHECK;
                    end
                end
                ITER_CHECK: begin
                    if ((product>=row[3] && !(!args[65] && args[63:0]==0 && row[3]==0))
                        || (args[65] && mode!=1)) refuse(args[65] && mode!=1 && product<row[3] ? CANNOT : INVALID);
                    else begin
                        plan_flags[0]<=5'd1 | (args[65] ? 5'd12 : 5'd4) | (args[64] ? 5'd16 : 5'd0);
                        plan_flags[1]<=0; plan_sequence[0]<=0; plan_sequence[1]<=0;
                        plan_frame[0]<=frame; plan_ticks[0]<=0; plan_position[0]<=product;
                        phase<=PACK;
                    end
                end
                WRITE_WORD: if (memory_ready) phase<=WRITE_WAIT;
                WRITE_WAIT: if (memory_response_valid) begin
                    if (memory_response_error) begin write_fault<=1; refuse(CANNOT); end
                    else if (word_index==15) phase<=RESULT;
                    else begin word_index<=word_index+1'b1; phase<=WRITE_WORD; end
                end
                GAIN: if (memory_ready) phase<=GAIN_WAIT;
                GAIN_WAIT: if (memory_response_valid) begin
                    if (memory_response_error) begin write_fault<=1; refuse(CANNOT); end
                    else refuse(APPLIED);
                end
                RESULT: if (result_ready) phase<=IDLE;
                default: phase<=IDLE;
            endcase
        end
    end
endmodule
