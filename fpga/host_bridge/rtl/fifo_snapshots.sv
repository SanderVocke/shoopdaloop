module fifo_snapshots #(
    parameter integer LOOPS=128
) (
    input wire clk, reset, input wire [31:0] epoch, input wire fault,
    input wire capture_valid, output wire capture_ready, input wire capture_initial,
    input wire [63:0] capture_frame, capture_revision, input wire [31:0] capture_quantum,
    output wire memory_valid, input wire memory_ready, output wire [15:0] memory_address,
    input wire memory_response_valid, memory_response_error, input wire [128:0] memory_read_data,
    output wire metadata_valid, input wire metadata_ready, output wire [15:0] metadata_slot,
    input wire metadata_response_valid, metadata_response_error, input wire [128:0] metadata_read_data,
    output wire [1:0] ready_mask, free_mask,
    output wire [63:0] sequence0, sequence1, output reg [63:0] last_sequence, skipped,
    output wire sequence_exhausted, output reg capture_fault, output reg [31:0] peak_cycles,
    input wire read_valid, input wire read_bank, input wire [12:0] read_word,
    output reg response_valid, response_error, output wire [31:0] read_data,
    input wire ack_valid, output wire ack_ready, input wire ack_bank,
    input wire [31:0] ack_epoch, input wire [63:0] ack_sequence,
    output reg ack_response_valid, output reg [3:0] ack_receipt
);
    localparam integer WORDS=8+LOOPS*19;
    localparam integer ADDRESS_BITS=$clog2(WORDS*2);
    localparam [13:0] ACTIVE_LIMIT=14'd8189;
    localparam [1:0] FREE=0, FILLING=1, READY=2;
    typedef enum logic [3:0] {IDLE, LOOP_REQUEST, LOOP_WAIT, META_REQUEST, META_WAIT,
        META_WORD1, META_WORD2, HEADER, COMMIT, RELEASE} phase_t;
    initial if (LOOPS<1 || LOOPS>128) $fatal(1,"snapshot extent must fit 128 rows");
    (* ram_style="block" *) reg [63:0] packets [0:WORDS*2-1];
    reg [1:0] state [0:1];
    reg [63:0] bank_sequence [0:1], acknowledged [0:1];
    reg [31:0] incarnation, saved_epoch, saved_quantum;
    reg [63:0] saved_frame, saved_revision, saved_sequence, saved_skipped;
    reg saved_initial, bank;
    reg [6:0] row;
    reg [3:0] word_number;
    reg [2:0] header_word;
    reg [11:0] cursor;
    reg [128:0] metadata;
    reg [13:0] cycles;
    reg [63:0] read_latch;
    reg read_high;
    phase_t phase;
    wire active_epoch=epoch!=0 && epoch==incarnation;
    wire stopped=fault || capture_fault;
    wire running=phase!=IDLE && phase!=RELEASE;
    wire service=active_epoch && !reset && !stopped && capture_valid && cycles<ACTIVE_LIMIT;
    wire free_available=state[0]==FREE || state[1]==FREE;
    wire next_bank=state[0]!=FREE;
    wire read_allowed=active_epoch && state[read_bank]==READY && {19'b0,read_word}<32'(WORDS*2);
    wire [12:0] read_address=(read_bank ? 13'(WORDS) : 13'd0)+{1'b0,read_word[12:1]};
    reg [3:0] receipt;
    wire release_bank=ack_valid && ack_ready && receipt==0;
    reg write_enable;
    reg [11:0] write_offset;
    reg [63:0] write_data;
    wire [12:0] write_address=(bank ? 13'(WORDS) : 13'd0)+{1'b0,write_offset};
    assign sequence0=bank_sequence[0];
    assign sequence1=bank_sequence[1];
    assign sequence_exhausted=&last_sequence;
    assign capture_ready=!reset && (!active_epoch || stopped || phase==RELEASE);
    assign memory_valid=service && phase==LOOP_REQUEST;
    assign memory_address={5'b0,row,word_number};
    assign metadata_valid=service && phase==META_REQUEST;
    assign metadata_slot={9'b0,row};
    assign ack_ready=!reset;
    assign read_data=response_error ? 32'd0 : read_high ? read_latch[63:32] : read_latch[31:0];
    for (genvar g=0;g<2;g=g+1) begin: status
        assign ready_mask[g]=!reset && active_epoch && state[g]==READY;
        assign free_mask[g]=!reset && active_epoch && state[g]==FREE;
    end
    always @* begin
        receipt=0;
        if (!active_epoch || ack_epoch!=incarnation) receipt=2;
        else if (stopped) receipt=3;
        else if (ack_sequence==0 || ack_sequence>last_sequence) receipt=4;
        else if (ack_sequence==acknowledged[ack_bank]
            || (bank_sequence[ack_bank]!=0 && ack_sequence<bank_sequence[ack_bank])) receipt=6;
        else if (state[ack_bank]!=READY || ack_sequence!=bank_sequence[ack_bank]) receipt=5;
    end
    always @* begin
        write_enable=0; write_offset=cursor; write_data=0;
        if (service) case (phase)
            LOOP_WAIT: begin write_enable=memory_response_valid && !memory_response_error; write_data=memory_read_data[63:0]; end
            META_WAIT: begin write_enable=metadata_response_valid && !metadata_response_error; write_data=metadata_read_data[63:0]; end
            META_WORD1: begin write_enable=1; write_data=metadata[127:64]; end
            META_WORD2: begin write_enable=1; write_data={63'b0,metadata[128]}; end
            HEADER: begin
                write_enable=1; write_offset={9'b0,header_word};
                case (header_word)
                    0: write_data={16'(saved_initial ? 0 : 1),16'd16,32'h53465331};
                    1: write_data={saved_quantum,saved_epoch};
                    2: write_data=saved_sequence;
                    3: write_data=saved_frame;
                    4: write_data=saved_revision;
                    5: write_data={32'd19,32'(LOOPS)};
                    6: write_data=saved_skipped;
                    7: write_data=0;
                    default: ;
                endcase
            end
            default: ;
        endcase
    end
    always @(posedge clk) begin
        if (write_enable) packets[ADDRESS_BITS'(write_address)]<=write_data;
        if (!reset && read_valid && read_allowed) read_latch<=packets[ADDRESS_BITS'(read_address)];
    end
    always @(posedge clk) begin
        if (reset || epoch!=incarnation) begin
            incarnation<=epoch; phase<=capture_valid ? RELEASE : IDLE;
            last_sequence<=0; skipped<=0; capture_fault<=0; peak_cycles<=0; cycles<=0;
            bank<=0; row<=0; word_number<=0; cursor<=8; header_word<=1; metadata<=0;
            saved_epoch<=0; saved_quantum<=0; saved_frame<=0; saved_revision<=0;
            saved_sequence<=0; saved_skipped<=0; saved_initial<=0;
            response_valid<=read_valid && !reset; response_error<=1; read_high<=0;
            ack_response_valid<=ack_valid && !reset; ack_receipt<=2;
            for (integer i=0;i<2;i=i+1) begin state[i]<=FREE; bank_sequence[i]<=0; acknowledged[i]<=0; end
        end else begin
            response_valid<=read_valid;
            response_error<=read_valid && !read_allowed;
            if (read_valid) read_high<=read_word[0];
            ack_response_valid<=ack_valid;
            if (ack_valid) ack_receipt<=receipt;
            if (release_bank) begin state[ack_bank]<=FREE; acknowledged[ack_bank]<=ack_sequence; end
            if (running) begin
                cycles<=cycles+1'b1;
                // Count entry and the capture-ready handoff as part of the phase.
                if ({18'b0,cycles}+32'd3>peak_cycles) peak_cycles<={18'b0,cycles}+32'd3;
            end
            if (running && (stopped || !capture_valid || cycles>=ACTIVE_LIMIT)) begin
                if (!fault) capture_fault<=1;
                phase<=RELEASE;
            end else case (phase)
                IDLE: if (capture_valid && active_epoch && !stopped) begin
                    cycles<=0;
                    if (peak_cycles<2) peak_cycles<=2;
                    if (sequence_exhausted) phase<=RELEASE;
                    else begin
                        last_sequence<=last_sequence+1'b1;
                        if (!free_available) begin
                            if (!(&skipped)) skipped<=skipped+1'b1;
                            phase<=RELEASE;
                        end else begin
                            bank<=next_bank; state[next_bank]<=FILLING; bank_sequence[next_bank]<=last_sequence+1'b1;
                            saved_epoch<=epoch; saved_sequence<=last_sequence+1'b1; saved_skipped<=skipped;
                            saved_frame<=capture_frame; saved_revision<=capture_revision;
                            saved_quantum<=capture_quantum; saved_initial<=capture_initial;
                            row<=0; word_number<=0; cursor<=8; phase<=LOOP_REQUEST;
                        end
                    end
                end
                LOOP_REQUEST: if (memory_ready) phase<=LOOP_WAIT;
                LOOP_WAIT: if (memory_response_valid) begin
                    if (memory_response_error) begin capture_fault<=1; phase<=RELEASE; end
                    else begin
                        cursor<=cursor+1'b1;
                        if (word_number==15) phase<=META_REQUEST;
                        else begin word_number<=word_number+1'b1; phase<=LOOP_REQUEST; end
                    end
                end
                META_REQUEST: if (metadata_ready) phase<=META_WAIT;
                META_WAIT: if (metadata_response_valid) begin
                    if (metadata_response_error) begin capture_fault<=1; phase<=RELEASE; end
                    else begin metadata<=metadata_read_data; cursor<=cursor+1'b1; phase<=META_WORD1; end
                end
                META_WORD1: begin cursor<=cursor+1'b1; phase<=META_WORD2; end
                META_WORD2: begin
                    cursor<=cursor+1'b1;
                    if (row==7'(LOOPS-1)) begin header_word<=1; phase<=HEADER; end
                    else begin row<=row+1'b1; word_number<=0; phase<=LOOP_REQUEST; end
                end
                HEADER: begin
                    if (header_word==0) phase<=COMMIT;
                    else header_word<=header_word==7 ? 3'd0 : header_word+1'b1;
                end
                COMMIT: begin state[bank]<=READY; phase<=RELEASE; end
                RELEASE: if (!capture_valid) phase<=IDLE;
                default: begin capture_fault<=1; phase<=RELEASE; end
            endcase
        end
    end
endmodule
