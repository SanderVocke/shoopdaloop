module fifo_group_banks (
    input wire clk, input wire reset,
    input wire [31:0] epoch, input wire fault,
    input wire host_valid, output wire host_ready,
    input wire [2:0] host_operation, input wire [1:0] host_bank,
    input wire [31:0] host_epoch, input wire [31:0] host_word,
    input wire [3:0] host_strobe, input wire [63:0] host_sequence,
    output reg host_response_valid, output reg [3:0] host_receipt,
    input wire [1:0] inspect_bank,
    output wire [2:0] bank_state, output wire [8:0] written_words,
    output wire [3:0] free_mask, output wire [3:0] result_ready_mask,
    output wire [63:0] next_sequence, output reg [63:0] last_admitted,
    output wire sequence_exhausted,
    output wire head_valid, output wire [1:0] head_bank,
    output reg [63:0] head_sequence, input wire take,
    input wire command_read_valid, input wire [1:0] command_read_bank,
    input wire [8:0] command_read_word,
    output reg command_response_valid, output reg command_response_error,
    output reg [31:0] command_read_data,
    input wire result_write_valid, output wire result_write_ready,
    input wire [1:0] result_write_bank, input wire [5:0] result_write_word,
    input wire [31:0] result_write_data,
    input wire finish_valid, input wire [1:0] finish_bank,
    input wire [31:0] finish_epoch, input wire [63:0] finish_sequence,
    input wire finish_wait_fence, input wire finish_fault,
    input wire fence_valid, input wire [1:0] fence_bank,
    input wire [31:0] fence_epoch, input wire [63:0] fence_sequence,
    input wire fence_fault,
    input wire result_read_valid, input wire [1:0] result_read_bank,
    input wire [5:0] result_read_word,
    output reg result_response_valid, output reg result_response_error,
    output reg [31:0] result_read_data,
    output reg protocol_fault
);
    localparam [2:0] FREE=0, FILLING=1, QUEUED=2, EXECUTING=3,
        WAITING_FENCE=4, RESULT_READY=5, RETAINED_FAULT=6;
    localparam [3:0] ACCEPTED=0, FULL=1, BAD_EPOCH=2, FAULTED=3,
        INVALID=4, UNAVAILABLE=5, OLD_SEQUENCE=6, INCOMPLETE=7, EXHAUSTED=8;
    (* ram_style="block" *) reg [31:0] commands [0:2047];
    (* ram_style="block" *) reg [31:0] results [0:255];
    reg [2:0] state [0:3];
    reg [8:0] cursor [0:3];
    reg [5:0] count [0:3], result_written [0:3];
    reg [63:0] bank_sequence [0:3];
    reg bad_header [0:3], bad_epoch [0:3], deadline [0:3];
    reg [1:0] order [0:3];
    reg [1:0] enqueue_index, execute_index, release_index, executing_bank;
    reg [2:0] admitted_count, queued_count;
    reg execution_active, release_exhausted, retained_fault;
    reg [31:0] incarnation;
    reg [63:0] release_sequence;
    reg [3:0] receipt;
    wire active_epoch = epoch!=0 && epoch==incarnation;
    wire stopped = fault || protocol_fault || retained_fault;
    wire [1:0] release_bank = order[release_index];
    wire [8:0] required_words = 9'd16 + ({3'd0,count[host_bank]}<<3)
        + ({3'd0,count[host_bank]}<<2);
    wire [5:0] expected_result_word = result_written[result_write_bank]<count[result_write_bank]
        ? 6'd16+result_written[result_write_bank]
        : result_written[result_write_bank]-count[result_write_bank];
    wire [5:0] result_length = 6'd16+count[result_write_bank];
    wire take_accepted = take && head_valid;
    wire host_accepted = host_valid && host_ready && receipt==ACCEPTED;
    wire publish_accepted = host_accepted && host_operation==2;
    wire release_accepted = host_accepted && host_operation==3
        && host_sequence==release_sequence && admitted_count!=0;
    wire data_accepted = host_accepted && host_operation==1;
    wire write_accepted = result_write_valid && result_write_ready;
    wire finish_matches = finish_valid && active_epoch && finish_epoch==incarnation
        && finish_sequence==bank_sequence[finish_bank] && execution_active
        && finish_bank==executing_bank && state[finish_bank]==EXECUTING;
    wire fence_matches = fence_valid && active_epoch && fence_epoch==incarnation
        && fence_sequence==bank_sequence[fence_bank] && state[fence_bank]==WAITING_FENCE;
    wire [6:0] finish_written = {1'b0,result_written[finish_bank]}
        + (write_accepted && result_write_bank==finish_bank ? 7'd1 : 7'd0);
    wire [6:0] fence_written = {1'b0,result_written[fence_bank]}
        + (write_accepted && result_write_bank==fence_bank ? 7'd1 : 7'd0);
    wire command_read_allowed = active_epoch && state[command_read_bank]==EXECUTING
        && command_read_word<cursor[command_read_bank];
    wire result_read_allowed = active_epoch && admitted_count!=0
        && result_read_bank==release_bank && state[release_bank]==RESULT_READY
        && {1'b0,result_read_word}<7'd16+{1'b0,count[release_bank]};

    assign host_ready = !reset;
    assign bank_state = state[inspect_bank];
    assign written_words = cursor[inspect_bank];
    assign sequence_exhausted = &last_admitted;
    assign next_sequence = sequence_exhausted ? 64'd0 : last_admitted+64'd1;
    assign head_bank = order[execute_index];
    assign head_valid = !reset && active_epoch && !stopped && !execution_active && queued_count!=0;
    assign result_ready_mask = !reset && active_epoch && admitted_count!=0
        && state[release_bank]==RESULT_READY ? 4'b0001<<release_bank : 4'd0;
    assign result_write_ready = !reset && active_epoch && !stopped
        && (state[result_write_bank]==EXECUTING || state[result_write_bank]==WAITING_FENCE)
        && result_written[result_write_bank]<result_length
        && result_write_word==expected_result_word;
    for (genvar g=0;g<4;g=g+1) begin: free_banks
        assign free_mask[g] = !reset && active_epoch && state[g]==FREE;
    end

    always @* begin
        receipt=ACCEPTED;
        if (!active_epoch || (host_operation!=1 && host_epoch!=incarnation)) receipt=BAD_EPOCH;
        else if (stopped) receipt=FAULTED;
        else case (host_operation)
            0: if (host_strobe!=4'hf) receipt=INVALID;
               else if (state[host_bank]!=FREE && state[host_bank]!=FILLING) receipt=FULL;
            1: if (state[host_bank]!=FILLING) receipt=UNAVAILABLE;
               else if (host_strobe!=4'hf) receipt=INCOMPLETE;
               else if (cursor[host_bank]>=9'd400) receipt=INVALID;
            2: if (host_strobe!=4'hf) receipt=INVALID;
               else if (state[host_bank]!=FILLING) begin
                   if (bank_sequence[host_bank]!=0 && bank_sequence[host_bank]<=last_admitted) receipt=OLD_SEQUENCE;
                   else receipt=UNAVAILABLE;
               end else if (bad_epoch[host_bank]) receipt=BAD_EPOCH;
               else if (bad_header[host_bank] || count[host_bank]==0) receipt=INVALID;
               else if (cursor[host_bank]!=required_words) receipt=INCOMPLETE;
               else if (sequence_exhausted) receipt=EXHAUSTED;
               else if (bank_sequence[host_bank]==0) receipt=INVALID;
               else if (bank_sequence[host_bank]<=last_admitted) receipt=OLD_SEQUENCE;
               else if (bank_sequence[host_bank]!=next_sequence) receipt=INVALID;
               else if (admitted_count==4) receipt=FULL;
            3: if (host_strobe!=4'hf || host_sequence==0) receipt=INVALID;
               else if (release_exhausted || host_sequence<release_sequence) receipt=ACCEPTED;
               else if (admitted_count==0 || host_bank!=release_bank
                   || host_sequence!=release_sequence || state[release_bank]!=RESULT_READY) receipt=UNAVAILABLE;
            default: receipt=INVALID;
        endcase
    end

    always @(posedge clk) begin
        if (data_accepted) commands[{host_bank,cursor[host_bank]}]<=host_word;
        if (command_read_valid && command_read_allowed)
            command_read_data<=commands[{command_read_bank,command_read_word}];
        if (write_accepted) results[{result_write_bank,result_write_word}]<=result_write_data;
        if (result_read_valid && result_read_allowed)
            result_read_data<=results[{result_read_bank,result_read_word}];
    end

    always @(posedge clk) begin
        if (reset) begin
            incarnation<=epoch; host_response_valid<=0; host_receipt<=0;
            last_admitted<=0; head_sequence<=1; release_sequence<=1;
            enqueue_index<=0; execute_index<=0; release_index<=0;
            executing_bank<=0; execution_active<=0; admitted_count<=0; queued_count<=0;
            release_exhausted<=0; retained_fault<=0;
            command_response_valid<=0; command_response_error<=0;
            result_response_valid<=0; result_response_error<=0; protocol_fault<=0;
            for (integer i=0;i<4;i=i+1) begin
                state[i]<=FREE; cursor[i]<=0; count[i]<=0; bank_sequence[i]<=0;
                result_written[i]<=0; bad_header[i]<=0; bad_epoch[i]<=0; deadline[i]<=0;
                order[i]<=0;
            end
        end else begin
            host_response_valid<=host_valid && host_ready;
            if (host_valid && host_ready) host_receipt<=receipt;
            command_response_valid<=command_read_valid;
            command_response_error<=command_read_valid && !command_read_allowed;
            result_response_valid<=result_read_valid;
            result_response_error<=result_read_valid && !result_read_allowed;
            case ({publish_accepted,release_accepted})
                2'b10: admitted_count<=admitted_count+1'b1;
                2'b01: admitted_count<=admitted_count-1'b1;
                default: ;
            endcase
            case ({publish_accepted,take_accepted})
                2'b10: queued_count<=queued_count+1'b1;
                2'b01: queued_count<=queued_count-1'b1;
                default: ;
            endcase
            if (host_accepted) case (host_operation)
                0: begin
                    state[host_bank]<=FILLING; cursor[host_bank]<=0; count[host_bank]<=0;
                    bank_sequence[host_bank]<=0; bad_header[host_bank]<=0; bad_epoch[host_bank]<=0;
                    deadline[host_bank]<=0; result_written[host_bank]<=0;
                end
                1: begin
                    cursor[host_bank]<=cursor[host_bank]+1'b1;
                    if (cursor[host_bank]<16) case (cursor[host_bank][3:0])
                        0: if (host_word!=32'h53434631) bad_header[host_bank]<=1;
                        1: begin
                            if (host_word[15:0]!=16 || host_word[31:16]==0 || host_word[31:16]>32)
                                bad_header[host_bank]<=1;
                            count[host_bank]<=host_word[21:16];
                        end
                        2: if (host_word!=incarnation) bad_epoch[host_bank]<=1;
                        3: begin
                            deadline[host_bank]<=host_word[0];
                            if (host_word[31:1]!=0) bad_header[host_bank]<=1;
                        end
                        4: bank_sequence[host_bank][31:0]<=host_word;
                        5: bank_sequence[host_bank][63:32]<=host_word;
                        10,11: if (!deadline[host_bank] && host_word!=0) bad_header[host_bank]<=1;
                        12,13,14,15: if (host_word!=0) bad_header[host_bank]<=1;
                        default: ;
                    endcase
                end
                2: begin
                    state[host_bank]<=QUEUED; order[enqueue_index]<=host_bank;
                    enqueue_index<=enqueue_index+1'b1; last_admitted<=bank_sequence[host_bank];
                end
                3: if (release_accepted) begin
                    state[host_bank]<=FREE; release_index<=release_index+1'b1;
                    if (&release_sequence) release_exhausted<=1;
                    else release_sequence<=release_sequence+1'b1;
                end
                default: ;
            endcase
            if (take_accepted) begin
                state[head_bank]<=EXECUTING; executing_bank<=head_bank; execution_active<=1;
                execute_index<=execute_index+1'b1;
                if (!(&head_sequence)) head_sequence<=head_sequence+1'b1;
            end
            if (write_accepted) result_written[result_write_bank]<=result_written[result_write_bank]+1'b1;
            if (result_write_valid && !result_write_ready) protocol_fault<=1;
            if (finish_matches) begin
                execution_active<=0;
                if (finish_fault) begin state[finish_bank]<=RETAINED_FAULT; retained_fault<=1; end
                else if (finish_wait_fence && finish_written>={1'b0,count[finish_bank]})
                    state[finish_bank]<=WAITING_FENCE;
                else if (!finish_wait_fence && finish_written==7'd16+{1'b0,count[finish_bank]})
                    state[finish_bank]<=RESULT_READY;
                else begin state[finish_bank]<=RETAINED_FAULT; protocol_fault<=1; end
            end
            if (fence_matches) begin
                if (fence_fault) begin state[fence_bank]<=RETAINED_FAULT; retained_fault<=1; end
                else if (fence_written==7'd16+{1'b0,count[fence_bank]}) state[fence_bank]<=RESULT_READY;
                else begin state[fence_bank]<=RETAINED_FAULT; protocol_fault<=1; end
            end
        end
    end
endmodule
