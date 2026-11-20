    localparam integer WORK_BITS=$clog2(MIDI_WORK_WORDS), WORK_DEPTH=1<<WORK_BITS;
    localparam [4:0] FETCH=0, READ=1, EXECUTE=2, INIT=3, INDIRECT=4, FILL=5,
        MEMORY_WAIT=7, STATE_WAIT=9, HEAD_WAIT=10,
        MULTIPLY=11, MULTIPLIED=12, DIVIDE=13, CONTEXT_STORE_REQUEST=16, CONTEXT_STORE_WRITE=17,
        BULK_COUNT_READ=18, BULK_START=19, BULK_READ_REQUEST=20, BULK_READ_WAIT=21,
        BULK_WRITE_REQUEST=22, BULK_WRITE_WAIT=23, BULK_RF_READ=24, CONTEXT_ZERO=25, CONTEXT_CELL=26;
    reg [4:0] phase;
    reg [15:0] micro_pc;
    reg [63:0] instruction,operand_a,operand_b;
    (* ram_style="block" *) reg [63:0] work_a[0:WORK_DEPTH-1];
    (* ram_style="block" *) reg [63:0] work_b[0:WORK_DEPTH-1];
    reg [WORK_BITS-1:0] work_index;
    reg [15:0] fill_remaining;
    reg [LOOP_BITS-1:0] transfer_context;
    reg [31:0] context_dirty;
    wire [4:0] dirty_field=5'(first_set_bit(context_dirty));
    reg selected_dirty;
    reg [63:0] bulk_source,bulk_target;
    reg [31:0] bulk_remaining,bulk_word;
    reg [8:0] bulk_field;
    reg [WORK_BITS-1:0] bulk_register;
    wire bulk_to_ram=operation==MC_BULKIN;
    wire bulk_from_ram=operation==MC_BULKOUT;
    wire bulk_reads=operation==MC_BULKCOPY || bulk_to_ram;
    wire bulk_request=phase==BULK_READ_REQUEST || phase==BULK_WRITE_REQUEST;
    wire bulk_wait=phase==BULK_READ_WAIT || phase==BULK_WRITE_WAIT;
    wire [31:0] bulk_write_data=operation==MC_BULKINIT ?
        (bulk_remaining==1 ? 0 : initial_cell({23'b0,bulk_field})) : bulk_from_ram ? operand_a[31:0] :
        operation==MC_BULKZERO ? 0 : bulk_word;
    wire bulk_ram_write=phase==BULK_READ_WAIT && memory_response_valid && !memory_response_error && bulk_to_ram;
    reg [31:0] multiply_a,multiply_b;
    reg [63:0] multiply_result,divide_remainder,divide_dividend,divide_divisor,divide_quotient;
    reg [6:0] divide_bit;
    wire [7:0] operation=instruction[7:0];
    wire [9:0] destination_register=instruction[17:8];
    wire [15:0] target_pc=instruction[63:48];
    wire [31:0] immediate=instruction[63:32];
    wire [64:0] division_trial={1'b0,divide_remainder[62:0],divide_dividend[63]};
    wire [64:0] division_subtract=division_trial-{1'b0,divide_divisor};
    reg [63:0] alu_result;
    always @* begin
        case(operation)
            MC_MOV: alu_result=operand_a;
            MC_MOVI: alu_result={32'b0,immediate};
            MC_MOVHI: alu_result={immediate,operand_a[31:0]};
            MC_ADD: alu_result=operand_a+operand_b;
            MC_SUB: alu_result=operand_a-operand_b;
            MC_AND: alu_result=operand_a&operand_b;
            MC_OR: alu_result=operand_a|operand_b;
            MC_XOR: alu_result=operand_a^operand_b;
            MC_SHL: alu_result=operand_a<<operand_b;
            MC_SHR: alu_result=operand_a>>operand_b;
            MC_EQ: alu_result={63'b0,operand_a==operand_b};
            MC_NE: alu_result={63'b0,operand_a!=operand_b};
            MC_LT: alu_result={63'b0,operand_a<operand_b};
            MC_LE: alu_result={63'b0,operand_a<=operand_b};
            MC_GT: alu_result={63'b0,operand_a>operand_b};
            MC_GE: alu_result={63'b0,operand_a>=operand_b};
            MC_BOOL: alu_result={63'b0,operand_a!=0};
            MC_NOT: alu_result={63'b0,operand_a==0};
            MC_INV: alu_result=~operand_a;
            MC_IOREAD: alu_result=io_read_data;
            MC_PKTERR: alu_result={61'b0,packet_error(operand_a[23:0],operand_b[1:0])};
            MC_INITIALCELL: alu_result={32'b0,initial_cell(32'(operand_a))};
            MC_FIRSTBIT: alu_result={32'b0,first_set_bit(operand_a[31:0])};
            MC_EXPENSIVE: alu_result={63'b0,expensive(operand_a[23:0])};
            default: alu_result=0;
        endcase
    end
    wire alu_writes=operation==MC_MOV || operation==MC_MOVI || operation==MC_MOVHI ||
        (operation>=MC_ADD && operation<=MC_INV) || operation==MC_IOREAD ||
        operation==MC_PKTERR || operation==MC_EXPENSIVE || operation==MC_FIRSTBIT || operation==MC_INITIALCELL;
    wire work_write=phase==CONTEXT_CELL || bulk_ram_write || phase==INIT || phase==FILL || phase==INDIRECT || phase==MULTIPLIED ||
        (phase==DIVIDE && divide_bit==0) || (phase==EXECUTE && (alu_writes || operation==MC_STORE || operation==MC_CSTORE));
    wire [WORK_BITS-1:0] work_write_address=phase==EXECUTE && operation==MC_CSTORE ? WORK_BITS'(R_c)+WORK_BITS'(operand_a[4:0]) : bulk_ram_write ? bulk_register : phase==INIT || phase==FILL ? work_index :
        phase==EXECUTE && operation==MC_STORE ? operand_a[WORK_BITS-1:0] : WORK_BITS'(destination_register);
    wire [63:0] work_write_data=phase==CONTEXT_CELL ? (selected_dirty ? operand_a : {32'b0,context_q}) : bulk_ram_write ? {32'b0,memory_read_data} : phase==INIT || phase==FILL ? 0 : phase==INDIRECT ? operand_a :
        phase==MULTIPLIED ? multiply_result : phase==DIVIDE ?
            (operation==MC_DIV ? {divide_quotient[62:0],!division_subtract[64]} :
             division_subtract[64] ? division_trial[63:0] : division_subtract[63:0]) :
        operation==MC_STORE || operation==MC_CSTORE ? operand_b : alu_result;
    wire [WORK_BITS-1:0] work_read_address=phase==BULK_COUNT_READ ? WORK_BITS'(destination_register) :
        phase==BULK_RF_READ ? bulk_register : phase==CONTEXT_STORE_REQUEST ? WORK_BITS'(R_c)+WORK_BITS'(dirty_field) :
        phase==EXECUTE && operation==MC_CLOAD ? WORK_BITS'(R_c)+WORK_BITS'(operand_a[4:0]) : phase==EXECUTE ? operand_a[WORK_BITS-1:0] :
        WORK_BITS'(operation==MC_MOVHI ? destination_register : instruction[27:18]);
    always @(posedge clk) begin
        if(phase==FETCH) instruction<=midi_ucode[micro_pc[MIDI_UCODE_BITS-1:0]];
        if(phase==BULK_RF_READ || phase==BULK_COUNT_READ || phase==CONTEXT_STORE_REQUEST || phase==READ || phase==EXECUTE && (operation==MC_LOAD || operation==MC_CLOAD))
            operand_a<=work_a[work_read_address];
        if(phase==READ) operand_b<=work_b[WORK_BITS'(instruction[37:28])];
        if(work_write && !reset) begin
            work_a[work_write_address]<=work_write_data;
            work_b[work_write_address]<=work_write_data;
        end
    end
    initial begin
        if(MIDI_WORK_WORDS>1024 || MIDI_UCODE_WORDS>65536 || HEAD_BITS>64)
            $fatal(1,"MIDI execution workspace capacity exceeded");
    end
    always @(posedge clk) begin
        if(reset) begin
            phase<=INIT;work_index<=0;micro_pc<=U_BOOT;
            bulk_source<=0;bulk_target<=0;bulk_remaining<=0;bulk_word<=0;bulk_field<=0;bulk_register<=0;
            context_dirty<=0;selected_dirty<=0;transfer_context<=0;fill_remaining<=0;multiply_a<=0;multiply_b<=0;multiply_result<=0;
            divide_remainder<=0;divide_dividend<=0;divide_divisor<=0;divide_quotient<=0;divide_bit<=0;
        end else case(phase)
            INIT: if(work_index==WORK_BITS'(WORK_DEPTH-1)) begin phase<=FETCH;micro_pc<=U_BOOT;end
                else work_index<=work_index+1'b1;
            FETCH: phase<=READ;
            READ: phase<=EXECUTE;
            EXECUTE: begin
                case(operation)
                    MC_JUMP: begin micro_pc<=target_pc;phase<=FETCH;end
                    MC_JZERO: begin micro_pc<=operand_a==0 ? target_pc : micro_pc+1'b1;phase<=FETCH;end
                    MC_JIND: begin micro_pc<=operand_a[15:0];phase<=FETCH;end
                    MC_COMMAND: if(command_valid) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                    MC_REPLY: if(response_ready) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                    MC_PACKET: if(packet_ready) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                    MC_BULKCOPY,MC_BULKZERO,MC_BULKINIT,MC_BULKIN,MC_BULKOUT: begin
                        bulk_source<=operand_a;bulk_target<=operand_b;
                        bulk_register<=operation==MC_BULKIN ? operand_b[WORK_BITS-1:0] : operand_a[WORK_BITS-1:0];
                        bulk_field<=0;phase<=BULK_COUNT_READ;
                    end
                    MC_CTXZERO: begin transfer_context<=0;work_index<=0;phase<=CONTEXT_ZERO;end
                    MC_CTXREAD: begin transfer_context<=operand_a[LOOP_BITS-1:0];context_dirty<=0;micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                    MC_CTXWRITE: if(context_dirty==0) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                        else phase<=CONTEXT_STORE_REQUEST;
                    MC_CLOAD: begin selected_dirty<=context_dirty[operand_a[4:0]];phase<=CONTEXT_CELL;end
                    MC_CSTORE: begin context_dirty[operand_a[4:0]]<=1;micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                    MC_LOAD: phase<=INDIRECT;
                    MC_FILL: if(operand_b!=0) begin
                        if(operand_a==64'(R_c) && operand_b==32) context_dirty<=32'hffffffff;
                        work_index<=operand_a[WORK_BITS-1:0];fill_remaining<=operand_b[15:0];phase<=FILL;end
                        else begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                    MC_MEMREAD,MC_MEMWRITE: if(memory_ready) phase<=MEMORY_WAIT;
                    MC_STATE: if(state_ready) phase<=STATE_WAIT;
                    MC_HEADREAD: if(operand_a<64'(LOOPS)) phase<=HEAD_WAIT;
                        else begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                    MC_MUL32: begin multiply_a<=operand_a[31:0];multiply_b<=operand_b[31:0];phase<=MULTIPLY;end
                    MC_DIV,MC_MOD: begin
                        divide_remainder<=0;divide_quotient<=0;divide_dividend<=operand_a;divide_divisor<=operand_b;
                        divide_bit<=63;phase<=DIVIDE;
                    end
                    default: begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                endcase
            end
            CONTEXT_ZERO: if(work_index==31) begin
                work_index<=0;
                if(transfer_context==LOOP_BITS'(LOOPS-1)) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                else transfer_context<=transfer_context+1'b1;
            end else work_index<=work_index+1'b1;
            BULK_COUNT_READ: phase<=BULK_START;
            BULK_START: begin
                bulk_remaining<=operand_a[31:0];
                if(operand_a==0) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                else phase<=bulk_reads ? BULK_READ_REQUEST : bulk_from_ram ? BULK_RF_READ : BULK_WRITE_REQUEST;
            end
            BULK_RF_READ: phase<=BULK_WRITE_REQUEST;
            BULK_READ_REQUEST: if(memory_ready) phase<=BULK_READ_WAIT;
            BULK_WRITE_REQUEST: if(memory_ready) phase<=BULK_WRITE_WAIT;
            BULK_READ_WAIT: if(memory_response_valid) begin
                if(memory_response_error) begin micro_pc<=target_pc;phase<=FETCH;end
                else begin
                    bulk_word<=memory_read_data;
                    if(!bulk_to_ram) phase<=BULK_WRITE_REQUEST;
                    else if(bulk_remaining==1) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                    else begin
                        bulk_source<=bulk_source+4;bulk_register<=bulk_register+1'b1;
                        bulk_remaining<=bulk_remaining-1'b1;phase<=BULK_READ_REQUEST;
                    end
                end
            end
            BULK_WRITE_WAIT: if(memory_response_valid) begin
                if(memory_response_error) begin micro_pc<=target_pc;phase<=FETCH;end
                else if(bulk_remaining==1) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                else begin
                    bulk_source<=bulk_source+4;bulk_target<=bulk_target+4;
                    bulk_register<=bulk_register+1'b1;bulk_remaining<=bulk_remaining-1'b1;
                    bulk_field<=bulk_field==263 ? 0 : bulk_field+1'b1;
                    phase<=bulk_reads ? BULK_READ_REQUEST : bulk_from_ram ? BULK_RF_READ : BULK_WRITE_REQUEST;
                end
            end
            CONTEXT_STORE_REQUEST: begin work_index<=WORK_BITS'(dirty_field);phase<=CONTEXT_STORE_WRITE;end
            CONTEXT_STORE_WRITE: begin
                context_dirty<=context_dirty & ~(32'b1<<work_index[4:0]);
                if((context_dirty & ~(32'b1<<work_index[4:0]))==0) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                else phase<=CONTEXT_STORE_REQUEST;
            end
            CONTEXT_CELL: begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
            INDIRECT,HEAD_WAIT,MULTIPLIED: begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
            FILL: if(fill_remaining==1) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                else begin fill_remaining<=fill_remaining-1'b1;work_index<=work_index+1'b1;end
            MULTIPLY: begin multiply_result<=64'(multiply_a)*64'(multiply_b);phase<=MULTIPLIED;end
            DIVIDE: begin
                divide_remainder<=division_subtract[64] ? division_trial[63:0] : division_subtract[63:0];
                divide_dividend<={divide_dividend[62:0],1'b0};
                divide_quotient<={divide_quotient[62:0],!division_subtract[64]};
                if(divide_bit==0) begin micro_pc<=micro_pc+1'b1;phase<=FETCH;end
                else divide_bit<=divide_bit-1'b1;
            end
            MEMORY_WAIT: if(memory_response_valid) begin
                micro_pc<=memory_response_error ? target_pc : micro_pc+1'b1;phase<=FETCH;
            end
            STATE_WAIT: if(state_valid) begin micro_pc<=state_error==7 ? target_pc : micro_pc+1'b1;phase<=FETCH;end
            default: phase<=INIT;
        endcase
    end

// synthesis translate_off
    always @(posedge clk) if(!reset) begin
        if(phase==EXECUTE && operation==MC_CTXWRITE && operand_a!=64'(transfer_context))
            $fatal(1,"MIDI metadata commit changed selected context");
        if(phase==EXECUTE && (operation==MC_CLOAD || operation==MC_CSTORE) && operand_a>=32)
            $fatal(1,"MIDI context field outside descriptor");
        if(phase==FETCH && 32'(micro_pc)>=MIDI_UCODE_WORDS) $fatal(1,"MIDI instruction outside program");
        if(phase==EXECUTE && (operation==MC_LOAD || operation==MC_STORE) && operand_a>=64'(MIDI_WORK_WORDS))
            $fatal(1,"MIDI workspace index outside prepared capacity");
    end
// synthesis translate_on
