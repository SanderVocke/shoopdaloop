module fifo_mmio #(
    parameter integer LOOPS=128, SYSTEM_REGISTERS=0, EPOCH_RESET_COUPLED=0,
    parameter [31:0] CORE_CLOCK_HZ=100_000_000, ARENA_BASE=32'h30000000,
    parameter [63:0] ARENA_BYTES=64'h08000000,
    parameter integer PROGRAM_DEPTH=0, STATE_DEPTH=0, JOB_DEPTH=0,
    parameter integer BUFFERS=0, VIEWS=0, MEDIA_BLOCKS=0,
    parameter integer CHUNK_FRAMES=0, BLOCK_CHUNKS=0, QUANTUM=32
) (
    input wire clk, reset, runtime_reset, input wire [31:0] epoch,
    input wire faulted, input wire [2:0] recovery_state,
    output wire recovery_request, output wire [31:0] recovery_epoch, output wire irq, abort_request,
    input wire quantum_done, input wire [31:0] quantum_cycles, audio_xruns,
    input wire [31:0] first_fault_epoch, first_fault_cause, first_fault_op,
    input wire [31:0] first_fault_cycles, first_fault_control_cycles, first_fault_detail,
    input wire [63:0] first_fault_revision, first_fault_frame,
    input wire [11:0] s_axi_awaddr, input wire s_axi_awvalid, output wire s_axi_awready,
    input wire [31:0] s_axi_wdata, input wire [3:0] s_axi_wstrb,
    input wire s_axi_wvalid, output wire s_axi_wready,
    output reg [1:0] s_axi_bresp, output reg s_axi_bvalid, input wire s_axi_bready,
    input wire [11:0] s_axi_araddr, input wire s_axi_arvalid, output wire s_axi_arready,
    output reg [31:0] s_axi_rdata, output reg [1:0] s_axi_rresp,
    output reg s_axi_rvalid, input wire s_axi_rready,
    output wire host_valid, input wire host_ready,
    output wire [2:0] host_operation, output wire [1:0] host_bank,
    output wire [31:0] host_epoch, host_word, output wire [3:0] host_strobe,
    output wire [63:0] host_sequence,
    input wire host_response_valid, input wire [3:0] host_receipt,
    output wire [1:0] inspect_bank,
    input wire [2:0] bank_state, input wire [8:0] written_words,
    input wire [3:0] free_mask, result_ready_mask,
    input wire [63:0] next_sequence, last_admitted, cutoff_sequence, groups_attempted,
    input wire [31:0] peak_command_cycles, peak_snapshot_cycles,
    input wire [63:0] fault_sequence,
    output wire result_read_valid, output wire [1:0] result_read_bank,
    output wire [5:0] result_read_word,
    input wire result_response_valid, result_response_error, input wire [31:0] result_read_data,
    input wire [1:0] snapshot_ready_mask,
    input wire [63:0] snapshot_skipped, input wire snapshot_sequence_exhausted,
    output wire snapshot_read_valid, output wire snapshot_read_bank,
    output wire [12:0] snapshot_read_word,
    input wire snapshot_response_valid, snapshot_response_error, input wire [31:0] snapshot_read_data,
    output wire snapshot_ack_valid, input wire snapshot_ack_ready, output wire snapshot_ack_bank,
    output wire [31:0] snapshot_ack_epoch, output wire [63:0] snapshot_ack_sequence,
    input wire snapshot_ack_response_valid, input wire [3:0] snapshot_ack_receipt
);
    import semantic_registers::*;
    localparam [1:0] OKAY=0, SLVERR=2;
    localparam [1:0] WRITE_NONE=0, WRITE_COMMAND=1, WRITE_RESULT=2, WRITE_SNAPSHOT=3;
    localparam [1:0] READ_NONE=0, READ_RESULT=1, READ_SNAPSHOT=2;
    localparam [12:0] SNAPSHOT_WORDS=13'(16+LOOPS*38);
    initial begin
        if (LOOPS<1 || LOOPS>128) $fatal(1,"semantic MMIO extent must fit 128 rows");
        if (SYSTEM_REGISTERS!=0 && (ARENA_BYTES==0 || ARENA_BYTES>64'hffffffff))
            $fatal(1,"semantic discovery arena must fit a nonzero u32 extent");
    end
    reg [31:0] completed_quanta, peak_quantum_cycles;
    reg [3:0] submission_receipt;
    reg [31:0] live_epoch;
    reg aw_pending, w_pending, aw_current, w_current;
    reg [11:0] write_address;
    reg [31:0] write_data;
    reg [3:0] write_strobes;
    reg [1:0] write_owner, read_owner;
    reg read_current, read_advance;
    reg [1:0] command_bank, result_bank;
    reg snapshot_bank;
    reg [5:0] result_cursor;
    reg [12:0] snapshot_cursor;
    reg [63:0] result_ack_sequence, saved_snapshot_sequence;
    reg [31:0] result_ack_epoch, saved_snapshot_epoch;
    reg [3:0] command_receipt, result_receipt, snapshot_receipt;
    wire epoch_changed=EPOCH_RESET_COUPLED==0 && epoch!=live_epoch;
    wire live=!reset && !runtime_reset && epoch!=0 && (EPOCH_RESET_COUPLED!=0 || epoch==live_epoch);
    wire write_current=live && aw_current && w_current;
    wire write_complete=aw_pending && w_pending && write_owner==WRITE_NONE && !s_axi_bvalid;
    wire read_accept=s_axi_arvalid && s_axi_arready;
    reg write_format;
    wire command_request=write_address==REG_COMMAND_BEGIN || write_address==REG_COMMAND_DATA
        || write_address==REG_COMMAND_PUBLISH || write_address==REG_RESULT_ACK;
    wire snapshot_request=write_address==REG_SNAPSHOT_ACK;
    wire recovery_access=SYSTEM_REGISTERS!=0 && write_address==REG_RECOVERY_REQUEST;
    wire abort_access=SYSTEM_REGISTERS!=0 && write_address==REG_ABORT;
    assign abort_request=write_complete && write_format && write_current && abort_access && write_data==epoch;
    wire recovery_available=recovery_state==0 || recovery_state==3;
    wire system_fault=faulted || recovery_state==4;
    assign recovery_request=write_complete && write_format && write_current && recovery_access
        && write_data==epoch && recovery_available;
    assign recovery_epoch=write_data;
    assign irq=SYSTEM_REGISTERS!=0 && !reset && epoch!=0
        && (system_fault || result_ready_mask!=0 || snapshot_ready_mask!=0);
    wire result_access=read_accept && s_axi_araddr==REG_RESULT_DATA && live && result_cursor<48;
    wire snapshot_access=read_accept && s_axi_araddr==REG_SNAPSHOT_DATA && live && snapshot_cursor<SNAPSHOT_WORDS;
    assign s_axi_awready=!reset && !aw_pending && !s_axi_bvalid && write_owner==WRITE_NONE;
    assign s_axi_wready=!reset && !w_pending && !s_axi_bvalid && write_owner==WRITE_NONE;
    assign s_axi_arready=!reset && !s_axi_rvalid && read_owner==READ_NONE;
    assign host_valid=write_complete && write_format && write_current && command_request;
    assign host_operation=write_address==REG_COMMAND_DATA ? 3'd1
        : write_address==REG_COMMAND_PUBLISH ? 3'd2 : write_address==REG_RESULT_ACK ? 3'd3 : 3'd0;
    assign host_bank=write_address==REG_RESULT_ACK ? result_bank : command_bank;
    assign host_epoch=write_address==REG_RESULT_ACK ? result_ack_epoch : write_data;
    assign host_word=write_data;
    assign host_strobe=write_strobes;
    assign host_sequence=result_ack_sequence;
    assign inspect_bank=command_bank;
    assign result_read_valid=result_access;
    assign result_read_bank=result_bank;
    assign result_read_word=result_cursor;
    assign snapshot_read_valid=snapshot_access;
    assign snapshot_read_bank=snapshot_bank;
    assign snapshot_read_word=snapshot_cursor;
    assign snapshot_ack_valid=write_complete && write_format && write_current && snapshot_request;
    assign snapshot_ack_bank=snapshot_bank;
    assign snapshot_ack_epoch=saved_snapshot_epoch;
    assign snapshot_ack_sequence=saved_snapshot_sequence;

    always @* begin
        write_format=write_address[1:0]==0 && write_strobes==4'hf;
        case (write_address)
            REG_COMMAND_BANK, REG_RESULT_BANK: write_format=write_format && write_data[31:2]==0;
            REG_SNAPSHOT_BANK: write_format=write_format && write_data[31:1]==0;
            REG_RESULT_REWIND, REG_SNAPSHOT_REWIND, REG_RESULT_ACK, REG_SNAPSHOT_ACK:
                write_format=write_format && write_data==1;
            REG_COMMAND_DATA: write_format=1;
            REG_ABORT, REG_RECOVERY_REQUEST: write_format=write_format && SYSTEM_REGISTERS!=0;
            REG_COMMAND_BEGIN, REG_COMMAND_PUBLISH, REG_RESULT_ACK_LOW, REG_RESULT_ACK_HIGH,
            REG_RESULT_ACK_EPOCH, REG_SNAPSHOT_ACK_LOW, REG_SNAPSHOT_ACK_HIGH, REG_SNAPSHOT_ACK_EPOCH: ;
            default: write_format=0;
        endcase
    end
    task automatic respond_write(input [1:0] response);
        begin
            aw_pending<=0; w_pending<=0; write_owner<=WRITE_NONE;
            s_axi_bvalid<=1; s_axi_bresp<=response;
        end
    endtask
    task automatic refuse_write_epoch;
        begin
            if (write_address<REG_SEMANTIC_ID) submission_receipt<=RECEIPT_EPOCH;
            else if (write_address>=REG_SNAPSHOT_READY) snapshot_receipt<=RECEIPT_EPOCH;
            else if (write_address>=REG_RESULT_READY) result_receipt<=RECEIPT_EPOCH;
            else command_receipt<=RECEIPT_EPOCH;
        end
    endtask
    always @(posedge clk) begin
        if (reset) begin
            completed_quanta<=0; peak_quantum_cycles<=0; submission_receipt<=RECEIPT_UNAVAILABLE;
            live_epoch<=0; aw_pending<=0; w_pending<=0; aw_current<=0; w_current<=0;
            write_address<=0; write_data<=0; write_strobes<=0;
            write_owner<=WRITE_NONE; read_owner<=READ_NONE; read_current<=0; read_advance<=0;
            s_axi_bvalid<=0; s_axi_bresp<=OKAY; s_axi_rvalid<=0; s_axi_rresp<=OKAY; s_axi_rdata<=0;
            command_bank<=0; result_bank<=0; snapshot_bank<=0; result_cursor<=0; snapshot_cursor<=0;
            result_ack_sequence<=0; saved_snapshot_sequence<=0; result_ack_epoch<=0; saved_snapshot_epoch<=0;
            command_receipt<=RECEIPT_UNAVAILABLE; result_receipt<=RECEIPT_UNAVAILABLE;
            snapshot_receipt<=RECEIPT_UNAVAILABLE;
        end else begin
            live_epoch<=epoch;
            if (SYSTEM_REGISTERS!=0 && live && quantum_done) begin
                if (!(&completed_quanta)) completed_quanta<=completed_quanta+1'b1;
                if (quantum_cycles>peak_quantum_cycles) peak_quantum_cycles<=quantum_cycles;
            end
            if (s_axi_awvalid && s_axi_awready) begin
                aw_pending<=1; write_address<=s_axi_awaddr; aw_current<=live;
            end
            if (s_axi_wvalid && s_axi_wready) begin
                w_pending<=1; write_data<=s_axi_wdata; write_strobes<=s_axi_wstrb; w_current<=live;
            end
            if (s_axi_bready) s_axi_bvalid<=0;
            if (s_axi_rready) s_axi_rvalid<=0;
            if (read_owner!=READ_NONE) begin
                if (!live || !read_current) begin
                    read_owner<=READ_NONE; s_axi_rvalid<=1; s_axi_rresp<=OKAY; s_axi_rdata<=0;
                    if (read_owner==READ_RESULT) result_receipt<=RECEIPT_EPOCH;
                    else snapshot_receipt<=RECEIPT_EPOCH;
                end else if (read_owner==READ_RESULT && result_response_valid) begin
                    read_owner<=READ_NONE; s_axi_rvalid<=1; s_axi_rresp<=OKAY;
                    s_axi_rdata<=result_response_error ? 32'd0 : result_read_data;
                    result_receipt<=result_response_error ? RECEIPT_UNAVAILABLE : RECEIPT_ACCEPTED;
                    if (!result_response_error && read_advance) result_cursor<=result_cursor+1'b1;
                end else if (read_owner==READ_SNAPSHOT && snapshot_response_valid) begin
                    read_owner<=READ_NONE; s_axi_rvalid<=1; s_axi_rresp<=OKAY;
                    s_axi_rdata<=snapshot_response_error ? 32'd0 : snapshot_read_data;
                    snapshot_receipt<=snapshot_response_error ? RECEIPT_UNAVAILABLE : RECEIPT_ACCEPTED;
                    if (!snapshot_response_error && read_advance) snapshot_cursor<=snapshot_cursor+1'b1;
                end
            end
            if (read_accept) begin
                s_axi_rvalid<=1; s_axi_rresp<=OKAY; s_axi_rdata<=0;
                if (SYSTEM_REGISTERS!=0 && s_axi_araddr<REG_SEMANTIC_ID) begin
                    case (s_axi_araddr)
                        REG_ID: s_axi_rdata<=SEMANTIC_DISCOVERY_ID;
                        REG_VERSION: s_axi_rdata<=SEMANTIC_VERSION;
                        REG_STATUS: s_axi_rdata<={29'd0,system_fault,2'd0};
                        REG_EPOCH: s_axi_rdata<=epoch;
                        REG_CORE_CLOCK_HZ: s_axi_rdata<=CORE_CLOCK_HZ;
                        REG_ARENA_BASE: s_axi_rdata<=ARENA_BASE;
                        REG_ARENA_BYTES: s_axi_rdata<=ARENA_BYTES[31:0];
                        REG_PROGRAM_DEPTH: s_axi_rdata<=32'(PROGRAM_DEPTH);
                        REG_STATE_DEPTH: s_axi_rdata<=32'(STATE_DEPTH);
                        REG_JOB_DEPTH: s_axi_rdata<=32'(JOB_DEPTH);
                        REG_BUFFERS: s_axi_rdata<=32'(BUFFERS);
                        REG_VIEWS: s_axi_rdata<=32'(VIEWS);
                        REG_MEDIA_BLOCKS: s_axi_rdata<=32'(MEDIA_BLOCKS);
                        REG_CHUNK_FRAMES: s_axi_rdata<=32'(CHUNK_FRAMES);
                        REG_BLOCK_CHUNKS: s_axi_rdata<=32'(BLOCK_CHUNKS);
                        REG_QUANTUM: s_axi_rdata<=32'(QUANTUM);
                        REG_COMPLETED_QUANTA: s_axi_rdata<=completed_quanta;
                        REG_PEAK_QUANTUM_CYCLES: s_axi_rdata<=peak_quantum_cycles;
                        REG_AUDIO_XRUNS: s_axi_rdata<=audio_xruns;
                        REG_RECOVERY_ID: s_axi_rdata<=32'h52535431;
                        REG_RECOVERY_STATUS: s_axi_rdata<={29'd0,recovery_state};
                        REG_SUBMISSION_RESULT: s_axi_rdata<={28'd0,submission_receipt};
                        REG_FAULT_EPOCH: s_axi_rdata<=first_fault_epoch;
                        REG_FAULT_CAUSE: s_axi_rdata<=first_fault_cause;
                        REG_FAULT_TAG: s_axi_rdata<=fault_sequence[31:0];
                        REG_FAULT_OP: s_axi_rdata<=first_fault_op;
                        REG_FAULT_CYCLES: s_axi_rdata<=first_fault_cycles;
                        REG_FAULT_CONTROL_CYCLES: s_axi_rdata<=first_fault_control_cycles;
                        REG_FAULT_DETAIL: s_axi_rdata<=first_fault_detail;
                        REG_FAULT_REVISION_LOW: s_axi_rdata<=first_fault_revision[31:0];
                        REG_FAULT_REVISION_HIGH: s_axi_rdata<=first_fault_revision[63:32];
                        REG_FAULT_FRAME_LOW: s_axi_rdata<=first_fault_frame[31:0];
                        REG_FAULT_FRAME_HIGH: s_axi_rdata<=first_fault_frame[63:32];
                        default: s_axi_rresp<=SLVERR;
                    endcase
                end else case (s_axi_araddr)
                    REG_SEMANTIC_ID: s_axi_rdata<=SEMANTIC_ID;
                    REG_SEMANTIC_CAPABILITIES: s_axi_rdata<=SEMANTIC_CAPABILITIES;
                    REG_GROUP_BANKS: s_axi_rdata<=SEMANTIC_GROUP_BANKS;
                    REG_COMMANDS_PER_GROUP: s_axi_rdata<=SEMANTIC_COMMANDS_PER_GROUP;
                    REG_SNAPSHOT_LOOP_LIMIT: s_axi_rdata<=32'(LOOPS);
                    REG_OBSERVATION_ROW_WORDS: s_axi_rdata<=SEMANTIC_ROW_WORDS;
                    REG_COMMAND_CYCLE_LIMIT: s_axi_rdata<=SEMANTIC_COMMAND_CYCLES;
                    REG_SNAPSHOT_CYCLE_LIMIT: s_axi_rdata<=SEMANTIC_SNAPSHOT_CYCLES;
                    REG_QUANTUM_CYCLE_LIMIT: s_axi_rdata<=SEMANTIC_QUANTUM_CYCLES;
                    REG_FAULT_SEQUENCE_HIGH: s_axi_rdata<=fault_sequence[63:32];
                    REG_NEXT_SEQUENCE_LOW: s_axi_rdata<=next_sequence[31:0];
                    REG_NEXT_SEQUENCE_HIGH: s_axi_rdata<=next_sequence[63:32];
                    REG_FREE_BANKS: s_axi_rdata<={28'b0,free_mask};
                    REG_COMMAND_BANK: s_axi_rdata<={30'b0,command_bank};
                    REG_COMMAND_BANK_STATE: s_axi_rdata<={29'b0,bank_state};
                    REG_COMMAND_RECEIPT: s_axi_rdata<={28'b0,command_receipt};
                    REG_COMMAND_WRITTEN: s_axi_rdata<={23'b0,written_words};
                    REG_LAST_ADMITTED_LOW: s_axi_rdata<=last_admitted[31:0];
                    REG_LAST_ADMITTED_HIGH: s_axi_rdata<=last_admitted[63:32];
                    REG_LAST_CUTOFF_LOW: s_axi_rdata<=cutoff_sequence[31:0];
                    REG_LAST_CUTOFF_HIGH: s_axi_rdata<=cutoff_sequence[63:32];
                    REG_GROUPS_ATTEMPTED_LOW: s_axi_rdata<=groups_attempted[31:0];
                    REG_GROUPS_ATTEMPTED_HIGH: s_axi_rdata<=groups_attempted[63:32];
                    REG_PEAK_COMMAND_CYCLES: s_axi_rdata<=peak_command_cycles;
                    REG_PEAK_SNAPSHOT_CYCLES: s_axi_rdata<=peak_snapshot_cycles;
                    REG_RESULT_READY: s_axi_rdata<={28'b0,result_ready_mask};
                    REG_RESULT_BANK: s_axi_rdata<={30'b0,result_bank};
                    REG_RESULT_ACK_LOW: s_axi_rdata<=result_ack_sequence[31:0];
                    REG_RESULT_ACK_HIGH: s_axi_rdata<=result_ack_sequence[63:32];
                    REG_RESULT_ACK_EPOCH: s_axi_rdata<=result_ack_epoch;
                    REG_RESULT_RECEIPT: s_axi_rdata<={28'b0,result_receipt};
                    REG_SNAPSHOT_READY: s_axi_rdata<={30'b0,snapshot_ready_mask};
                    REG_SNAPSHOT_BANK: s_axi_rdata<={31'b0,snapshot_bank};
                    REG_SNAPSHOT_ACK_LOW: s_axi_rdata<=saved_snapshot_sequence[31:0];
                    REG_SNAPSHOT_ACK_HIGH: s_axi_rdata<=saved_snapshot_sequence[63:32];
                    REG_SNAPSHOT_ACK_EPOCH: s_axi_rdata<=saved_snapshot_epoch;
                    REG_SNAPSHOT_RECEIPT: s_axi_rdata<={28'b0,snapshot_receipt};
                    REG_SNAPSHOT_SKIPPED_LOW: s_axi_rdata<=snapshot_skipped[31:0];
                    REG_SNAPSHOT_SKIPPED_HIGH: s_axi_rdata<=snapshot_skipped[63:32];
                    REG_SNAPSHOT_STATUS: s_axi_rdata<={31'b0,snapshot_sequence_exhausted};
                    REG_RESULT_DATA, REG_SNAPSHOT_DATA: begin
                        if (result_access || snapshot_access) begin
                            s_axi_rvalid<=0; read_owner<=result_access ? READ_RESULT : READ_SNAPSHOT;
                            read_current<=1; read_advance<=1;
                        end else if (s_axi_araddr==REG_RESULT_DATA)
                            result_receipt<=live ? RECEIPT_UNAVAILABLE : RECEIPT_EPOCH;
                        else snapshot_receipt<=live ? RECEIPT_UNAVAILABLE : RECEIPT_EPOCH;
                    end
                    default: s_axi_rresp<=SLVERR;
                endcase
            end
            if (write_owner!=WRITE_NONE) begin
                if (!write_current) begin respond_write(OKAY); refuse_write_epoch(); end
                else case (write_owner)
                    WRITE_COMMAND, WRITE_RESULT: if (host_response_valid) begin
                        respond_write(OKAY);
                        if (write_owner==WRITE_COMMAND) command_receipt<=host_receipt;
                        else result_receipt<=host_receipt;
                    end
                    WRITE_SNAPSHOT: if (snapshot_ack_response_valid) begin
                        respond_write(OKAY); snapshot_receipt<=snapshot_ack_receipt;
                    end
                    default: ;
                endcase
            end
            if (write_complete) begin
                if (!write_format) respond_write(SLVERR);
                else if (!write_current) begin respond_write(OKAY); refuse_write_epoch(); end
                else if (abort_access || recovery_access) begin
                    respond_write(OKAY);
                    submission_receipt<=write_data!=epoch ? RECEIPT_EPOCH
                        : abort_access || recovery_available ? RECEIPT_ACCEPTED : RECEIPT_UNAVAILABLE;
                end else if (command_request) begin
                    if (host_ready) begin
                        aw_pending<=0; w_pending<=0;
                        write_owner<=write_address==REG_RESULT_ACK ? WRITE_RESULT : WRITE_COMMAND;
                    end
                end else if (snapshot_request) begin
                    if (snapshot_ack_ready) begin aw_pending<=0; w_pending<=0; write_owner<=WRITE_SNAPSHOT; end
                end else begin
                    respond_write(OKAY);
                    case (write_address)
                        REG_COMMAND_BANK: command_bank<=write_data[1:0];
                        REG_RESULT_BANK, REG_RESULT_REWIND: begin
                            if (write_address==REG_RESULT_BANK) result_bank<=write_data[1:0];
                            result_cursor<=0;
                            if (read_owner==READ_RESULT || result_access) read_advance<=0;
                        end
                        REG_SNAPSHOT_BANK, REG_SNAPSHOT_REWIND: begin
                            if (write_address==REG_SNAPSHOT_BANK) snapshot_bank<=write_data[0];
                            snapshot_cursor<=0;
                            if (read_owner==READ_SNAPSHOT || snapshot_access) read_advance<=0;
                        end
                        REG_RESULT_ACK_LOW: result_ack_sequence[31:0]<=write_data;
                        REG_RESULT_ACK_HIGH: result_ack_sequence[63:32]<=write_data;
                        REG_RESULT_ACK_EPOCH: result_ack_epoch<=write_data;
                        REG_SNAPSHOT_ACK_LOW: saved_snapshot_sequence[31:0]<=write_data;
                        REG_SNAPSHOT_ACK_HIGH: saved_snapshot_sequence[63:32]<=write_data;
                        REG_SNAPSHOT_ACK_EPOCH: saved_snapshot_epoch<=write_data;
                        default: ;
                    endcase
                end
            end
            if (runtime_reset || epoch_changed) begin
                completed_quanta<=0; peak_quantum_cycles<=0;
                aw_current<=0; w_current<=0; read_current<=0; read_advance<=0;
                command_bank<=0; result_bank<=0; snapshot_bank<=0; result_cursor<=0; snapshot_cursor<=0;
                result_ack_sequence<=0; saved_snapshot_sequence<=0; result_ack_epoch<=0; saved_snapshot_epoch<=0;
                command_receipt<=RECEIPT_EPOCH; result_receipt<=RECEIPT_EPOCH; snapshot_receipt<=RECEIPT_EPOCH;
            end
        end
    end
endmodule
